use axum::{
    extract::{DefaultBodyLimit, Multipart},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use bambu_slicer::{slice_model, Slicer, SlicerConfig, SlicerStats};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};
use uuid::Uuid;

/// API error response
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    details: Option<String>,
}

/// Slice request configuration
#[derive(Debug, Deserialize)]
struct SliceRequest {
    /// Printer preset name (e.g., "Bambu Lab A1")
    printer_preset: Option<String>,
    
    /// Filament preset name (e.g., "Bambu PLA Basic @BBL A1")
    filament_preset: Option<String>,
    
    /// Process preset name (e.g., "0.20mm Standard @BBL A1")
    process_preset: Option<String>,
    
    /// Custom parameters as key-value pairs
    custom_params: Option<Vec<(String, String)>>,
}

/// Slice response
#[derive(Debug, Serialize)]
struct SliceResponse {
    /// Unique ID for this slicing job
    job_id: String,
    
    /// Print statistics
    stats: SlicerStats,
    
    /// Base64-encoded G-code content
    gcode: String,
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    bambu_version: String,
}

/// Custom error type
enum AppError {
    SlicerError(bambu_slicer::SlicerError),
    IoError(std::io::Error),
    InvalidRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message, details) = match self {
            AppError::SlicerError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Slicing failed".to_string(),
                Some(e.to_string()),
            ),
            AppError::IoError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "I/O error".to_string(),
                Some(e.to_string()),
            ),
            AppError::InvalidRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "Invalid request".to_string(),
                Some(msg),
            ),
        };

        let error_response = ErrorResponse {
            error: error_message,
            details,
        };

        (status, Json(error_response)).into_response()
    }
}

impl From<bambu_slicer::SlicerError> for AppError {
    fn from(err: bambu_slicer::SlicerError) -> Self {
        AppError::SlicerError(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::IoError(err)
    }
}

/// Health check endpoint
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: bambu_slicer::get_version(),
        bambu_version: bambu_slicer::get_bambu_version(),
    })
}

/// Main slicing endpoint
/// 
/// Accepts multipart form data with:
/// - `model`: STL/3MF/AMF/OBJ file
/// - `config`: JSON configuration (optional)
async fn slice(mut multipart: Multipart) -> Result<Json<SliceResponse>, AppError> {
    let job_id = Uuid::new_v4().to_string();
    info!("Starting slicing job: {}", job_id);

    // Create temporary directory for this job
    let temp_dir = TempDir::new()?;
    let mut model_path: Option<PathBuf> = None;
    let mut config: Option<SliceRequest> = None;

    // Parse multipart form
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::InvalidRequest(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "model" => {
                // Get filename
                let filename = field
                    .file_name()
                    .ok_or_else(|| AppError::InvalidRequest("Model file must have a filename".to_string()))?
                    .to_string();

                // Verify extension
                let extension = Path::new(&filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .ok_or_else(|| AppError::InvalidRequest("Invalid file extension".to_string()))?;

                if !["stl", "3mf", "amf", "obj"].contains(&extension.to_lowercase().as_str()) {
                    return Err(AppError::InvalidRequest(
                        "Unsupported file format. Use STL, 3MF, AMF, or OBJ".to_string(),
                    ));
                }

                // Save file
                let file_path = temp_dir.path().join(&filename);
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::InvalidRequest(e.to_string()))?;
                
                std::fs::write(&file_path, data)?;
                info!("Saved model file: {}", file_path.display());
                model_path = Some(file_path);
            }
            "config" => {
                let data = field
                    .text()
                    .await
                    .map_err(|e| AppError::InvalidRequest(e.to_string()))?;
                
                config = Some(
                    serde_json::from_str(&data)
                        .map_err(|e| AppError::InvalidRequest(format!("Invalid config JSON: {}", e)))?,
                );
                info!("Loaded configuration");
            }
            _ => {
                info!("Ignoring unknown field: {}", name);
            }
        }
    }

    // Verify model was provided
    let model_path = model_path
        .ok_or_else(|| AppError::InvalidRequest("No model file provided".to_string()))?;

    // Use default A1 config if none provided
    let config = config.unwrap_or(SliceRequest {
        printer_preset: Some("Bambu Lab A1".to_string()),
        filament_preset: Some("Bambu PLA Basic @BBL A1".to_string()),
        process_preset: Some("0.20mm Standard @BBL A1".to_string()),
        custom_params: None,
    });

    // Perform slicing
    info!("Starting slicing process");
    let output_path = temp_dir.path().join("output.gcode");

    let stats = if config.custom_params.is_some() {
        // Use builder API for custom parameters
        slice_with_custom_params(&model_path, &output_path, &config)?
    } else {
        // Use simple API for presets
        slice_with_presets(&model_path, &output_path, &config)?
    };

    info!("Slicing completed successfully");
    info!("Stats: time={}, filament={:.2}mm, weight={:.2}g",
        stats.estimated_print_time,
        stats.total_used_filament,
        stats.total_weight
    );

    // Read G-code and encode as base64
    let gcode_bytes = std::fs::read(&output_path)?;
    let gcode_base64 = base64_encode(&gcode_bytes);

    Ok(Json(SliceResponse {
        job_id,
        stats,
        gcode: gcode_base64,
    }))
}

/// Slice using preset-based configuration (simple API)
fn slice_with_presets(
    model_path: &Path,
    output_path: &Path,
    config: &SliceRequest,
) -> Result<SlicerStats, AppError> {
    let slicer_config = SlicerConfig {
        printer_preset: config.printer_preset.clone(),
        filament_preset: config.filament_preset.clone(),
        process_preset: config.process_preset.clone(),
        custom_config_json: None,
    };

    slice_model(model_path, &slicer_config, output_path).map_err(Into::into)
}

/// Slice using custom parameters (builder API)
fn slice_with_custom_params(
    model_path: &Path,
    output_path: &Path,
    config: &SliceRequest,
) -> Result<SlicerStats, AppError> {
    let mut slicer = Slicer::new()?;

    // Load model
    slicer.load_model(model_path)?;

    // Load presets if provided
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

    // Apply custom parameters
    if let Some(params) = &config.custom_params {
        for (key, value) in params {
            slicer.set_config_param(key, value)?;
        }
    }

    // Slice
    let stats = slicer.slice()?;

    // Export
    slicer.export_gcode(output_path)?;

    Ok(stats)
}

/// Simple base64 encoding
fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut output = Vec::new();
    {
        let mut encoder = base64::write::EncoderWriter::new(&mut output, &base64::engine::general_purpose::STANDARD);
        encoder.write_all(data).unwrap();
    }
    String::from_utf8(output).unwrap()
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "slicer_service=info,tower_http=debug".into()),
        )
        .init();

    info!("BambuSlicer Service starting...");
    info!("Version: {}", bambu_slicer::get_version());
    info!("BambuStudio: {}", bambu_slicer::get_bambu_version());

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/slice", post(slice))
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024)) // 100MB max
        .layer(tower_http::trace::TraceLayer::new_for_http());

    // Start server
    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    info!("Server listening on http://{}", addr);
    info!("Endpoints:");
    info!("  GET  /health - Health check");
    info!("  POST /slice  - Slice a model");

    axum::serve(listener, app)
        .await
        .expect("Server failed");
}
