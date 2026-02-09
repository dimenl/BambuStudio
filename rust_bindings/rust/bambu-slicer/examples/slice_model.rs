use bambu_slicer::{slice_model, Slicer, SlicerConfig};
use std::path::PathBuf;
use std::process;

fn print_usage() {
    eprintln!("Usage: slice_model [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --model <PATH>         Input model file (STL, 3MF, AMF, OBJ)");
    eprintln!("  --output <PATH>        Output G-code file");
    eprintln!("  --printer <NAME>       Printer preset name");
    eprintln!("  --filament <NAME>      Filament preset name");
    eprintln!("  --process <NAME>       Process preset name");
    eprintln!("  --param <KEY=VALUE>    Set config parameter (can be used multiple times)");
    eprintln!("  --simple               Use simple API (default: builder API)");
    eprintln!("  --help                 Print this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  # Using presets:");
    eprintln!("  slice_model --model cube.3mf --output cube.gcode \\");
    eprintln!("    --printer \"Bambu Lab A1 0.4 nozzle\" \\");
    eprintln!("    --filament \"Bambu PLA Basic @BBL A1\" \\");
    eprintln!("    --process \"0.20mm Standard @BBL A1\"");
    eprintln!();
    eprintln!("  # Using custom parameters:");
    eprintln!("  slice_model --model model.stl --output output.gcode \\");
    eprintln!("    --param layer_height=0.2 \\");
    eprintln!("    --param sparse_infill_density=15%");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse arguments
    let mut model_path: Option<PathBuf> = None;
    let mut output_path: Option<PathBuf> = None;
    let mut printer_preset: Option<String> = None;
    let mut filament_preset: Option<String> = None;
    let mut process_preset: Option<String> = None;
    let mut use_simple = false;
    let mut params: Vec<(String, String)> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            "--model" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --model requires a value");
                    process::exit(1);
                }
                model_path = Some(PathBuf::from(&args[i]));
            }
            "--output" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --output requires a value");
                    process::exit(1);
                }
                output_path = Some(PathBuf::from(&args[i]));
            }
            "--printer" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --printer requires a value");
                    process::exit(1);
                }
                printer_preset = Some(args[i].clone());
            }
            "--filament" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --filament requires a value");
                    process::exit(1);
                }
                filament_preset = Some(args[i].clone());
            }
            "--process" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --process requires a value");
                    process::exit(1);
                }
                process_preset = Some(args[i].clone());
            }
            "--param" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --param requires a value");
                    process::exit(1);
                }
                let parts: Vec<&str> = args[i].splitn(2, '=').collect();
                if parts.len() != 2 {
                    eprintln!("Error: --param value must be in KEY=VALUE format");
                    process::exit(1);
                }
                params.push((parts[0].to_string(), parts[1].to_string()));
            }
            "--simple" => {
                use_simple = true;
            }
            _ => {
                eprintln!("Error: Unknown option: {}", args[i]);
                print_usage();
                process::exit(1);
            }
        }
        i += 1;
    }

    // Validate required arguments
    if model_path.is_none() {
        eprintln!("Error: --model is required");
        print_usage();
        process::exit(1);
    }
    if output_path.is_none() {
        eprintln!("Error: --output is required");
        print_usage();
        process::exit(1);
    }

    let model = model_path.unwrap();
    let output = output_path.unwrap();

    println!("BambuSlicer Rust Bindings v{}", bambu_slicer::get_version());
    println!("BambuStudio version: {}", bambu_slicer::get_bambu_version());
    println!();
    println!("Model:  {}", model.display());
    println!("Output: {}", output.display());
    println!();

    // Run slicing
    let result = if use_simple {
        println!("Using simple API...");

        let config = SlicerConfig {
            printer_preset,
            filament_preset,
            process_preset,
            custom_config_json: None,
        };

        slice_model(&model, &config, &output)
    } else {
        println!("Using builder API...");

        let mut slicer = match Slicer::new() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create slicer: {}", e);
                process::exit(1);
            }
        };

        // Load model
        print!("Loading model... ");
        if let Err(e) = slicer.load_model(&model) {
            eprintln!("\nFailed to load model: {}", e);
            process::exit(1);
        }
        println!("OK");

        // Apply configuration
        if printer_preset.is_some() || filament_preset.is_some() || process_preset.is_some() {
            let config = SlicerConfig {
                printer_preset,
                filament_preset,
                process_preset,
                custom_config_json: None,
            };

            print!("Loading presets... ");
            if let Err(e) = slicer.load_preset(&config) {
                eprintln!("\nFailed to load presets: {}", e);
                process::exit(1);
            }
            println!("OK");
        }

        // Apply custom parameters
        for (key, value) in params {
            print!("Setting {}={}... ", key, value);
            if let Err(e) = slicer.set_config_param(&key, &value) {
                eprintln!("\nFailed to set parameter: {}", e);
                process::exit(1);
            }
            println!("OK");
        }

        // Slice
        print!("Slicing... ");
        if let Err(e) = slicer.slice() {
            eprintln!("\nFailed to slice: {}", e);
            process::exit(1);
        }
        println!("OK");

        // Export
        print!("Exporting G-code... ");
        if let Err(e) = slicer.export_gcode(&output) {
            eprintln!("\nFailed to export: {}", e);
            process::exit(1);
        }
        println!("OK");

        // Get stats
        print!("Reading stats... ");
        let stats = match slicer.get_stats() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("\nFailed to read stats: {}", e);
                process::exit(1);
            }
        }
        println!("OK");

        Ok(stats)
    };

    match result {
        Ok(stats) => {
            println!();
            println!("=== Slicing Complete ===");
            println!("Print Time:      {}", stats.estimated_print_time);
            println!("Filament Used:   {:.2} mm", stats.total_used_filament);
            println!("Extruded Volume: {:.2} mm³", stats.total_extruded_volume);
            println!("Weight:          {:.2} g", stats.total_weight);
            println!("Cost:            ${:.2}", stats.total_cost);
            println!("Tool Changes:    {}", stats.total_toolchanges);

            if !stats.filament_stats.is_empty() {
                println!();
                println!("=== Per-Filament Stats ===");
                for (id, usage) in &stats.filament_stats {
                    println!("Filament {}: {:.2} mm", id, usage);
                }
            }

            println!();
            println!("Statistics JSON:");
            match serde_json::to_string_pretty(&stats) {
                Ok(json) => println!("{}", json),
                Err(e) => eprintln!("Failed to serialize stats: {}", e),
            }
        }
        Err(e) => {
            eprintln!("\nSlicing failed: {}", e);
            process::exit(1);
        }
    }
}
