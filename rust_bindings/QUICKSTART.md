# Quick Start Guide - Rust Bindings for BambuStudio

## What You Have Now

A complete implementation of Rust FFI bindings for BambuStudio slicer, with:

✅ C API wrapper  
✅ Safe Rust library  
✅ Two API styles (simple + builder)  
✅ Example CLI tool  
✅ Minimal upstream conflicts

## Build Steps (Quick Reference)

### 1. Install Dependencies

On macOS (adjust for your system):
```bash
brew install boost opencv cgal tbb eigen nlohmann-json glew glfw cereal
brew install opencascade openvdb
```

### 2. Build C API

```bash
cd BambuStudio
mkdir -p build_rust && cd build_rust
cmake .. -DCMAKE_BUILD_TYPE=Release -DBUILD_RUST_BINDINGS=ON
cmake --build . --target libslic3r -j$(sysctl -n hw.ncpu)
```

### 3. Build Rust Bindings

```bash
cd ../rust_bindings/rust/bambu-slicer
export BAMBU_BUILD_DIR=$PWD/../../../build_rust
cargo build --release
```

### 4. Test It

```bash
cargo run --release --example slice_model -- \
    --model /path/to/model.3mf \
    --output /tmp/test.gcode \
    --printer "Bambu Lab A1 0.4 nozzle" \
    --filament "Bambu PLA Basic @BBL A1" \
    --process "0.20mm Standard @BBL A1"
```

## Simple Usage Example

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

println!("Filament: {:.1}mm", stats.total_used_filament);
```

## What's Not Yet Done

1. **Preset bundling** - Currently preset loading returns error; use `set_config_param()` instead
2. **Link configuration** - `build.rs` needs all dependencies added once C API builds
3. **Config bundling** - A1 configs not yet embedded in binary

See `rust_bindings/rust/bambu-slicer/README.md` for a full usage guide and troubleshooting.

## Need Help?

The usage guide covers:
- Detailed troubleshooting
- Architecture explanations
- More usage examples
- Build configuration details
