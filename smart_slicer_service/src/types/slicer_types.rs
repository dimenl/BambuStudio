use bambu_slicer::SlicerStats;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct SliceRouteRequest {
    pub input_path: String,
    pub output_path: String,
    pub config: Option<SliceConfigRequest>,
}

#[derive(Debug, Deserialize)]
pub struct SliceConfigRequest {
    pub printer_preset: Option<String>,
    pub filament_preset: Option<String>,
    pub process_preset: Option<String>,
    pub custom_params: Option<Vec<(String, String)>>,
    pub custom_config_json: Option<Value>,
    pub rotation: Option<(f64, f64, f64)>,
    #[serde(default)]
    pub gcode_needed: bool,
}

impl Default for SliceConfigRequest {
    fn default() -> Self {
        Self {
            printer_preset: Some("Bambu Lab A1 0.4 nozzle".to_string()),
            filament_preset: Some("Bambu PLA Basic @BBL A1".to_string()),
            process_preset: Some("0.20mm Standard @BBL A1".to_string()),
            custom_params: None,
            custom_config_json: None,
            rotation: None,
            gcode_needed: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SliceRouteResponse {
    pub stats: SlicerStats,
    pub presets: Value,
    pub config: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gcode: Option<String>,
}
