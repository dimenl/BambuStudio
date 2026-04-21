use std::path::{Path, PathBuf};

use aws_sdk_s3::{Client as S3Client, primitives::ByteStream};
use axum::{Json, http::StatusCode};
use bambu_slicer::{Slicer, SlicerConfig};
use serde_json::Value;
use tempfile::TempDir;
use tokio::{fs, io::AsyncReadExt, sync::OnceCell};
use tracing::{error, info};

use crate::types::{
    error_types::AppError,
    slicer_types::{SliceConfigRequest, SliceRouteRequest, SliceRouteResponse},
};

#[derive(Debug)]
struct SliceOutcome {
    stats: bambu_slicer::SlicerStats,
    presets: Value,
    config: Value,
}

static S3_CLIENT: OnceCell<S3Client> = OnceCell::const_new();

pub async fn slice(
    Json(payload): Json<SliceRouteRequest>,
) -> Result<Json<SliceRouteResponse>, AppError> {
    ensure_resources_in_tmp().await?;

    let client = s3_client().await;
    let (input_bucket, input_key) = parse_s3_location(&payload.input_path)?;
    let (output_bucket, output_prefix) = normalize_output_prefix(&payload.output_path)?;

    let temp_dir = TempDir::new().map_err(internal_error)?;
    let input_path = ensure_local_input(client, &input_bucket, &input_key, temp_dir.path()).await?;
    let output_gcode_path = temp_dir.path().join("output.gcode");
    let metadata_json_path = temp_dir.path().join("metadata.json");
    let config = payload.config.unwrap_or_default();

    let outcome = if config.custom_params.is_some() {
        slice_with_custom_params(&input_path, &output_gcode_path, &config)?
    } else {
        slice_with_presets(&input_path, &output_gcode_path, &config)?
    };

    let gcode_base64 = if config.gcode_needed {
        let gcode_bytes = fs::read(&output_gcode_path).await.map_err(internal_error)?;
        Some(base64_encode(&gcode_bytes))
    } else {
        None
    };

    let response = SliceRouteResponse {
        stats: outcome.stats,
        presets: outcome.presets,
        config: outcome.config,
        gcode: gcode_base64,
    };

    let response_json = serde_json::to_vec_pretty(&response).map_err(internal_error)?;
    fs::write(&metadata_json_path, response_json)
        .await
        .map_err(internal_error)?;

    let gcode_key = format!("{}/output.gcode", output_prefix);
    let metadata_key = format!("{}/metadata.json", output_prefix);

    upload_output(client, &output_bucket, &gcode_key, &output_gcode_path).await?;
    upload_output(client, &output_bucket, &metadata_key, &metadata_json_path).await?;

    Ok(Json(response))
}

async fn s3_client() -> &'static S3Client {
    S3_CLIENT
        .get_or_init(|| async {
            let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            S3Client::new(&config)
        })
        .await
}

fn parse_s3_location(location: &str) -> Result<(String, String), AppError> {
    let trimmed = location.trim();
    let Some(stripped) = trimmed.strip_prefix("s3://") else {
        return Err(bad_request(
            "input_path and output_path must start with s3://",
        ));
    };

    let mut parts = stripped.splitn(2, '/');
    let bucket = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("S3 path is missing bucket"))?;
    let key = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("S3 path is missing key"))?;

    Ok((bucket.to_string(), key.to_string()))
}

fn normalize_output_prefix(output_path: &str) -> Result<(String, String), AppError> {
    let (bucket, key) = parse_s3_location(output_path)?;
    let prefix = key.trim_end_matches('/');
    if prefix.is_empty() {
        return Err(bad_request("output_path must not be empty"));
    }
    Ok((bucket, prefix.to_string()))
}

async fn ensure_local_input(
    client: &S3Client,
    bucket: &str,
    key: &str,
    temp_dir: &Path,
) -> Result<PathBuf, AppError> {
    let file_name = Path::new(key)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("input_path must include a file name"))?;

    let cache_dir = Path::new("/tmp/models");
    fs::create_dir_all(cache_dir)
        .await
        .map_err(internal_error)?;

    let cached_path = cache_dir.join(file_name);
    if fs::try_exists(&cached_path).await.map_err(internal_error)? {
        info!("Using cached model at {:?}", cached_path);
        return Ok(cached_path);
    }

    download_input(client, bucket, key, temp_dir, file_name).await
}

async fn download_input(
    client: &S3Client,
    bucket: &str,
    key: &str,
    temp_dir: &Path,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    let temp_path = temp_dir.join(file_name);
    let cache_path = Path::new("/tmp/models").join(file_name);

    info!("Downloading s3://{bucket}/{key} to {:?}", cache_path);
    let object = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(internal_error)?;

    let mut reader = object.body.into_async_read();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.map_err(internal_error)?;

    fs::write(&temp_path, &buf).await.map_err(internal_error)?;
    fs::write(&cache_path, &buf).await.map_err(internal_error)?;

    Ok(cache_path)
}

async fn upload_output(
    client: &S3Client,
    bucket: &str,
    key: &str,
    path: &Path,
) -> Result<(), AppError> {
    info!("Uploading {:?} to s3://{bucket}/{key}", path);
    let bytes = fs::read(path).await.map_err(internal_error)?;
    let body = ByteStream::from(bytes);

    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await
        .map_err(internal_error)?;
    Ok(())
}

fn slice_with_presets(
    model_path: &Path,
    output_path: &Path,
    config: &SliceConfigRequest,
) -> Result<SliceOutcome, AppError> {
    let mut slicer = Slicer::new().map_err(internal_error)?;
    slicer.load_model(model_path).map_err(internal_error)?;

    let slicer_config = build_slicer_config(config);
    slicer.load_preset(&slicer_config).map_err(internal_error)?;

    if let Some((x, y, z)) = config.rotation {
        slicer.rotate_model(x, y, z).map_err(internal_error)?;
    }

    slicer.slice().map_err(internal_error)?;
    slicer.export_gcode(output_path).map_err(internal_error)?;

    build_outcome(&slicer)
}

fn slice_with_custom_params(
    model_path: &Path,
    output_path: &Path,
    config: &SliceConfigRequest,
) -> Result<SliceOutcome, AppError> {
    let mut slicer = Slicer::new().map_err(internal_error)?;
    slicer.load_model(model_path).map_err(internal_error)?;

    if config.printer_preset.is_some()
        || config.filament_preset.is_some()
        || config.process_preset.is_some()
        || config.custom_config_json.is_some()
    {
        let slicer_config = build_slicer_config(config);
        slicer.load_preset(&slicer_config).map_err(internal_error)?;
    }

    if let Some(params) = &config.custom_params {
        for (key, value) in params {
            slicer
                .set_config_param(key, value)
                .map_err(internal_error)?;
        }
    }

    if let Some((x, y, z)) = config.rotation {
        slicer.rotate_model(x, y, z).map_err(internal_error)?;
    }

    slicer.slice().map_err(internal_error)?;
    slicer.export_gcode(output_path).map_err(internal_error)?;

    build_outcome(&slicer)
}

fn build_slicer_config(config: &SliceConfigRequest) -> SlicerConfig {
    let custom_config_json = config.custom_config_json.as_ref().map(|value| match value {
        Value::String(raw) => raw.clone(),
        _ => value.to_string(),
    });

    SlicerConfig {
        printer_preset: config.printer_preset.clone(),
        filament_preset: config.filament_preset.clone(),
        process_preset: config.process_preset.clone(),
        custom_config_json,
        rotation: config.rotation,
    }
}

fn build_outcome(slicer: &Slicer) -> Result<SliceOutcome, AppError> {
    let stats = slicer.get_stats().map_err(internal_error)?;
    let presets = parse_json_value(
        slicer.get_preset_info_json().map_err(internal_error)?,
        "preset info",
    )?;
    let config = parse_json_value(slicer.get_config_json().map_err(internal_error)?, "config")?;

    Ok(SliceOutcome {
        stats,
        presets,
        config,
    })
}

fn parse_json_value(json_str: String, label: &str) -> Result<Value, AppError> {
    serde_json::from_str(&json_str)
        .map_err(|err| internal_error(format!("failed to parse {label} JSON: {err}")))
}

fn copy_dir_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if !from.is_dir() {
        return Ok(());
    }

    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_dir_all(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }

    Ok(())
}

async fn ensure_resources_in_tmp() -> Result<(), AppError> {
    let src = Path::new("/app/resources");
    let dst = Path::new("/tmp/resources");

    let should_copy = match std::fs::read_dir(dst) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    };

    if should_copy {
        let src = src.to_path_buf();
        let dst = dst.to_path_buf();
        tokio::task::spawn_blocking(move || copy_dir_all(&src, &dst))
            .await
            .map_err(internal_error)?
            .map_err(internal_error)?;
    }

    Ok(())
}

fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;

    let mut output = Vec::new();
    {
        let mut encoder = base64::write::EncoderWriter::new(
            &mut output,
            &base64::engine::general_purpose::STANDARD,
        );
        encoder
            .write_all(data)
            .expect("base64 encoder write to Vec should not fail");
    }
    String::from_utf8(output).expect("base64 output should be valid UTF-8")
}

fn bad_request(message: impl Into<String>) -> AppError {
    AppError::Message {
        status_code: StatusCode::BAD_REQUEST,
        error_message: message.into(),
        user_message: None,
    }
}

fn internal_error(error: impl std::fmt::Display) -> AppError {
    let message = error.to_string();
    error!("{message}");
    AppError::InternalServerError(message)
}
