use std::path::{Path, PathBuf};

use aws_sdk_s3::{primitives::ByteStream, Client as S3Client};
use bambu_slicer::{Slicer, SlicerConfig, SlicerStats};
use lambda_runtime::{tracing, Error, LambdaEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::TempDir;
use tokio::{fs, io::AsyncReadExt, sync::OnceCell};

#[derive(Debug, Deserialize)]
struct SliceRequest {
    /// Printer preset name (e.g., "Bambu Lab A1 0.4 nozzle")
    printer_preset: Option<String>,

    /// Filament preset name (e.g., "Bambu PLA Basic @BBL A1")
    filament_preset: Option<String>,

    /// Process preset name (e.g., "0.20mm Standard @BBL A1")
    process_preset: Option<String>,

    /// Custom parameters as key-value pairs
    custom_params: Option<Vec<(String, String)>>,
}

#[derive(Debug, Deserialize)]
struct JobPayload {
    #[serde(rename = "file", alias = "input")]
    input_path: String,

    #[serde(alias = "output")]
    output_path: String,

    config: Option<SliceRequest>,
}

#[derive(Debug, Serialize)]
struct SliceResponse {
    stats: SlicerStats,
    presets: Value,
    config: Value,
}

#[derive(Debug)]
struct SliceOutcome {
    stats: SlicerStats,
    presets: Value,
    config: Value,
}

static S3_CLIENT: OnceCell<S3Client> = OnceCell::const_new();

async fn s3_client() -> &'static S3Client {
    S3_CLIENT
        .get_or_init(|| async {
            let config = aws_config::load_from_env().await;
            S3Client::new(&config)
        })
        .await
}

fn parse_s3_location(location: &str) -> Result<(String, String), Error> {
    let trimmed = location.trim();
    if let Some(stripped) = trimmed.strip_prefix("s3://") {
        let mut parts = stripped.splitn(2, '/');
        let bucket = parts
            .next()
            .ok_or_else(|| "S3 location missing bucket".to_string())?;
        let key = parts
            .next()
            .ok_or_else(|| "S3 location missing key".to_string())?;
        if bucket.is_empty() || key.is_empty() {
            return Err("S3 location must include bucket and key".into());
        }
        return Ok((bucket.to_string(), key.to_string()));
    }

    let bucket = std::env::var("S3_BUCKET")
        .map_err(|_| "S3_BUCKET env var must be set when using non-s3:// paths".to_string())?;
    Ok((bucket, trimmed.to_string()))
}

fn normalize_output_prefix(output_path: &str) -> Result<(String, String), Error> {
    let (bucket, key) = parse_s3_location(output_path)?;
    let prefix = key.trim_end_matches('/');
    if prefix.is_empty() {
        return Err("output_path must not be empty".into());
    }
    Ok((bucket, prefix.to_string()))
}

async fn download_input(
    client: &S3Client,
    bucket: &str,
    key: &str,
    dir: &Path,
) -> Result<PathBuf, Error> {
    let file_name = Path::new(key)
        .file_name()
        .ok_or_else(|| "S3 key must contain a file name".to_string())?
        .to_string_lossy()
        .to_string();
    let local_path = dir.join(file_name);

    tracing::info!("Downloading s3://{}/{} to {:?}", bucket, key, local_path);
    let object = client.get_object().bucket(bucket).key(key).send().await?;

    let mut reader = object.body.into_async_read();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await?;
    fs::write(&local_path, &buf).await?;

    Ok(local_path)
}

async fn upload_output(
    client: &S3Client,
    bucket: &str,
    key: &str,
    path: &Path,
) -> Result<(), Error> {
    tracing::info!("Uploading {:?} to s3://{}/{}", path, bucket, key);
    let bytes = fs::read(path).await?;
    let body = ByteStream::from(bytes);

    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await?;
    Ok(())
}

fn slice_with_presets(
    model_path: &Path,
    output_path: &Path,
    config: &SliceRequest,
) -> Result<SliceOutcome, Error> {
    let mut slicer = Slicer::new()?;

    slicer.load_model(model_path)?;

    let slicer_config = SlicerConfig {
        printer_preset: config.printer_preset.clone(),
        filament_preset: config.filament_preset.clone(),
        process_preset: config.process_preset.clone(),
        custom_config_json: None,
    };
    slicer.load_preset(&slicer_config)?;

    slicer.slice()?;
    slicer.export_gcode(output_path)?;

    let stats = slicer.get_stats()?;
    let presets = parse_json_value(slicer.get_preset_info_json()?, "preset info")?;
    let config = parse_json_value(slicer.get_config_json()?, "config")?;

    Ok(SliceOutcome {
        stats,
        presets,
        config,
    })
}

fn slice_with_custom_params(
    model_path: &Path,
    output_path: &Path,
    config: &SliceRequest,
) -> Result<SliceOutcome, Error> {
    let mut slicer = Slicer::new()?;

    slicer.load_model(model_path)?;

    if config.printer_preset.is_some()
        || config.filament_preset.is_some()
        || config.process_preset.is_some()
    {
        let preset_config = SlicerConfig {
            printer_preset: config.printer_preset.clone(),
            filament_preset: config.filament_preset.clone(),
            process_preset: config.process_preset.clone(),
            custom_config_json: None,
        };
        slicer.load_preset(&preset_config)?;
    }

    if let Some(params) = &config.custom_params {
        for (key, value) in params {
            slicer.set_config_param(key, value)?;
        }
    }

    slicer.slice()?;
    slicer.export_gcode(output_path)?;

    let stats = slicer.get_stats()?;
    let presets = parse_json_value(slicer.get_preset_info_json()?, "preset info")?;
    let config = parse_json_value(slicer.get_config_json()?, "config")?;

    Ok(SliceOutcome {
        stats,
        presets,
        config,
    })
}

fn parse_json_value(json_str: String, label: &str) -> Result<Value, Error> {
    serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse {} JSON: {}", label, e).into())
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

async fn ensure_resources_in_tmp() -> Result<(), Error> {
    let src = Path::new("/app/resources");
    let dst = Path::new("/tmp/resources");

    // Only copy if destination is missing or empty.
    let should_copy = match std::fs::read_dir(dst) {
        Ok(mut iter) => iter.next().is_none(),
        Err(_) => true,
    };

    if should_copy {
        let src = src.to_path_buf();
        let dst = dst.to_path_buf();
        tokio::task::spawn_blocking(move || copy_dir_all(&src, &dst))
            .await?
            .map_err(|e| format!("Failed to copy resources to /tmp: {}", e))?;
    }
    Ok(())
}

pub(crate) async fn function_handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
    let payload = event.payload;
    tracing::info!("Payload: {:?}", payload);

    ensure_resources_in_tmp().await?;

    let job: JobPayload = serde_json::from_value(payload)?;
    let client = s3_client().await;

    let (input_bucket, input_key) = parse_s3_location(&job.input_path)?;
    let (output_bucket, output_prefix) = normalize_output_prefix(&job.output_path)?;

    let temp_dir = TempDir::new()?;
    let input_path = download_input(client, &input_bucket, &input_key, temp_dir.path()).await?;
    let output_gcode_path = temp_dir.path().join("output.gcode");
    let metadata_json_path = temp_dir.path().join("metadata.json");

    let config = job.config.unwrap_or(SliceRequest {
        printer_preset: Some("Bambu Lab A1 0.4 nozzle".to_string()),
        filament_preset: Some("Bambu PLA Basic @BBL A1".to_string()),
        process_preset: Some("0.20mm Standard @BBL A1".to_string()),
        custom_params: None,
    });

    let outcome = if config.custom_params.is_some() {
        slice_with_custom_params(&input_path, &output_gcode_path, &config)?
    } else {
        slice_with_presets(&input_path, &output_gcode_path, &config)?
    };

    let response = SliceResponse {
        stats: outcome.stats,
        presets: outcome.presets,
        config: outcome.config,
    };

    let response_json = serde_json::to_vec_pretty(&response)?;
    fs::write(&metadata_json_path, response_json).await?;

    let gcode_key = format!("{}/output.gcode", output_prefix);
    let metadata_key = format!("{}/metadata.json", output_prefix);

    upload_output(client, &output_bucket, &gcode_key, &output_gcode_path).await?;
    upload_output(client, &output_bucket, &metadata_key, &metadata_json_path).await?;

    Ok(serde_json::to_value(response)?)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use lambda_runtime::{Context, LambdaEvent};
//     use serde_json::json;

//     #[tokio::test]
//     async fn test_payload_deserialize() {
//         let event = LambdaEvent::new(
//             json!({
//                 "file": "s3://my-bucket/input.stl",
//                 "output_path": "s3://my-bucket/outputs/job-1",
//                 "config": {
//                     "printer_preset": "Bambu Lab A1 0.4 nozzle",
//                     "filament_preset": "Bambu PLA Basic @BBL A1",
//                     "process_preset": "0.20mm Standard @BBL A1",
//                     "custom_params": null
//                 }
//             }),
//             Context::default(),
//         );
//         let payload: JobPayload = serde_json::from_value(event.payload).unwrap();
//         assert_eq!(payload.input_path, "s3://my-bucket/input.stl");
//     }
// }
