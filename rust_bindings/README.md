# Rust Bindings for BambuStudio Slicer

This directory contains the Rust FFI bindings for the BambuStudio slicer core.

## Structure

```
rust_bindings/
├── c_api/                  # C API wrapper layer
│   ├── slicer_c_api.h     # C API header
│   ├── slicer_c_api.cpp   # C API implementation
│   └── CMakeLists.txt     # Build configuration
│
└── rust/                   # Rust bindings
    └── bambu-slicer/      # Rust crate
```

## Building

### 1. Build C API with CMake

From the BambuStudio root directory:

```bash
cd /path/to/BambuStudio
mkdir -p build_rust && cd build_rust
cmake .. -DCMAKE_BUILD_TYPE=Release -DBUILD_RUST_BINDINGS=ON
cmake --build . --target libslic3r
```

### 2. Build Rust Bindings

```bash
cd rust_bindings/rust/bambu-slicer
BAMBU_BUILD_DIR=../../../build_rust cargo build --release
```

## Integration with Upstream

This directory is designed to minimize conflicts with upstream BambuStudio:

- All new code is isolated in `rust_bindings/`
- Only minimal changes needed to parent CMakeLists.txt
- Can be excluded from upstream merges if needed

## Usage

See `rust/bambu-slicer/README.md` for a full usage guide and examples.
