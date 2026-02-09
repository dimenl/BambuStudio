# BambuSlicer HTTP Service

A containerized REST API for slicing 3D models using BambuStudio's core engine.

## Features

- 🚀 **HTTP REST API** - Simple POST endpoint for slicing
- 🐳 **Dockerized** - Easy deployment with Docker/Docker Compose
- 📦 **Format Support** - STL, 3MF, AMF, OBJ files
- ⚙️ **Flexible Config** - Presets or custom parameters
- 📊 **Statistics** - Returns print time, filament usage, cost, etc.
- 🔧 **A1 Optimized** - Default config for Bambu Lab A1 0.4 nozzle

## Quick Start

### Using Docker Compose (Recommended)

```bash
# Build and start the service
docker-compose up -d

# Check logs
docker-compose logs -f

# Stop the service
docker-compose down
```

### Using Docker

```bash
# Build
docker build -f Dockerfile.service -t bambu-slicer-service .

# Run
docker run -d -p 8080:8080 --name bambu-slicer bambu-slicer-service

# Check health
curl http://localhost:8080/health
```

## API Usage

### Health Check

```bash
curl http://localhost:8080/health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "bambu_version": "2.1.0"
}
```

### Slice a Model

**Using Presets (A1 defaults):**

```bash
curl -X POST http://localhost:8080/slice \
  -F "model=@cube.stl"
```

**With Custom Config:**

```bash
curl -X POST http://localhost:8080/slice \
  -F "model=@model.3mf" \
  -F 'config={
    "printer_preset": "Bambu Lab A1 0.4 nozzle",
    "filament_preset": "Bambu PLA Basic @BBL A1",
    "process_preset": "0.20mm Standard @BBL A1"
  }'
```

**With Custom Parameters:**

```bash
curl -X POST http://localhost:8080/slice \
  -F "model=@part.stl" \
  -F 'config={
    "printer_preset": "Bambu Lab A1 0.4 nozzle",
    "custom_params": [
      ["layer_height", "0.2"],
      ["sparse_infill_density", "20%"],
      ["wall_loops", "4"],
      ["print_speed", "150"]
    ]
  }'
```

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "stats": {
    "estimated_print_time": "1h 23m",
    "total_used_filament": 1234.56,
    "total_extruded_volume": 9876.54,
    "total_weight": 15.30,
    "total_cost": 2.45,
    "total_toolchanges": 0,
    "filament_stats": {
      "0": 1234.56
    }
  },
  "presets": {
    "printer": "Bambu Lab A1 0.4 nozzle",
    "filament": "Bambu PLA Basic @BBL A1",
    "process": "0.20mm Standard @BBL A1"
  },
  "config": {
    "...": "full resolved config"
  },
  "gcode": "base64_encoded_gcode_here..."
}
```

### Decode G-code

The G-code is base64-encoded. To save it:

```bash
# Using jq
curl -X POST http://localhost:8080/slice -F "model=@cube.stl" | \
  jq -r '.gcode' | base64 -d > output.gcode

# Using Python
python3 << EOF
import requests, base64

with open('cube.stl', 'rb') as f:
    response = requests.post(
        'http://localhost:8080/slice',
        files={'model': f}
    )

gcode = base64.b64decode(response.json()['gcode'])
with open('output.gcode', 'wb') as f:
    f.write(gcode)
EOF
```

## Configuration Reference

### Preset Names

**Printers (examples):**
- "Bambu Lab A1 0.4 nozzle"
- "Bambu Lab A1 0.6 nozzle"
- "Bambu Lab A1 mini 0.4 nozzle"
- "Bambu Lab P1P 0.4 nozzle"
- "Bambu Lab X1 Carbon 0.4 nozzle"

Preset names include nozzle size and must match the bundled profiles under
`resources/profiles`.

**Filaments:**
- "Bambu PLA Basic @BBL A1"
- "Bambu PETG Basic @BBL A1"
- "Bambu ABS @BBL A1"
- "Bambu TPU 95A @BBL A1"

**Processes:**
- "0.20mm Standard @BBL A1"
- "0.12mm Fine @BBL A1"
- "0.28mm Draft @BBL A1"

### Custom Parameters

Common parameters you can set:

| Parameter | Type | Example | Description |
|-----------|------|---------|-------------|
| `layer_height` | float | "0.2" | Layer height in mm |
| `initial_layer_print_height` | float | "0.2" | First layer height |
| `wall_loops` | int | "3" | Number of perimeters |
| `sparse_infill_density` | string | "15%" | Infill percentage |
| `sparse_infill_pattern` | string | "grid" | Infill pattern |
| `print_speed` | int | "200" | Print speed mm/s |
| `outer_wall_speed` | int | "100" | Outer wall speed |
| `support_enable` | bool | "true" | Enable supports |
| `brim_width` | int | "5" | Brim width mm |

## Development

### Local Build (Without Docker)

1. **Install Dependencies** (Ubuntu/Debian):
   ```bash
   sudo apt-get install build-essential cmake libboost-all-dev \
     libcgal-dev libtbb-dev libeigen3-dev nlohmann-json3-dev
   ```

2. **Build C API**:
   ```bash
   mkdir -p build_rust && cd build_rust
   cmake .. -DCMAKE_BUILD_TYPE=Release -DBUILD_RUST_BINDINGS=ON
   cmake --build . --target libslic3r -j$(nproc)
   cd ..
   ```

3. **Build Service**:
   ```bash
   cd slicer_service
   export BAMBU_BUILD_DIR=../build_rust
   cargo build --release
   ```

4. **Run**:
   ```bash
   ./target/release/slicer-service
   ```

### Test Locally

```bash
cd slicer_service
RUST_LOG=debug cargo run
```

## Architecture

```
┌─────────────┐
│   Client    │
│ (curl/app)  │
└──────┬──────┘
       │ HTTP POST
       ▼
┌──────────────────────────────┐
│  Axum Web Server (Rust)      │
│  - Multipart file upload     │
│  - JSON config parsing       │
│  - Error handling            │
└──────┬───────────────────────┘
       │
       ▼
┌──────────────────────────────┐
│  Rust FFI Bindings           │
│  - Safe wrappers             │
│  - Type conversions          │
└──────┬───────────────────────┘
       │
       ▼
┌──────────────────────────────┐
│  C API Wrapper               │
│  - SlicerContext management  │
│  - Exception handling        │
└──────┬───────────────────────┘
       │
       ▼
┌──────────────────────────────┐
│  BambuStudio Core (C++)      │
│  - Model loading             │
│  - Slicing engine            │
│  - G-code generation         │
└──────────────────────────────┘
```

## Troubleshooting

### Service won't start

Check logs:
```bash
docker-compose logs
```

### Build fails

Ensure you have enough disk space and memory. The build requires ~4GB RAM.

### Slicing errors

Check the `/slice` endpoint response for detailed error messages in the `details` field.

## License

AGPL-3.0 (same as BambuStudio)

## Credits

Built on [BambuStudio](https://github.com/bambulab/BambuStudio) slicer core.
