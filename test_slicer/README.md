# BambuSlicer A1 Test Program

A simple test program for the Rust bindings configured specifically for the Bambu Lab A1 printer.

## Structure

```
test_slicer/
├── Cargo.toml          # Project dependencies
├── src/
│   └── main.rs         # Test program
├── models/             # Place your STL/3MF files here
└── output/             # G-code output directory
```

## Usage

### 1. Place Your STL File

Put your test STL file in the `models/` directory:

```bash
cp /path/to/your/model.stl test_slicer/models/
```

### 2. Build Dependencies First

Before running this test, you need to build the C API:

```bash
# From BambuStudio root
cd /Users/darshan.sithan/Documents/mee/BambuStudio

# Install dependencies (if not done yet)
brew install boost opencv cgal tbb eigen nlohmann-json glew glfw cereal opencascade openvdb

# Build C API
mkdir -p build_rust && cd build_rust
cmake .. -DCMAKE_BUILD_TYPE=Release -DBUILD_RUST_BINDINGS=ON
cmake --build . --target libslic3r -j$(sysctl -n hw.ncpu)
```

### 3. Configure Rust Linking

Set the build directory environment variable:

```bash
export BAMBU_BUILD_DIR=/Users/darshan.sithan/Documents/mee/BambuStudio/build_rust
```

### 4. Run the Test

```bash
cd test_slicer
cargo run models/your-model.stl
```

## A1 Configuration

The test program uses these A1-specific settings:

**Presets** (Simple API):
- Printer: "Bambu Lab A1"
- Filament: "Bambu PLA Basic @BBL A1"
- Process: "0.20mm Standard @BBL A1"

**Manual Parameters** (Builder API):
- Layer height: 0.2mm
- Walls: 3 loops
- Infill: 15% grid pattern
- Print speed: 200mm/s
- Wall speed: 100mm/s (outer), 150mm/s (inner)
- Brim: 5mm
- Support: Disabled

## Output

G-code files will be saved in the `output/` directory:
- `{filename}_a1.gcode` - From simple API
- `{filename}_a1_builder.gcode` - From builder API

## Troubleshooting

If you get errors:

1. **"Failed to create slicer"** → C API not linked yet
   - Build the C API library first
   - Configure linking in `../rust_bindings/rust/bambu-slicer/build.rs`

2. **"Model file not found"** → Place STL in `models/` directory

3. **"Preset not found"** → Preset loading not fully implemented yet
   - Use builder API with manual parameters instead

## Example Output

```
=== BambuSlicer A1 Test ===

Model:  models/cube.stl
Output: output/cube_a1.gcode

--- Using Simple API ---
Slicing with A1 presets...
✓ Success!

=== Print Statistics ===
Print Time:      1h 23m
Filament Used:   1234.56 mm (15.30 g)
Extruded Volume: 9876.54 mm³
Estimated Cost:  $2.45
Tool Changes:    0
```

## Next Steps

Once this works:
1. Try different A1 settings
2. Test with complex models
3. Compare output with BambuStudio GUI
4. Integrate into your own Rust projects
