//! Error types for the BambuSlicer library

use thiserror::Error;

/// Errors that can occur when using the slicer
#[derive(Error, Debug)]
pub enum SlicerError {
    #[error("Null context pointer")]
    NullContext,

    #[error("Null parameter provided")]
    NullParameter,

    #[error("Failed to load model: {0}")]
    ModelLoad(String),

    #[error("Failed to parse configuration: {0}")]
    ConfigParse(String),

    #[error("Preset not found: {0}")]
    PresetNotFound(String),

    #[error("No model loaded")]
    NoModel,

    #[error("No configuration loaded")]
    NoConfig,

    #[error("Slicing process failed: {0}")]
    ProcessFailed(String),

    #[error("G-code export failed: {0}")]
    ExportFailed(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid UTF-8 in string")]
    InvalidUtf8(#[from] std::ffi::NulError),

    #[error("Unknown error code: {0}")]
    Unknown(i32),
}

impl SlicerError {
    /// Convert from C API error code and optional error message
    pub(crate) fn from_code(code: i32, message: Option<String>) -> Self {
        let msg = message.unwrap_or_else(|| "No error message available".to_string());

        match code {
            1 => SlicerError::NullContext,
            2 => SlicerError::NullParameter,
            3 => SlicerError::ModelLoad(msg),
            4 => SlicerError::ConfigParse(msg),
            5 => SlicerError::PresetNotFound(msg),
            6 => SlicerError::NoModel,
            7 => SlicerError::NoConfig,
            8 => SlicerError::ProcessFailed(msg),
            9 => SlicerError::ExportFailed(msg),
            10 => SlicerError::Io(std::io::Error::new(std::io::ErrorKind::Other, msg)),
            99 => SlicerError::Internal(msg),
            _ => SlicerError::Unknown(code),
        }
    }
}

/// Result type alias for slicer operations
pub type Result<T> = std::result::Result<T, SlicerError>;
