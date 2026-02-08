use bambu_slicer::{slice_model, Slicer, SlicerConfig};
use std::path::Path;

fn main() {
    println!("=== BambuSlicer A1 Test ===\n");

    // Check if model file is provided
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <model.stl>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  cargo run models/test.stl");
        std::process::exit(1);
    }

    let model_path = Path::new(&args[1]);

    // Verify model exists
    if !model_path.exists() {
        eprintln!("Error: Model file not found: {}", model_path.display());
        eprintln!("\nPlace your STL file in the models/ directory");
        std::process::exit(1);
    }

    // Output path
    let model_name = model_path.file_stem().unwrap().to_str().unwrap();
    let output_path = Path::new("output").join(format!("{}_a1.gcode", model_name));

    println!("Model:  {}", model_path.display());
    println!("Output: {}\n", output_path.display());

    // Method 1: Simple API (one function call)
    println!("--- Using Simple API ---");
    test_simple_api(model_path, &output_path);

    println!("\n--- Using Builder API ---");
    test_builder_api(model_path, &output_path);
}

/// Test using the simple API
fn test_simple_api(model_path: &Path, output_path: &Path) {
    // A1 printer configuration
    let config = SlicerConfig {
        printer_preset: Some("Bambu Lab A1 0.4 nozzle".to_string()),
        filament_preset: Some("Bambu PLA Basic @BBL A1".to_string()),
        process_preset: Some("0.20mm Standard @BBL A1".to_string()),
        custom_config_json: None,
    };

    println!("Slicing with A1 presets...");

    match slice_model(model_path, &config, output_path) {
        Ok(stats) => {
            println!("✓ Success!");
            print_stats(&stats);
        }
        Err(e) => {
            eprintln!("✗ Failed: {}", e);
            eprintln!("\nNote: This might fail if:");
            eprintln!("  1. C API library not built yet (run cmake with -DBUILD_RUST_BINDINGS=ON)");
            eprintln!("  2. Dependencies not installed");
            eprintln!("  3. Linking not configured in build.rs");
        }
    }
}

/// Test using the builder API
fn test_builder_api(model_path: &Path, output_path: &Path) {
    let output_path_builder = output_path
        .with_file_name(format!(
            "{}_builder",
            output_path.file_stem().unwrap().to_str().unwrap()
        ))
        .with_extension("gcode");

    println!("Creating slicer context...");
    let mut slicer = match Slicer::new() {
        Ok(s) => {
            println!("✓ Context created");
            s
        }
        Err(e) => {
            eprintln!("✗ Failed to create slicer: {}", e);
            return;
        }
    };

    // Load model
    print!("Loading model... ");
    if let Err(e) = slicer.load_model(model_path) {
        eprintln!("✗ Failed: {}", e);
        return;
    }
    println!("✓");

    // Set A1-specific parameters manually
    let params = [
        ("layer_height", "0.2"),
        ("initial_layer_print_height", "0.2"),
        ("wall_loops", "3"),
        ("top_surface_pattern", "monotonic"),
        ("bottom_surface_pattern", "monotonic"),
        ("sparse_infill_density", "15%"),
        ("sparse_infill_pattern", "grid"),
        ("support_enable", "false"),
        ("brim_width", "5"),
        ("print_speed", "200"),
        ("outer_wall_speed", "100"),
        ("inner_wall_speed", "150"),
        ("initial_layer_speed", "50"),
    ];

    println!("Setting A1 parameters:");
    for (key, value) in &params {
        print!("  {} = {}... ", key, value);
        match slicer.set_config_param(key, value) {
            Ok(_) => println!("✓"),
            Err(e) => {
                eprintln!("✗ {}", e);
                // Continue with other params
            }
        }
    }

    // Slice
    print!("\nSlicing... ");
    if let Err(e) = slicer.slice() {
        eprintln!("✗ Failed: {}", e);
        return;
    }
    println!("✓");

    // Export
    print!("Exporting G-code... ");
    if let Err(e) = slicer.export_gcode(&output_path_builder) {
        eprintln!("✗ Failed: {}", e);
        return;
    }
    println!("✓");

    // Get statistics
    let stats = match slicer.get_stats() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("✗ Failed to get stats: {}", e);
            return;
        }
    };

    println!("\n✓ Success!");
    print_stats(&stats);
}

fn print_stats(stats: &bambu_slicer::SlicerStats) {
    println!("\n=== Print Statistics ===");
    println!("Print Time:      {}", stats.estimated_print_time);
    println!(
        "Filament Used:   {:.2} mm ({:.2} g)",
        stats.total_used_filament, stats.total_weight
    );
    println!("Extruded Volume: {:.2} mm³", stats.total_extruded_volume);
    println!("Estimated Cost:  ${:.2}", stats.total_cost);
    println!("Tool Changes:    {}", stats.total_toolchanges);

    if !stats.filament_stats.is_empty() {
        println!("\nPer-Filament Usage:");
        for (id, usage) in &stats.filament_stats {
            println!("  Extruder {}: {:.2} mm", id, usage);
        }
    }
}
