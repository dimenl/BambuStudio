# Place your test STL/3MF files here

This directory is for test model files.

## Getting Test Models

You can:
1. Use one of your existing STL files from previous prints
2. Download a simple model from Thingiverse or Printables
3. Export an STL from any CAD software

## Recommended Test Models

For initial testing, use simple models:
- **Small cube** (10-20mm) - Fast to slice, good for testing
- **Benchy** - Classic 3D printing test
- **Calibration cube** - Good for verifying settings

## Usage

Once you have a model here:

```bash
cd /Users/darshan.sithan/Documents/mee/BambuStudio/test_slicer
cargo run models/your-model.stl
```
