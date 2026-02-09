#!/usr/bin/env bash
# Example bash script to test the slicer service

set -e

SERVER="http://localhost:8080"
MODEL="${1:-test.stl}"

echo "🔍 Testing BambuSlicer Service"
echo "================================"
echo ""

# Health check
echo "1. Health Check"
curl -s $SERVER/health | jq .
echo ""

# Slice model
echo "2. Slicing $MODEL"
RESPONSE=$(curl -s -X POST $SERVER/slice -F "model=@$MODEL")

# Extract stats
echo "$RESPONSE" | jq '.stats'

# Save G-code
JOB_ID=$(echo "$RESPONSE" | jq -r '.job_id')
echo "$RESPONSE" | jq -r '.gcode' | base64 -d > "output_${JOB_ID}.gcode"

echo ""
echo "✅ Done! G-code saved to: output_${JOB_ID}.gcode"
