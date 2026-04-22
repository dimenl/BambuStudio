use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use aws_config::{BehaviorVersion, Region, SdkConfig};
use aws_credential_types::{
    Credentials,
    provider::{self, ProvideCredentials, SharedCredentialsProvider, future},
};
use aws_sdk_s3::{Client as S3Client, primitives::ByteStream};
use aws_sdk_sts::Client as StsClient;
use axum::{Json, http::StatusCode};
use bambu_slicer::{Slicer, SlicerConfig};
use serde_json::Value;
use tempfile::TempDir;
use tokio::{fs, io::AsyncReadExt, sync::OnceCell};
use tracing::{error, info, warn};

use crate::types::{
    error_types::AppError,
    slicer_types::{
        LoadModelRequest, LoadModelResponse, SliceConfigRequest, SliceRouteRequest,
        SliceRouteResponse,
    },
};

#[derive(Debug)]
struct SliceOutcome {
    stats: bambu_slicer::SlicerStats,
    presets: Value,
    config: Value,
}

static S3_CLIENT: OnceCell<S3Client> = OnceCell::const_new();

const STS_SESSION_DURATION_SECONDS: i32 = 43_200;

#[derive(Debug, Clone)]
struct StsSessionTokenProvider {
    sts_client: StsClient,
    duration_seconds: i32,
}

impl StsSessionTokenProvider {
    async fn load_credentials(&self) -> provider::Result {
        let response = self
            .sts_client
            .get_session_token()
            .duration_seconds(self.duration_seconds)
            .send()
            .await
            .map_err(|err| {
                provider::error::CredentialsError::provider_error(format!(
                    "failed to refresh AWS session token via STS GetSessionToken: {err:?}"
                ))
            })?;

        let credentials = response.credentials().ok_or_else(|| {
            provider::error::CredentialsError::provider_error(
                "STS GetSessionToken returned no credentials",
            )
        })?;

        let expiration = Some(
            SystemTime::try_from(credentials.expiration().clone()).map_err(|err| {
                provider::error::CredentialsError::provider_error(format!(
                    "failed to convert STS credential expiration to SystemTime: {err}"
                ))
            })?,
        );

        Ok(Credentials::new(
            credentials.access_key_id(),
            credentials.secret_access_key(),
            Some(credentials.session_token().to_string()),
            expiration,
            "smart-slicer-sts-get-session-token",
        ))
    }
}

impl ProvideCredentials for StsSessionTokenProvider {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(self.load_credentials())
    }
}

pub async fn slice(
    Json(payload): Json<SliceRouteRequest>,
) -> Result<Json<SliceRouteResponse>, AppError> {
    ensure_resources_in_tmp().await?;

    let client = s3_client().await;
    let (input_bucket, input_key) = parse_s3_location(&payload.input_path)?;
    let (output_bucket, output_prefix) = normalize_output_prefix(&payload.output_path)?;

    let temp_dir = TempDir::new().map_err(internal_error)?;
    let (input_path, _) = ensure_local_input(client, &input_bucket, &input_key).await?;
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

pub async fn load_model(
    Json(payload): Json<LoadModelRequest>,
) -> Result<Json<LoadModelResponse>, AppError> {
    let client = s3_client().await;
    let (input_bucket, input_key) = parse_s3_location(&payload.input_path)?;
    let (local_path, cached) = ensure_local_input(client, &input_bucket, &input_key).await?;

    Ok(Json(LoadModelResponse {
        local_path: local_path.display().to_string(),
        cached,
    }))
}

async fn s3_client() -> &'static S3Client {
    S3_CLIENT
        .get_or_init(|| async {
            let config = load_aws_sdk_config().await;
            S3Client::new(&config)
        })
        .await
}

async fn load_aws_sdk_config() -> SdkConfig {
    let region = read_non_empty_env("AWS_REGION")
        .or_else(|| read_non_empty_env("AWS_DEFAULT_REGION"))
        .map(Region::new);
    let access_key_id = read_non_empty_env("AWS_ACCESS_KEY_ID");
    let secret_access_key = read_non_empty_env("AWS_SECRET_ACCESS_KEY");

    match (access_key_id, secret_access_key) {
        (Some(access_key_id), Some(secret_access_key)) if access_key_id.starts_with("AKIA") => {
            info!("Using long-term IAM access keys with auto-refreshing STS session credentials");

            let base_credentials = Credentials::new(
                access_key_id,
                secret_access_key,
                None,
                None,
                "smart-slicer-static-env",
            );

            let mut base_loader = aws_config::defaults(BehaviorVersion::latest())
                .credentials_provider(base_credentials);
            if let Some(region) = region.clone() {
                base_loader = base_loader.region(region);
            }
            let base_config = base_loader.load().await;

            let provider = SharedCredentialsProvider::new(StsSessionTokenProvider {
                sts_client: StsClient::new(&base_config),
                duration_seconds: STS_SESSION_DURATION_SECONDS,
            });

            let mut final_loader =
                aws_config::defaults(BehaviorVersion::latest()).credentials_provider(provider);
            if let Some(region) = region {
                final_loader = final_loader.region(region);
            }
            final_loader.load().await
        }
        (Some(access_key_id), Some(_)) if access_key_id.starts_with("ASIA") => {
            warn!(
                "AWS_ACCESS_KEY_ID starts with ASIA, which means temporary STS credentials. They cannot be auto-refreshed from only AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY. Provide long-term IAM user keys (AKIA...) or use an IAM role-based provider."
            );
            aws_config::load_defaults(BehaviorVersion::latest()).await
        }
        _ => aws_config::load_defaults(BehaviorVersion::latest()).await,
    }
}

fn read_non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
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
) -> Result<(PathBuf, bool), AppError> {
    validate_s3_key_has_file_name(key)?;

    let cached_path = cache_path_for_s3_object(bucket, key);
    if fs::try_exists(&cached_path).await.map_err(internal_error)? {
        info!("Using cached model at {:?}", cached_path);
        return Ok((cached_path, true));
    }

    download_input(client, bucket, key).await
}

fn validate_s3_key_has_file_name(key: &str) -> Result<(), AppError> {
    Path::new(key)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("input_path must include a file name"))?;
    Ok(())
}

fn cache_path_for_s3_object(bucket: &str, key: &str) -> PathBuf {
    Path::new("/tmp/models").join(bucket).join(key)
}

async fn download_input(
    client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<(PathBuf, bool), AppError> {
    let file_name = Path::new(key)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("input_path must include a file name"))?;
    let cache_path = cache_path_for_s3_object(bucket, key);
    let cache_parent = cache_path
        .parent()
        .ok_or_else(|| internal_error("cache path is missing parent directory"))?;
    let temp_path = cache_parent.join(format!("{file_name}.part"));

    fs::create_dir_all(cache_parent)
        .await
        .map_err(internal_error)?;

    info!("Downloading s3://{bucket}/{key} to {:?}", cache_path);
    let object = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|err| {
            internal_error(format!("failed to download s3://{bucket}/{key}: {err:?}"))
        })?;

    let mut reader = object.body.into_async_read();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.map_err(|err| {
        internal_error(format!(
            "failed to read S3 body for s3://{bucket}/{key}: {err}"
        ))
    })?;

    fs::write(&temp_path, &buf).await.map_err(|err| {
        internal_error(format!("failed to write temp file {:?}: {err}", temp_path))
    })?;
    fs::rename(&temp_path, &cache_path).await.map_err(|err| {
        internal_error(format!(
            "failed to move temp file {:?} to {:?}: {err}",
            temp_path, cache_path
        ))
    })?;

    Ok((cache_path, false))
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
        .map_err(|err| internal_error(format!("failed to upload s3://{bucket}/{key}: {err:?}")))?;
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
