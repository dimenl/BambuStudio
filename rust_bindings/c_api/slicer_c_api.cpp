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

// JSON library for statistics output
#include <sstream>

using namespace Slic3r;

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
            
            ctx->preset_bundle = std::make_unique<PresetBundle>();
            ctx->preset_bundle->load_presets(*ctx->app_config, ForwardCompatibilitySubstitutionRule::EnableSystemSilent);
            
            ctx->presets_loaded = true;
        }
        
        // Select Printer
        if (printer && *printer) {
            if (!ctx->preset_bundle->printers.find_preset(printer, false)) {
                ctx->set_error(std::string("Printer preset not found: ") + printer);
                return SLICER_ERROR_PRESET_NOT_FOUND;
            }
            ctx->preset_bundle->printers.select_preset_by_name(printer, true);
        }

        // Select Filament
        if (filament && *filament) {
            if (!ctx->preset_bundle->filaments.find_preset(filament, false)) {
                ctx->set_error(std::string("Filament preset not found: ") + filament);
                return SLICER_ERROR_PRESET_NOT_FOUND;
            }
            ctx->preset_bundle->filaments.select_preset_by_name(filament, true);
        }

        // Select Process
        if (process && *process) {
            if (!ctx->preset_bundle->prints.find_preset(process, false)) {
                ctx->set_error(std::string("Process preset not found: ") + process);
                return SLICER_ERROR_PRESET_NOT_FOUND;
            }
            ctx->preset_bundle->prints.select_preset_by_name(process, true);
        }

        // Apply config to context
        ctx->config = ctx->preset_bundle->full_config();
        
        ctx->config_loaded = true;
        ctx->processed = false; // Invalidate previous processing
        return SLICER_SUCCESS;
        
    } catch (const std::exception& e) {
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
        ctx->print = std::make_unique<Print>();
        ctx->print->apply(*ctx->model, ctx->config);
        
        // Process the print
        ctx->print->process();
        
        // Validate
        StringObjectException validation_result = ctx->print->validate();
        if (!validation_result.string.empty()) {
            ctx->set_error("Print validation failed: " + validation_result.string);
            return SLICER_ERROR_PROCESS_FAILED;
        }
        
        ctx->processed = true;
        return SLICER_SUCCESS;
        
    } catch (const std::exception& e) {
        ctx->set_error(std::string("Exception during processing: ") + e.what());
        return SLICER_ERROR_PROCESS_FAILED;
    }
}

int slicer_export_gcode(SlicerContext* ctx, const char* output_path) {
    int err = validate_context(ctx);
    if (err != SLICER_SUCCESS) return err;
    
    if (!ctx->processed) {
        ctx->set_error("Model not processed yet");
        return SLICER_ERROR_PROCESS_FAILED;
    }
    
    if (!output_path) {
        ctx->set_error("Output path is NULL");
        return SLICER_ERROR_NULL_PARAMETER;
    }
    
    try {
        std::string path_str(output_path);
        
        // Export G-code
        GCodeProcessorResult* result = nullptr;
        std::string exported_path = ctx->print->export_gcode(path_str, result);
        
        if (exported_path.empty()) {
            ctx->set_error("Failed to export G-code");
            return SLICER_ERROR_EXPORT_FAILED;
        }
        
        return SLICER_SUCCESS;
        
    } catch (const std::exception& e) {
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
    if (!ctx) return nullptr;
    
    if (!ctx->processed) {
        ctx->set_error("Model not processed yet");
        return nullptr;
    }
    
    try {
        const PrintStatistics& stats = ctx->print->print_statistics();
        ctx->stats_json = generate_stats_json(stats);
        return ctx->stats_json.c_str();
        
    } catch (const std::exception& e) {
        ctx->set_error(std::string("Exception getting statistics: ") + e.what());
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
