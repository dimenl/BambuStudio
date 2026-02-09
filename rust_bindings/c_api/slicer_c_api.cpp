/**
 * @file slicer_c_api.cpp
 * @brief C API Implementation for BambuStudio Slicer Core
 */

#include "slicer_c_api.h"

// BambuStudio/libslic3r includes
#include "../../src/libslic3r/libslic3r.h"
#include "../../src/libslic3r/Model.hpp"
#include "../../src/libslic3r/Print.hpp"
#include "../../src/libslic3r/PrintConfig.hpp"
#include "../../src/libslic3r/Config.hpp"
#include "../../src/libslic3r/Format/3mf.hpp"
#include "../../src/libslic3r/Format/bbs_3mf.hpp"
#include "../../src/libslic3r/Format/STL.hpp"
#include "../../src/libslic3r/Format/AMF.hpp"
#include "../../src/libslic3r/Format/OBJ.hpp"  
#include "../../src/libslic3r/GCode/GCodeProcessor.hpp"
#include "../../src/libslic3r/Preset.hpp"
#include "../../src/libslic3r/PresetBundle.hpp"
#include "../../src/libslic3r/AppConfig.hpp"
#include "../../src/libslic3r/Utils.hpp"

#include <memory>
#include <string>
#include <sstream>
#include <stdexcept>
#include <cstring>
#include <algorithm>
#include <cmath>
#include "nlohmann/json.hpp"

// JSON library for statistics output
#include <sstream>

using namespace Slic3r;
using nlohmann::json;

/* ============================================================================
 * Internal Structures
 * ============================================================================ */

struct SlicerContext {
    // Core objects
    std::unique_ptr<Model> model;
    std::unique_ptr<Print> print;
    DynamicPrintConfig config;
    
    // Preset management
    std::unique_ptr<AppConfig> app_config;
    std::unique_ptr<PresetBundle> preset_bundle;
    bool presets_loaded = false;
    
    // State flags
    bool model_loaded = false;
    bool config_loaded = false;
    bool processed = false;
    
    // Error handling
    std::string last_error;
    
    // Statistics (cached as JSON)
    std::string stats_json;

    // Configuration (cached as JSON)
    std::string config_json;

    // Preset info (cached as JSON)
    std::string preset_info_json;

    // Selected preset names
    std::string selected_printer_preset;
    std::string selected_filament_preset;
    std::string selected_process_preset;
    
    SlicerContext() {
        model = std::make_unique<Model>();
        print = std::make_unique<Print>();
    }
    
    void set_error(const std::string& error) {
        last_error = error;
    }
    
    void clear_error() {
        last_error.clear();
    }
};

/* ============================================================================
 * Helper Functions
 * ============================================================================ */

static int validate_context(SlicerContext* ctx) {
    if (!ctx) {
        return SLICER_ERROR_NULL_CONTEXT;
    }
    ctx->clear_error();
    return SLICER_SUCCESS;
}

static std::string generate_stats_json(const PrintStatistics& stats) {
    std::ostringstream oss;
    oss << "{\n";
    oss << "  \"estimated_print_time\": \"" << stats.estimated_normal_print_time << "\",\n";
    oss << "  \"total_used_filament\": " << stats.total_used_filament << ",\n";
    oss << "  \"total_extruded_volume\": " << stats.total_extruded_volume << ",\n";
    oss << "  \"total_weight\": " << stats.total_weight << ",\n";
    oss << "  \"total_cost\": " << stats.total_cost << ",\n";
    oss << "  \"total_toolchanges\": " << stats.total_toolchanges << ",\n";
    oss << "  \"filament_stats\": {\n";
    
    bool first = true;
    for (const auto& pair : stats.filament_stats) {
        if (!first) oss << ",\n";
        oss << "    \"" << pair.first << "\": " << pair.second;
        first = false;
    }
    
    oss << "\n  }\n";
    oss << "}";
    
    return oss.str();
}

static std::string generate_stats_json(
    const PrintEstimatedStatistics& stats,
    const PrintConfig& config,
    double timelapse_time_seconds,
    double initial_layer_time_seconds
) {
    json j;

    auto length_from_volume = [](double volume_mm3, double diameter) {
        double area = PI * (diameter / 2.0) * (diameter / 2.0);
        return area > 0.0 ? (volume_mm3 / area) : 0.0;
    };

    auto volume_map_to_json = [](const std::map<size_t, double>& volumes) {
        json out = json::object();
        for (const auto& [extruder_id, volume] : volumes) {
            out[std::to_string(extruder_id)] = volume;
        }
        return out;
    };

    auto length_map_to_json = [&](const std::map<size_t, double>& volumes) {
        json out = json::object();
        for (const auto& [extruder_id, volume] : volumes) {
            double diameter = 1.75;
            if (extruder_id < config.filament_diameter.values.size())
                diameter = config.filament_diameter.values[extruder_id];
            out[std::to_string(extruder_id)] = length_from_volume(volume, diameter);
        }
        return out;
    };

    const auto& normal = stats.modes[static_cast<size_t>(PrintEstimatedStatistics::ETimeMode::Normal)];
    const auto& stealth = stats.modes[static_cast<size_t>(PrintEstimatedStatistics::ETimeMode::Stealth)];

    double normal_model_time = std::max(0.0, (double)normal.time - (double)normal.prepare_time - timelapse_time_seconds);
    double stealth_model_time = std::max(0.0, (double)stealth.time - (double)stealth.prepare_time - timelapse_time_seconds);

    j["estimated_print_time"] = get_time_dhms(normal.time);
    j["estimated_print_time_seconds"] = normal.time;
    j["prepare_time_seconds"] = normal.prepare_time;
    j["prepare_time_formatted"] = get_time_dhms(normal.prepare_time);
    j["timelapse_time_seconds"] = timelapse_time_seconds;
    j["timelapse_time_formatted"] = get_time_dhms(timelapse_time_seconds);
    j["model_print_time_seconds"] = normal_model_time;
    j["model_print_time_formatted"] = get_time_dhms(normal_model_time);
    j["initial_layer_time_seconds"] = initial_layer_time_seconds;
    j["initial_layer_time_formatted"] = get_time_dhms(initial_layer_time_seconds);

    if (stealth.time > 0.0f) {
        j["estimated_silent_print_time"] = get_time_dhms(stealth.time);
        j["estimated_silent_print_time_seconds"] = stealth.time;
    }

    j["time_modes"] = json::object();
    j["time_modes"]["normal"] = {
        {"time_seconds", normal.time},
        {"prepare_time_seconds", normal.prepare_time},
        {"model_print_time_seconds", normal_model_time},
        {"time_formatted", get_time_dhms(normal.time)},
        {"prepare_time_formatted", get_time_dhms(normal.prepare_time)},
        {"model_print_time_formatted", get_time_dhms(normal_model_time)}
    };
    if (stealth.time > 0.0f) {
        j["time_modes"]["stealth"] = {
            {"time_seconds", stealth.time},
            {"prepare_time_seconds", stealth.prepare_time},
            {"model_print_time_seconds", stealth_model_time},
            {"time_formatted", get_time_dhms(stealth.time)},
            {"prepare_time_formatted", get_time_dhms(stealth.prepare_time)},
            {"model_print_time_formatted", get_time_dhms(stealth_model_time)}
        };
    }

    double total_vol = 0.0;      // volume in mm3
    double total_weight = 0.0;   // weight in g
    double total_cost = 0.0;     // cost
    double total_filament_len = 0.0; // length in mm

    std::map<size_t, double> filament_usage; // extruder_id -> length

    for (const auto& [extruder_id, volume] : stats.total_volumes_per_extruder) {
        total_vol += volume;

        double diameter = 1.75;
        if (extruder_id < config.filament_diameter.values.size())
            diameter = config.filament_diameter.values[extruder_id];

        double density = 1.24; // Default PLA
        if (extruder_id < config.filament_density.values.size())
            density = config.filament_density.values[extruder_id];

        double cost = 0.0;
        if (extruder_id < config.filament_cost.values.size())
            cost = config.filament_cost.values[extruder_id]; // cost per kg

        double length = length_from_volume(volume, diameter);
        double weight = volume * density / 1000.0; // g

        total_filament_len += length;
        total_weight += weight;
        total_cost += weight * cost / 1000.0; // cost is per kg

        filament_usage[extruder_id] = length;
    }

    j["total_used_filament"] = total_filament_len;
    j["total_extruded_volume"] = total_vol;
    j["total_weight"] = total_weight;
    j["total_cost"] = total_cost;
    j["total_toolchanges"] = stats.total_extruder_changes;
    j["total_filament_changes"] = stats.total_filament_changes;
    j["total_extruder_changes"] = stats.total_extruder_changes;
    j["total_nozzle_changes"] = stats.total_nozzle_changes;

    json filament_stats = json::object();
    for (const auto& pair : filament_usage) {
        filament_stats[std::to_string(pair.first)] = pair.second;
    }
    j["filament_stats"] = filament_stats;

    j["filament_usage_mm3"] = json::object();
    j["filament_usage_mm3"]["total"] = volume_map_to_json(stats.total_volumes_per_extruder);
    j["filament_usage_mm3"]["model"] = volume_map_to_json(stats.model_volumes_per_extruder);
    j["filament_usage_mm3"]["support"] = volume_map_to_json(stats.support_volumes_per_extruder);
    j["filament_usage_mm3"]["wipe_tower"] = volume_map_to_json(stats.wipe_tower_volumes_per_extruder);

    j["filament_usage_mm"] = json::object();
    j["filament_usage_mm"]["total"] = length_map_to_json(stats.total_volumes_per_extruder);
    j["filament_usage_mm"]["model"] = length_map_to_json(stats.model_volumes_per_extruder);
    j["filament_usage_mm"]["support"] = length_map_to_json(stats.support_volumes_per_extruder);
    j["filament_usage_mm"]["wipe_tower"] = length_map_to_json(stats.wipe_tower_volumes_per_extruder);

    j["volumes_per_color_change_mm3"] = stats.volumes_per_color_change;
    j["flush_per_filament_mm3"] = volume_map_to_json(stats.flush_per_filament);

    return j.dump(4);
}

static std::string generate_config_json(const ConfigBase& config) {
    json j;

    for (const std::string &opt_key : config.keys()) {
        const ConfigOption* opt = config.option(opt_key);
        if (opt == nullptr) {
            continue;
        }

        if (opt->is_scalar()) {
            if (opt->type() == coString) {
                j[opt_key] = static_cast<const ConfigOptionString*>(opt)->value;
            } else {
                j[opt_key] = opt->serialize();
            }
        } else {
            const ConfigOptionVectorBase* vec = static_cast<const ConfigOptionVectorBase*>(opt);
            std::vector<std::string> string_values = vec->vserialize();
            j[opt_key] = string_values;
        }
    }

    return j.dump(4);
}

/* ============================================================================
 * Context Management
 * ============================================================================ */

SlicerContext* slicer_create(void) {
    try {
        return new SlicerContext();
    } catch (const std::exception& e) {
        return nullptr;
    }
}

void slicer_destroy(SlicerContext* ctx) {
    delete ctx;
}

/* ============================================================================
 * Model Loading
 * ============================================================================ */

int slicer_load_model(SlicerContext* ctx, const char* model_path) {
    int err = validate_context(ctx);
    if (err != SLICER_SUCCESS) return err;
    
    if (!model_path) {
        ctx->set_error("Model path is NULL");
        return SLICER_ERROR_NULL_PARAMETER;
    }
    
    try {
        std::string path_str(model_path);
        std::string extension = path_str.substr(path_str.find_last_of('.') + 1);
        
        // Convert extension to lowercase
        std::transform(extension.begin(), extension.end(), extension.begin(), ::tolower);
        
        // Create new model
        ctx->model = std::make_unique<Model>();
        
        // Load based on file extension
        bool load_result = false;
        
        if (extension == "3mf") {
            // Use BBS 3MF loader
            PlateDataPtrs plate_data_list;
            std::vector<Preset*> project_presets;
            ConfigSubstitutionContext config_substitutions(ForwardCompatibilitySubstitutionRule::Enable);
            bool is_bbl_3mf = false;
            
            load_result = load_bbs_3mf(
                path_str.c_str(),
                &ctx->config,
                &config_substitutions,
                ctx->model.get(),
                &plate_data_list,
                &project_presets,
                &is_bbl_3mf,
                nullptr
            );
        } else if (extension == "stl") {
            load_result = load_stl(path_str.c_str(), ctx->model.get());
        } else if (extension == "amf") {
            DynamicPrintConfig temp_config;
            ConfigSubstitutionContext config_substitutions(ForwardCompatibilitySubstitutionRule::Enable);
            bool import_check_result = false;
            load_result = load_amf(path_str.c_str(), &temp_config, &config_substitutions, ctx->model.get(), &import_check_result);
        } else if (extension == "obj") {
            // OBJ loading - requires ObjInfo and message parameters
            ObjInfo vertex_colors;
            std::string obj_message;
            load_result = load_obj(path_str.c_str(), ctx->model.get(), vertex_colors, obj_message);
        } else {
            ctx->set_error("Unsupported file format: " + extension);
            return SLICER_ERROR_MODEL_LOAD;
        }
        
        if (!load_result || ctx->model->objects.empty()) {
            ctx->set_error("Failed to load model from file: " + path_str);
            return SLICER_ERROR_MODEL_LOAD;
        }
        
        ctx->model_loaded = true;
        ctx->processed = false; // Invalidate previous processing
        return SLICER_SUCCESS;
        
    } catch (const std::exception& e) {
        ctx->set_error(std::string("Exception loading model: ") + e.what());
        return SLICER_ERROR_MODEL_LOAD;
    }
}

/* ============================================================================
 * Configuration
 * ============================================================================ */

int slicer_load_config_from_json(SlicerContext* ctx, const char* config_json) {
    int err = validate_context(ctx);
    if (err != SLICER_SUCCESS) return err;
    
    if (!config_json) {
        ctx->set_error("Config JSON is NULL");
        return SLICER_ERROR_NULL_PARAMETER;
    }
    
    try {
        // TODO: Parse JSON and apply to config
        // For now, this is a placeholder
        // We'll need to implement JSON parsing or accept INI format
        ctx->set_error("JSON config loading not yet implemented");
        return SLICER_ERROR_CONFIG_PARSE;
        
    } catch (const std::exception& e) {
        ctx->set_error(std::string("Exception parsing config: ") + e.what());
        return SLICER_ERROR_CONFIG_PARSE;
    }
}

int slicer_load_preset(
    SlicerContext* ctx,
    const char* printer,
    const char* filament,
    const char* process
) {
    int err = validate_context(ctx);
    if (err != SLICER_SUCCESS) return err;
    
    try {
        // Lazy initialize presets
        if (!ctx->presets_loaded) {
            ctx->app_config = std::make_unique<AppConfig>();
            ctx->app_config->set_defaults();
            
            // Set resources directory if not set (important for Docker)
            if (resources_dir().empty()) {
                // Check if /app/resources exists
                 // For now, hardcode to typical docker location or expect environment
                 set_resources_dir("/app/resources");
            }
            
            // Set data directory if not set (needed for PresetBundle to find "system" folder)
            if (data_dir().empty()) {
                 set_data_dir("/app/resources");
            }
            
            ctx->preset_bundle = std::make_unique<PresetBundle>();
            std::cerr << "SlicerCAPI: Loading presets into PresetBundle..." << std::endl;
            ctx->preset_bundle->load_presets(*ctx->app_config, ForwardCompatibilitySubstitutionRule::EnableSystemSilent);
            std::cerr << "SlicerCAPI: Presets loaded into PresetBundle." << std::endl;
            
            ctx->presets_loaded = true;
        }
        
        // Select Printer
        if (printer && *printer) {
            std::string printer_name(printer);
            std::cerr << "SlicerCAPI: Attempting to load printer preset: " << printer_name << std::endl;
            
            if (!ctx->preset_bundle->printers.find_preset(printer_name, false)) {
                std::cerr << "SlicerCAPI: Printer preset NOT found: " << printer_name << std::endl;
                
                // Log available presets
                std::cerr << "Available printer presets:" << std::endl;
                for (const auto& preset : ctx->preset_bundle->printers) {
                    std::cerr << "  - " << preset.name << std::endl;
                }

                // Fallback: try to find a preset that contains the requested name or vice versa
                std::string best_match;
                for (const auto& preset : ctx->preset_bundle->printers) {
                    if (preset.name.find(printer_name) != std::string::npos || 
                        printer_name.find(preset.name) != std::string::npos) {
                        best_match = preset.name;
                        break;
                    }
                }

                if (!best_match.empty()) {
                    std::cerr << "SlicerCAPI: Found fallback printer preset: " << best_match << std::endl;
                    printer_name = best_match;
                } else {
                    ctx->set_error(std::string("Printer preset not found: ") + printer_name);
                    return SLICER_ERROR_PRESET_NOT_FOUND;
                }
            }
            
            ctx->preset_bundle->printers.select_preset_by_name(printer_name, true, true);
            ctx->selected_printer_preset = ctx->preset_bundle->printers.get_selected_preset().name;

            if (ctx->selected_printer_preset != printer_name) {
                ctx->set_error(std::string("Printer preset not selected: requested '") +
                               printer_name + "', selected '" + ctx->selected_printer_preset + "'");
                return SLICER_ERROR_PRESET_NOT_FOUND;
            }
            
            // Apply to context config
            ctx->config.apply(ctx->preset_bundle->printers.get_selected_preset().config);
            std::cerr << "SlicerCAPI: Printer preset selected and applied: " << ctx->selected_printer_preset << std::endl;
            
            // Check printable_area immediately (BambuStudio uses printable_area, not bed_shape)
            if (ctx->config.has("printable_area")) {
                 std::cerr << "SlicerCAPI: printable_area is present in loaded config." << std::endl;
            } else {
                 std::cerr << "SlicerCAPI: WARNING: printable_area is MISSING in loaded config!" << std::endl;
            }
        }

        // Select Filament
        if (filament && *filament) {
            std::string filament_name(filament);
            std::cerr << "SlicerCAPI: Attempting to load filament preset: " << filament_name << std::endl;
            if (!ctx->preset_bundle->filaments.find_preset(filament_name, false)) {
                std::cerr << "SlicerCAPI: Filament preset NOT found: " << filament_name << std::endl;
                ctx->set_error(std::string("Filament preset not found: ") + filament_name);
                return SLICER_ERROR_PRESET_NOT_FOUND;
            }
            ctx->preset_bundle->filaments.select_preset_by_name(filament_name, true, true);
            ctx->selected_filament_preset = ctx->preset_bundle->filaments.get_selected_preset().name;

            if (ctx->selected_filament_preset != filament_name) {
                ctx->set_error(std::string("Filament preset not selected: requested '") +
                               filament_name + "', selected '" + ctx->selected_filament_preset + "'");
                return SLICER_ERROR_PRESET_NOT_FOUND;
            }
            std::cerr << "SlicerCAPI: Filament preset selected: " << ctx->selected_filament_preset << std::endl;
        }

        // Select Process
        if (process && *process) {
            std::string process_name(process);
            std::cerr << "SlicerCAPI: Attempting to load process preset: " << process_name << std::endl;
            if (!ctx->preset_bundle->prints.find_preset(process_name, false)) {
                std::cerr << "SlicerCAPI: Process preset NOT found: " << process_name << std::endl;
                ctx->set_error(std::string("Process preset not found: ") + process_name);
                return SLICER_ERROR_PRESET_NOT_FOUND;
            }
            ctx->preset_bundle->prints.select_preset_by_name(process_name, true, true);
            ctx->selected_process_preset = ctx->preset_bundle->prints.get_selected_preset().name;

            if (ctx->selected_process_preset != process_name) {
                ctx->set_error(std::string("Process preset not selected: requested '") +
                               process_name + "', selected '" + ctx->selected_process_preset + "'");
                return SLICER_ERROR_PRESET_NOT_FOUND;
            }
            std::cerr << "SlicerCAPI: Process preset selected: " << ctx->selected_process_preset << std::endl;
        }

        // Apply config to context
        ctx->config = ctx->preset_bundle->full_config();
        std::cerr << "SlicerCAPI: Full config applied from preset bundle." << std::endl;

        // FORCE RE-APPLY PRINTER CONFIG
        // Sometimes full_config() might not merge correctly if there are inheritance issues
        // Re-applying the selected printer config ensures machine limits are set
        // if (printer && *printer) {
        //      std::cerr << "SlicerCAPI: Re-applying selected printer preset config to ensure precedence..." << std::endl;
        //      ctx->config.apply(ctx->preset_bundle->printers.get_selected_preset().config);
             
        //      // Check acceleration AGAIN
        //      if (ctx->config.has("machine_max_acceleration_x")) {
        //          const ConfigOptionFloats* acc = ctx->config.opt<ConfigOptionFloats>("machine_max_acceleration_x");
        //          if (acc && !acc->values.empty()) {
        //              std::cerr << "SlicerCAPI: CONFIG AFTER RE-APPLY machine_max_acceleration_x: " << acc->values[0] << std::endl;
        //          }
        //      }
        // }
        
        ctx->config_loaded = true;
        ctx->config_json.clear();
        ctx->preset_info_json.clear();
        ctx->processed = false; // Invalidate previous processing
        std::cerr << "SlicerCAPI: slicer_load_preset finished successfully." << std::endl;
        return SLICER_SUCCESS;
        
    } catch (const std::exception& e) {
        std::cerr << "SlicerCAPI: Exception loading preset: " << e.what() << std::endl;
        ctx->set_error(std::string("Exception loading preset: ") + e.what());
        return SLICER_ERROR_PRESET_NOT_FOUND;
    }
}

int slicer_set_config_param(
    SlicerContext* ctx,
    const char* key,
    const char* value
) {
    int err = validate_context(ctx);
    if (err != SLICER_SUCCESS) return err;
    
    if (!key || !value) {
        ctx->set_error("Config key or value is NULL");
        return SLICER_ERROR_NULL_PARAMETER;
    }
    
    try {
        std::string key_str(key);
        std::string value_str(value);
        
        // Try to set the config option
        ConfigSubstitutionContext substitution_context(ForwardCompatibilitySubstitutionRule::Enable);
        bool success = ctx->config.set_deserialize_nothrow(key_str, value_str, substitution_context, false);
        
        if (!success) {
            ctx->set_error("Failed to set config parameter: " + key_str);
            return SLICER_ERROR_CONFIG_PARSE;
        }
        
        ctx->config_loaded = true;
        ctx->config_json.clear();
        ctx->processed = false; // Invalidate previous processing
        return SLICER_SUCCESS;
        
    } catch (const std::exception& e) {
        ctx->set_error(std::string("Exception setting config param: ") + e.what());
        return SLICER_ERROR_CONFIG_PARSE;
    }
}

/* ============================================================================
 * Slicing Operations
 * ============================================================================ */

int slicer_process(SlicerContext* ctx) {
    int err = validate_context(ctx);
    if (err != SLICER_SUCCESS) return err;
    
    if (!ctx->model_loaded) {
        ctx->set_error("No model loaded");
        return SLICER_ERROR_NO_MODEL;
    }
    
    if (!ctx->config_loaded) {
        ctx->set_error("No configuration loaded");
        return SLICER_ERROR_NO_CONFIG;
    }
    
    try {
        // Apply model to print
        std::cerr << "SlicerCAPI: Preparing to apply model to print..." << std::endl;

        if (!ctx->model) {
            std::cerr << "SlicerCAPI: ERROR - ctx->model is NULL!" << std::endl;
             return SLICER_ERROR_PROCESS_FAILED;
        }

        std::cerr << "SlicerCAPI: Creating Print object..." << std::endl;
        ctx->print = std::make_unique<Print>();
        
        // Ensure objects are on the bed
        std::cerr << "SlicerCAPI: Adding default instances..." << std::endl;
        try {
            ctx->model->add_default_instances();
        } catch (const std::exception& e) {
             std::cerr << "SlicerCAPI: Exception in add_default_instances: " << e.what() << std::endl;
        } catch (...) {
             std::cerr << "SlicerCAPI: Unknown exception in add_default_instances" << std::endl;
        }
        
        // Center objects around bed center
        // Center objects around bed center
        std::cerr << "SlicerCAPI: Getting printable_area (bed_shape)..." << std::endl;
        const ConfigOptionPoints* printable_area = ctx->config.opt<ConfigOptionPoints>("printable_area");
        if (printable_area) {
            std::cerr << "SlicerCAPI: printable_area found. Points: " << printable_area->values.size() << std::endl;
            for(const auto& p : printable_area->values) {
                std::cerr << "  Point: " << p(0) << ", " << p(1) << std::endl;
            }
            BoundingBoxf bed_bbox(printable_area->values);
            std::cerr << "SlicerCAPI: Bed center: " << bed_bbox.center()(0) << ", " << bed_bbox.center()(1) << std::endl;
            
            std::cerr << "SlicerCAPI: Centering instances..." << std::endl;
            try {
                ctx->model->center_instances_around_point(bed_bbox.center());
                
                // Log new bounding box
                BoundingBoxf3 new_bbox = ctx->model->bounding_box();
                std::cerr << "SlicerCAPI: Model bounding box after centering:" << std::endl;
                std::cerr << "  Min: " << new_bbox.min(0) << ", " << new_bbox.min(1) << ", " << new_bbox.min(2) << std::endl;
                std::cerr << "  Max: " << new_bbox.max(0) << ", " << new_bbox.max(1) << ", " << new_bbox.max(2) << std::endl;
                
                // Log object/instance counts
                std::cerr << "SlicerCAPI: Model stats:" << std::endl;
                std::cerr << "  Objects: " << ctx->model->objects.size() << std::endl;
                for (size_t i = 0; i < ctx->model->objects.size(); ++i) {
                     std::cerr << "  Object " << i << " instances: " << ctx->model->objects[i]->instances.size() << std::endl;
                }

            } catch (const std::exception& e) {
                 std::cerr << "SlicerCAPI: Exception in center_instances_around_point: " << e.what() << std::endl;
            } catch (...) {
                 std::cerr << "SlicerCAPI: Unknown exception in center_instances_around_point" << std::endl;
            }
        } else {
            std::cerr << "SlicerCAPI: bed_shape NOT found in config!" << std::endl;
        }

        std::cerr << "SlicerCAPI: Applying model to print..." << std::endl;
        
        // // Debug config values
        // if (ctx->config.has("machine_max_acceleration_x")) {
        //      const ConfigOptionFloats* acc = ctx->config.opt<ConfigOptionFloats>("machine_max_acceleration_x");
        //      if (acc && !acc->values.empty()) {
        //          std::cerr << "SlicerCAPI: DEBUG CONFIG machine_max_acceleration_x: " << acc->values[0] << std::endl;
                 
        //          // SAFETY OVERRIDE: If acceleration is 1000 (default) and printer seems to be A1, force it.
        //          // This covers cases where inheritance fails or defaults stick.
        //          if (acc->values[0] <= 1000.0) {
        //              std::string printer_model = ctx->config.opt_string("printer_model");
        //              if (printer_model.find("A1") != std::string::npos || ctx->config.opt_string("printer_settings_id").find("A1") != std::string::npos) {
        //                  std::cerr << "SlicerCAPI: WARNING - Detected A1 printer with default acceleration (1000). Forcing override to 12000." << std::endl;
                         
        //                  // Force set correct values for A1
        //                  ConfigOptionFloats* acc_mut = ctx->config.opt<ConfigOptionFloats>("machine_max_acceleration_x", true);
        //                  if(acc_mut) acc_mut->values = {12000, 12000};
                         
        //                  ConfigOptionFloats* acc_y = ctx->config.opt<ConfigOptionFloats>("machine_max_acceleration_y", true);
        //                  if(acc_y) acc_y->values = {12000, 12000};
                         
        //                  ConfigOptionFloats* acc_e = ctx->config.opt<ConfigOptionFloats>("machine_max_acceleration_extruding", true);
        //                  if(acc_e) acc_e->values = {12000, 12000};
                         
        //                  std::cerr << "SlicerCAPI: Acceleration override applied." << std::endl;
        //              }
        //          }
        //      }
        // } else {
        //      std::cerr << "SlicerCAPI: DEBUG CONFIG machine_max_acceleration_x NOT SET in ctx->config" << std::endl;
        // }

        ctx->print->apply(*ctx->model, ctx->config);
        
        // Process the print
        std::cerr << "SlicerCAPI: Processing print..." << std::endl;
        ctx->print->process();
        
        // Validate
        std::cerr << "SlicerCAPI: Validating..." << std::endl;
        StringObjectException validation_result = ctx->print->validate();
        if (!validation_result.string.empty()) {
            std::cerr << "SlicerCAPI: Validation failed: " << validation_result.string << std::endl;
            ctx->set_error("Print validation failed: " + validation_result.string);
            return SLICER_ERROR_PROCESS_FAILED;
        }
        
        std::cerr << "SlicerCAPI: Processing complete." << std::endl;
        ctx->processed = true;
        return SLICER_SUCCESS;
        
    } catch (const std::exception& e) {
        ctx->set_error(std::string("Exception during processing: ") + e.what());
        return SLICER_ERROR_PROCESS_FAILED;
    }
}

int slicer_export_gcode(SlicerContext* ctx, const char* output_path) {
    std::cerr << "SlicerCAPI: slicer_export_gcode called" << std::endl;
    int err = validate_context(ctx);
    if (err != SLICER_SUCCESS) return err;
    
    if (!ctx->processed) {
        std::cerr << "SlicerCAPI: Model not processed yet" << std::endl;
        ctx->set_error("Model not processed yet");
        return SLICER_ERROR_PROCESS_FAILED;
    }
    
    if (!output_path) {
        std::cerr << "SlicerCAPI: Output path is NULL" << std::endl;
        ctx->set_error("Output path is NULL");
        return SLICER_ERROR_NULL_PARAMETER;
    }
    
    try {
        std::string path_str(output_path);
        std::cerr << "SlicerCAPI: Exporting G-code to: " << path_str << std::endl;
        
        // Export G-code
        if (!ctx->print) {
            std::cerr << "SlicerCAPI: ctx->print is null!" << std::endl;
             return SLICER_ERROR_PROCESS_FAILED;
        }

        GCodeProcessorResult result; // Create on stack
        std::cerr << "SlicerCAPI: Calling print->export_gcode with result object..." << std::endl;
        std::string exported_path = ctx->print->export_gcode(path_str, &result);
        std::cerr << "SlicerCAPI: Export completed. Path: " << exported_path << std::endl;
        
        // Log stats from result
        // Log stats from result
        std::cerr << "SlicerCAPI: Export Stats:" << std::endl;
        
        // Debug Result
        std::cerr << "SlicerCAPI: Result modes size: " << result.print_statistics.modes.size() << std::endl;
        if (result.print_statistics.modes.size() > 0) {
             std::cerr << "  Estimated time (Normal): " << result.print_statistics.modes[0].time << "s" << std::endl;
        } else {
             std::cerr << "  Estimated time: N/A (modes empty)" << std::endl;
        }
        
        // Log filament usage per extruder
        if (!result.print_statistics.total_volumes_per_extruder.empty()) {
            for (const auto& [extruder_id, volume] : result.print_statistics.total_volumes_per_extruder) {
                std::cerr << "  Filament used (extruder " << extruder_id << "): " << volume << " mm3" << std::endl;
            }
        } else {
            std::cerr << "  Filament used: N/A (stats empty)" << std::endl;
        }

        // Cache stats JSON
        std::cerr << "SlicerCAPI: Caching stats JSON from export result..." << std::endl;
        double timelapse_time = 0.0;
        auto tl_it = result.skippable_part_time.find(SkipType::stTimelapse);
        if (tl_it != result.skippable_part_time.end())
            timelapse_time = tl_it->second;

        ctx->stats_json = generate_stats_json(
            result.print_statistics,
            ctx->print->config(),
            timelapse_time,
            result.initial_layer_time
        );
        std::cerr << "SlicerCAPI: Stats JSON cached." << std::endl;

        // Debug Config Limits
        if (ctx->print) {
            const auto& conf = ctx->print->config();
            // machine_max_acceleration_x is a ConfigOptionFloatsNullable
            if (!conf.machine_max_acceleration_x.values.empty()) {
                 std::cerr << "SlicerCAPI: machine_max_acceleration_x: " << conf.machine_max_acceleration_x.values[0] << std::endl;
            } else {
                 std::cerr << "SlicerCAPI: machine_max_acceleration_x is empty/not found" << std::endl;
            }

            // machine_max_speed_x is a ConfigOptionFloatsNullable
            if (!conf.machine_max_speed_x.values.empty()) {
                 std::cerr << "SlicerCAPI: machine_max_speed_x: " << conf.machine_max_speed_x.values[0] << std::endl;
            } else {
                 std::cerr << "SlicerCAPI: machine_max_speed_x is empty/not found" << std::endl;
            }
        }
        
        // Also try to get structured stats if possible, similar to get_stats_json but from result
        // (For now just basic logging to confirm it works)
        
        if (exported_path.empty()) {
            std::cerr << "SlicerCAPI: Failed to export G-code (empty path returned)" << std::endl;
            ctx->set_error("Failed to export G-code");
            return SLICER_ERROR_EXPORT_FAILED;
        }
        
        return SLICER_SUCCESS;
        
    } catch (const std::exception& e) {
        std::cerr << "SlicerCAPI: Exception during export: " << e.what() << std::endl;
        ctx->set_error(std::string("Exception during export: ") + e.what());
        return SLICER_ERROR_EXPORT_FAILED;
    }
}

int slicer_slice_and_export(SlicerContext* ctx, const char* output_path) {
    int err = slicer_process(ctx);
    if (err != SLICER_SUCCESS) return err;
    
    return slicer_export_gcode(ctx, output_path);
}

/* ============================================================================
 * Statistics & Results
 * ============================================================================ */

const char* slicer_get_stats_json(SlicerContext* ctx) {
    std::cerr << "SlicerCAPI: slicer_get_stats_json called" << std::endl;
    if (!ctx) {
        std::cerr << "SlicerCAPI: ctx is null" << std::endl;
        return nullptr;
    }
    
    if (!ctx->processed) {
        std::cerr << "SlicerCAPI: Model not processed yet" << std::endl;
        ctx->set_error("Model not processed yet");
        return nullptr;
    }
    
    // Check for cached stats from export
    if (!ctx->stats_json.empty()) {
        std::cerr << "SlicerCAPI: Returning cached stats JSON from export." << std::endl;
        return ctx->stats_json.c_str();
    }
    
    try {
        std::cerr << "SlicerCAPI: Getting print statistics..." << std::endl;
        // Check if print object exists
        if (!ctx->print) {
            std::cerr << "SlicerCAPI: ctx->print is null!" << std::endl;
            return nullptr;
        }

        const PrintStatistics& stats = ctx->print->print_statistics();
        std::cerr << "SlicerCAPI: Stats retrieved. Generating JSON..." << std::endl;
        ctx->stats_json = generate_stats_json(stats);
        std::cerr << "SlicerCAPI: JSON generated: " << ctx->stats_json.substr(0, 50) << "..." << std::endl;
        return ctx->stats_json.c_str();
        
    } catch (const std::exception& e) {
        std::cerr << "SlicerCAPI: Exception getting statistics: " << e.what() << std::endl;
        ctx->set_error(std::string("Exception getting statistics: ") + e.what());
        return nullptr;
    }
}

const char* slicer_get_config_json(SlicerContext* ctx) {
    std::cerr << "SlicerCAPI: slicer_get_config_json called" << std::endl;
    if (!ctx) {
        std::cerr << "SlicerCAPI: ctx is null" << std::endl;
        return nullptr;
    }

    if (!ctx->config_loaded) {
        std::cerr << "SlicerCAPI: No configuration loaded" << std::endl;
        ctx->set_error("No configuration loaded");
        return nullptr;
    }

    try {
        if (ctx->config_json.empty()) {
            // Use the resolved DynamicPrintConfig stored on the context.
            // This avoids depending on Print::config() lifecycle details.
            ctx->config_json = generate_config_json(ctx->config);
        }

        return ctx->config_json.c_str();
    } catch (const std::exception& e) {
        std::cerr << "SlicerCAPI: Exception getting config JSON: " << e.what() << std::endl;
        ctx->set_error(std::string("Exception getting config JSON: ") + e.what());
        return nullptr;
    }
}

const char* slicer_get_preset_info_json(SlicerContext* ctx) {
    std::cerr << "SlicerCAPI: slicer_get_preset_info_json called" << std::endl;
    if (!ctx) {
        std::cerr << "SlicerCAPI: ctx is null" << std::endl;
        return nullptr;
    }

    try {
        if (ctx->preset_info_json.empty()) {
            json j;
            j["printer_preset"] = ctx->selected_printer_preset.empty()
                ? json(nullptr)
                : json(ctx->selected_printer_preset);
            j["filament_preset"] = ctx->selected_filament_preset.empty()
                ? json(nullptr)
                : json(ctx->selected_filament_preset);
            j["process_preset"] = ctx->selected_process_preset.empty()
                ? json(nullptr)
                : json(ctx->selected_process_preset);

            ctx->preset_info_json = j.dump(4);
        }

        return ctx->preset_info_json.c_str();
    } catch (const std::exception& e) {
        std::cerr << "SlicerCAPI: Exception getting preset info JSON: " << e.what() << std::endl;
        ctx->set_error(std::string("Exception getting preset info JSON: ") + e.what());
        return nullptr;
    }
}

/* ============================================================================
 * Error Handling
 * ============================================================================ */

const char* slicer_get_last_error(SlicerContext* ctx) {
    if (!ctx) return nullptr;
    return ctx->last_error.empty() ? nullptr : ctx->last_error.c_str();
}

void slicer_clear_error(SlicerContext* ctx) {
    if (ctx) {
        ctx->clear_error();
    }
}

/* ============================================================================
 * Version Information
 * ============================================================================ */

const char* slicer_get_version(void) {
    return "1.0.0";
}

const char* slicer_get_bambu_version(void) {
    return SLIC3R_VERSION;
}
