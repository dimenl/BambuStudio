#!/usr/bin/env python3
"""
Example Python client for BambuSlicer HTTP Service
"""

import requests
import base64
import sys

def slice_model(model_path, output_path, server_url="http://localhost:8080"):
    """
    Slice a 3D model using the BambuSlicer service
    
    Args:
        model_path: Path to STL/3MF/AMF/OBJ file
        output_path: Where to save the G-code
        server_url: Service URL (default: localhost:8080)
    """
    
    print(f"📁 Loading model: {model_path}")
    
    # Prepare request
    with open(model_path, 'rb') as f:
        files = {'model': f}
        
        # Optional: Add custom config
        config = {
            "printer_preset": "Bambu Lab A1",
            "filament_preset": "Bambu PLA Basic @BBL A1",
            "process_preset": "0.20mm Standard @BBL A1"
        }
        
        data = {'config': str(config).replace("'", '"')}
        
        print(f"🚀 Sending to {server_url}/slice...")
        response = requests.post(
            f"{server_url}/slice",
            files=files,
            data=data
        )
    
    if response.status_code != 200:
        print(f"❌ Error: {response.status_code}")
        print(response.json())
        sys.exit(1)
    
    result = response.json()
    
    # Print statistics
    stats = result['stats']
    print(f"\n✅ Slicing completed!")
    print(f"   Job ID: {result['job_id']}")
    print(f"   Print Time: {stats['estimated_print_time']}")
    print(f"   Filament: {stats['total_used_filament']:.2f} mm ({stats['total_weight']:.2f} g)")
    print(f"   Cost: ${stats['total_cost']:.2f}")
    
    # Decode and save G-code
    gcode_bytes = base64.b64decode(result['gcode'])
    with open(output_path, 'wb') as f:
        f.write(gcode_bytes)
    
    print(f"\n💾 G-code saved to: {output_path}")
    print(f"   Size: {len(gcode_bytes)} bytes")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python3 client.py <model.stl> <output.gcode>")
        print("\nExample:")
        print("  python3 client.py cube.stl output.gcode")
        sys.exit(1)
    
    slice_model(sys.argv[1], sys.argv[2])
