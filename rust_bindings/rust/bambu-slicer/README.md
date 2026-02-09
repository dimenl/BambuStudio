# BambuSlicer Rust Bindings

This crate provides safe Rust bindings to the BambuStudio slicer core.

## Build

From the BambuStudio root:

```bash
mkdir -p build_rust && cd build_rust
cmake .. -DCMAKE_BUILD_TYPE=Release -DBUILD_RUST_BINDINGS=ON
cmake --build . --target libslic3r -j$(sysctl -n hw.ncpu)
```

Then build the Rust crate:

```bash
cd rust_bindings/rust/bambu-slicer
export BAMBU_BUILD_DIR=../../../build_rust
cargo build --release
```

## Usage

### Simple API

```rust
use bambu_slicer::{slice_model, SlicerConfig};
use std::path::Path;

let config = SlicerConfig {
    printer_preset: Some("Bambu Lab A1 0.4 nozzle".to_string()),
    filament_preset: Some("Bambu PLA Basic @BBL A1".to_string()),
    process_preset: Some("0.20mm Standard @BBL A1".to_string()),
    custom_config_json: None,
};

let stats = slice_model(
    Path::new("model.3mf"),
    &config,
    Path::new("output.gcode"),
)?;

println!("Print time: {}", stats.estimated_print_time);
println!("Filament used: {:.2}mm", stats.total_used_filament);
```

### Builder API

```rust
use bambu_slicer::{Slicer, SlicerConfig};
use std::path::Path;

let mut slicer = Slicer::new()?;

slicer.load_model(Path::new("model.stl"))?;

let presets = SlicerConfig {
    printer_preset: Some("Bambu Lab A1 0.4 nozzle".to_string()),
    filament_preset: Some("Bambu PLA Basic @BBL A1".to_string()),
    process_preset: Some("0.20mm Standard @BBL A1".to_string()),
    custom_config_json: None,
};

slicer.load_preset(&presets)?;
slicer.set_config_param("layer_height", "0.2")?;

slicer.slice()?;
slicer.export_gcode(Path::new("output.gcode"))?;

let stats = slicer.get_stats()?;
println!("Cost: ${:.2}", stats.total_cost);
```

### CLI Example

```bash
cargo run --release --example slice_model -- \
  --model /path/to/model.3mf \
  --output /tmp/output.gcode \
  --printer "Bambu Lab A1 0.4 nozzle" \
  --filament "Bambu PLA Basic @BBL A1" \
  --process "0.20mm Standard @BBL A1" \
  --param layer_height=0.2 \
  --param sparse_infill_density=15%
```

## Notes

- The underlying slicer is not thread-safe. Create one `Slicer` per thread.
- Preset names must match the bundled profiles under `resources/profiles`.
- Statistics are available after `export_gcode()`.

## Troubleshooting

- If linking fails, confirm `BAMBU_BUILD_DIR` points to the build directory that
  contains `src/libslic3r/liblibslic3r.a`.
- If presets fail to load, verify the preset names and ensure profiles are
  available in your runtime environment.
