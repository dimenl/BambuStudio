//! BambuSlicer - Rust FFI bindings for BambuStudio slicer core
//!
//! This crate provides safe Rust bindings to the BambuStudio slicer,
//! allowing you to slice 3D models into G-code from Rust applications.
//!
//! # Features
//!
//! - Load 3D models (STL, 3MF, AMF, OBJ)
//! - Configure slicing parameters
//! - Generate G-code
//! - Retrieve print statistics (time, filament usage, etc.)
//!
//! # Examples
//!
//! ## Simple API
//!
//! ```no_run
//! use bambu_slicer::{slice_model, SlicerConfig};
//! use std::path::Path;
//!
//! let config = SlicerConfig {
//!     printer_preset: Some("Bambu Lab A1".to_string()),
//!     filament_preset: Some("Bambu PLA Basic @BBL A1".to_string()),
//!     process_preset: Some("0.20mm Standard @BBL A1".to_string()),
//!     custom_config_json: None,
//! };
//!
//! let stats = slice_model(
//!     Path::new("model.3mf"),
//!     &config,
//!     Path::new("output.gcode"),
//! ).expect("Slicing failed");
//!
//! println!("Print time: {}", stats.estimated_print_time);
//! println!("Filament used: {:.2}mm", stats.total_used_filament);
//! ```
//!
//! ## Builder API
//!
//! ```no_run
//! use bambu_slicer::Slicer;
//! use std::path::Path;
//!
//! let mut slicer = Slicer::new().expect("Failed to create slicer");
//!
//! slicer
//!     .load_model(Path::new("model.stl"))
//!     .expect("Failed to load model");
//!
//! slicer
//!     .set_config_param("layer_height", "0.2")
//!     .expect("Failed to set config");
//!
//! let stats = slicer.slice().expect("Slicing failed");
//!
//! slicer
//!     .export_gcode(Path::new("output.gcode"))
//!     .expect("Failed to export");
//! ```

mod error;
mod ffi;

pub use error::{Result, SlicerError};

use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;

// Success code from C API (defined here to avoid FFI dependency before build)
const SLICER_SUCCESS: i32 = 0;

/// Configuration for the slicer
#[derive(Debug, Clone, Default)]
pub struct SlicerConfig {
    /// Printer preset name (e.g., "Bambu Lab A1")
    pub printer_preset: Option<String>,

    /// Filament preset name (e.g., "Bambu PLA Basic @BBL A1")  
    pub filament_preset: Option<String>,

    /// Process preset name (e.g., "0.20mm Standard @BBL A1")
    pub process_preset: Option<String>,

    /// Custom configuration as JSON string (overrides presets)
    pub custom_config_json: Option<String>,
}

/// Statistics from the slicing process
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SlicerStats {
    /// Estimated print time (formatted string like "1h 23m")
    pub estimated_print_time: String,

    /// Total filament used in millimeters
    pub total_used_filament: f64,

    /// Total extruded volume in cubic millimeters
    pub total_extruded_volume: f64,

    /// Total weight in grams
    pub total_weight: f64,

    /// Total cost
    pub total_cost: f64,

    /// Number of tool changes
    pub total_toolchanges: i32,

    /// Per-filament statistics (filament ID -> usage in mm)
    #[serde(default)]
    pub filament_stats: std::collections::HashMap<usize, f64>,
}

/// Main slicer context - Builder API
///
/// This provides fine-grained control over the slicing process.
/// For a simpler API, see [`slice_model`].
pub struct Slicer {
    ctx: *mut ffi::SlicerContext,
}

impl Slicer {
    /// Create a new slicer context
    pub fn new() -> Result<Self> {
        let ctx = unsafe { ffi::slicer_create() };
        if ctx.is_null() {
            return Err(SlicerError::Internal(
                "Failed to create slicer context".to_string(),
            ));
        }
        Ok(Slicer { ctx })
    }

    /// Load a 3D model from file
    ///
    /// Supported formats: STL, 3MF, AMF, OBJ
    pub fn load_model(&mut self, path: &Path) -> Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| SlicerError::Internal("Invalid path encoding".to_string()))?;
        let c_path = CString::new(path_str)?;

        let result = unsafe { ffi::slicer_load_model(self.ctx, c_path.as_ptr()) };

        if result != SLICER_SUCCESS {
            let error_msg = self.get_error_message();
            return Err(SlicerError::from_code(result, error_msg));
        }

        Ok(())
    }

    /// Configure using preset names
    pub fn load_preset(&mut self, config: &SlicerConfig) -> Result<()> {
        let printer_c = config
            .printer_preset
            .as_ref()
            .map(|s| CString::new(s.as_str()))
            .transpose()?;
        let filament_c = config
            .filament_preset
            .as_ref()
            .map(|s| CString::new(s.as_str()))
            .transpose()?;
        let process_c = config
            .process_preset
            .as_ref()
            .map(|s| CString::new(s.as_str()))
            .transpose()?;

        let result = unsafe {
            ffi::slicer_load_preset(
                self.ctx,
                printer_c
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(ptr::null()),
                filament_c
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(ptr::null()),
                process_c
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(ptr::null()),
            )
        };

        if result != SLICER_SUCCESS {
            let error_msg = self.get_error_message();
            return Err(SlicerError::from_code(result, error_msg));
        }

        Ok(())
    }

    /// Set a single configuration parameter
    pub fn set_config_param(&mut self, key: &str, value: &str) -> Result<()> {
        let c_key = CString::new(key)?;
        let c_value = CString::new(value)?;

        let result =
            unsafe { ffi::slicer_set_config_param(self.ctx, c_key.as_ptr(), c_value.as_ptr()) };

        if result != SLICER_SUCCESS {
            let error_msg = self.get_error_message();
            return Err(SlicerError::from_code(result, error_msg));
        }

        Ok(())
    }

    /// Perform slicing and return statistics
    ///
    /// This processes the model but doesn't export G-code yet.
    /// Call [`export_gcode`](Self::export_gcode) to write the G-code file.
    pub fn slice(&mut self) -> Result<SlicerStats> {
        let result = unsafe { ffi::slicer_process(self.ctx) };

        if result != SLICER_SUCCESS {
            let error_msg = self.get_error_message();
            return Err(SlicerError::from_code(result, error_msg));
        }

        // Get statistics
        let stats_ptr = unsafe { ffi::slicer_get_stats_json(self.ctx) };
        if stats_ptr.is_null() {
            return Err(SlicerError::Internal(
                "Failed to get statistics".to_string(),
            ));
        }

        let stats_json = unsafe { CStr::from_ptr(stats_ptr) }
            .to_str()
            .map_err(|_| SlicerError::Internal("Invalid UTF-8 in statistics".to_string()))?;

        serde_json::from_str(stats_json)
            .map_err(|e| SlicerError::Internal(format!("Failed to parse statistics: {}", e)))
    }

    /// Export G-code to file
    ///
    /// Must be called after [`slice`](Self::slice).
    pub fn export_gcode(&self, path: &Path) -> Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| SlicerError::Internal("Invalid path encoding".to_string()))?;
        let c_path = CString::new(path_str)?;

        let result = unsafe { ffi::slicer_export_gcode(self.ctx, c_path.as_ptr()) };

        if result != SLICER_SUCCESS {
            let error_msg = self.get_error_message();
            return Err(SlicerError::from_code(result, error_msg));
        }

        Ok(())
    }

    /// Get the last error message from the C API
    fn get_error_message(&self) -> Option<String> {
        let error_ptr = unsafe { ffi::slicer_get_last_error(self.ctx) };
        if error_ptr.is_null() {
            return None;
        }

        unsafe { CStr::from_ptr(error_ptr) }
            .to_str()
            .ok()
            .map(|s| s.to_string())
    }
}

impl Drop for Slicer {
    fn drop(&mut self) {
        unsafe { ffi::slicer_destroy(self.ctx) };
    }
}

// Safety: SlicerContext is designed to be used from a single thread
// The underlying C++ objects are not thread-safe
unsafe impl Send for Slicer {}

/// Slice a model in one function call (Simple API)
///
/// This is a convenience function that creates a slicer, loads the model,
/// configures it, slices, and exports in one call.
///
/// # Examples
///
/// ```no_run
/// use bambu_slicer::{slice_model, SlicerConfig};
/// use std::path::Path;
///
/// let config = SlicerConfig {
///     printer_preset: Some("Bambu Lab A1".to_string()),
///     filament_preset: Some("Bambu PLA Basic @BBL A1".to_string()),
///     process_preset: Some("0.20mm Standard @BBL A1".to_string()),
///     custom_config_json: None,
/// };
///
/// let stats = slice_model(
///     Path::new("model.3mf"),
///     &config,
///     Path::new("output.gcode"),
/// ).expect("Slicing failed");
///
/// println!("Filament used: {:.2}mm", stats.total_used_filament);
/// ```
pub fn slice_model(
    model_path: &Path,
    config: &SlicerConfig,
    output_path: &Path,
) -> Result<SlicerStats> {
    let mut slicer = Slicer::new()?;

    slicer.load_model(model_path)?;

    // Apply configuration
    if config.printer_preset.is_some()
        || config.filament_preset.is_some()
        || config.process_preset.is_some()
    {
        slicer.load_preset(config)?;
    }

    if let Some(ref json) = config.custom_config_json {
        // TODO: Apply JSON config (not yet implemented in C API)
        let _ = json;
    }

    let stats = slicer.slice()?;
    slicer.export_gcode(output_path)?;

    Ok(stats)
}

/// Get the API version
pub fn get_version() -> String {
    let version_ptr = unsafe { ffi::slicer_get_version() };
    unsafe { CStr::from_ptr(version_ptr) }
        .to_str()
        .unwrap_or("unknown")
        .to_string()
}

/// Get the BambuStudio version
pub fn get_bambu_version() -> String {
    let version_ptr = unsafe { ffi::slicer_get_bambu_version() };
    unsafe { CStr::from_ptr(version_ptr) }
        .to_str()
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slicer_creation() {
        // Note: This test will fail without linking to libslic3r
        // It's here for demonstration
        let result = Slicer::new();
        // Can't actually test without the library linked
        let _ = result;
    }

    #[test]
    fn test_config_creation() {
        let config = SlicerConfig {
            printer_preset: Some("Bambu Lab A1".to_string()),
            filament_preset: None,
            process_preset: None,
            custom_config_json: None,
        };
        assert!(config.printer_preset.is_some());
    }
}
