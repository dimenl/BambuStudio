# BambuSlicer Docker Service - Quick Start

## Build the Service

```bash
# From BambuStudio directory
docker build -f Dockerfile.service -t bambu-slicer-service .
```

**Build time**: 20-30 minutes (one-time)  
**Final image size**: ~500MB (runtime only)

## Run the Service

### Option 1: Docker Compose (Recommended)

```bash
docker-compose up -d
```

### Option 2: Docker Run

```bash
docker run -d \
  -p 8080:8080 \
  --name bambu-slicer \
  --restart unless-stopped \
  bambu-slicer-service
```

## Test the Service

```bash
# Health check
curl http://localhost:8080/health

# Slice a model
curl -X POST http://localhost:8080/slice \
  -F "model=@your-file.stl" \
  | jq '.stats'
```

## What's Different

This Dockerfile:
- ✅ Uses BuildLinux.sh (proven, reliable)
- ✅ Multi-stage build (minimal runtime image)
- ✅ Only runtime dependencies in final image
- ✅ No GUI components
- ✅ Runs as non-root user

## Architecture

```
Builder Stage (4GB+)          Runtime Stage (500MB)
├── Ubuntu 24.04              ├── Ubuntu 24.04
├── All build tools           ├── Runtime libs only
├── BuildLinux.sh -u          ├── slicer-service binary
├── BuildLinux.sh -d          └── Health check
├── CMake build
├── Rust build
└── [discarded]
```

## Troubleshooting

**Build fails**: Ensure you have 10GB+ free disk and 10GB+ RAM

**Service won't start**: Check logs with `docker logs bambu-slicer`

**Port conflict**: Change port mapping `-p 8081:8080`

See full API documentation in `slicer_service/README.md`
