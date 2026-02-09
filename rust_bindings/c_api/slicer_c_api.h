#ifndef BAMBU_SLICER_C_API_H
#define BAMBU_SLICER_C_API_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>
#include <stdint.h>

/**
 * @file slicer_c_api.h
 * @brief C API for BambuStudio Slicer Core
 * 
 * This API provides a C-compatible interface to the BambuStudio slicer,
 * allowing it to be called from other languages like Rust.
 * 
 * Thread Safety: SlicerContext is NOT thread-safe. Each thread should
 * create its own context.
 */

/* ============================================================================
 * Opaque Types
 * ============================================================================ */

/**
 * @brief Opaque handle to the slicer context
 * 
 * This handle encapsulates all state needed for slicing operations,
 * including the loaded model, configuration, and error state.
 */
typedef struct SlicerContext SlicerContext;

/* ============================================================================
 * Error Codes
 * ============================================================================ */

typedef enum {
    SLICER_SUCCESS = 0,              /**< Operation succeeded */
    SLICER_ERROR_NULL_CONTEXT = 1,   /**< NULL context pointer provided */
    SLICER_ERROR_NULL_PARAMETER = 2, /**< NULL parameter provided */
    SLICER_ERROR_MODEL_LOAD = 3,     /**< Failed to load model file */
    SLICER_ERROR_CONFIG_PARSE = 4,   /**< Failed to parse configuration */
    SLICER_ERROR_PRESET_NOT_FOUND = 5, /**< Preset not found */
    SLICER_ERROR_NO_MODEL = 6,       /**< No model loaded */
    SLICER_ERROR_NO_CONFIG = 7,      /**< No configuration applied */
    SLICER_ERROR_PROCESS_FAILED = 8, /**< Slicing process failed */
    SLICER_ERROR_EXPORT_FAILED = 9,  /**< G-code export failed */
    SLICER_ERROR_IO = 10,            /**< I/O error (file read/write) */
    SLICER_ERROR_INTERNAL = 99       /**< Internal error (see error message) */
} SlicerErrorCode;

/* ============================================================================
 * Context Management
 * ============================================================================ */

/**
 * @brief Create a new slicer context
 * 
 * @return Pointer to new context, or NULL on allocation failure
 * 
 * @note Caller must free the context with slicer_destroy()
 */
SlicerContext* slicer_create(void);

/**
 * @brief Destroy a slicer context and free all resources
 * 
 * @param ctx Context to destroy (can be NULL)
 * 
 * @note After calling this, the context pointer is invalid
 */
void slicer_destroy(SlicerContext* ctx);

/* ============================================================================
 * Model Loading
 * ============================================================================ */

/**
 * @brief Load a 3D model from file
 * 
 * Supported formats: 3MF, STL, AMF, OBJ
 * 
 * @param ctx Slicer context
 * @param model_path Path to model file (UTF-8 encoded)
 * @return SLICER_SUCCESS on success, error code otherwise
 * 
 * @note This replaces any previously loaded model
 */
int slicer_load_model(SlicerContext* ctx, const char* model_path);

/* ============================================================================
 * Configuration
 * ============================================================================ */

/**
 * @brief Load configuration from JSON string
 * 
 * The JSON should contain configuration parameters compatible with
 * BambuStudio's config format.
 * 
 * @param ctx Slicer context
 * @param config_json JSON configuration string (UTF-8)
 * @return SLICER_SUCCESS on success, error code otherwise
 * 
 * @example
 * {
 *   "layer_height": 0.2,
 *   "sparse_infill_density": "15%",
 *   "support_enable": true,
 *   ...
 * }
 */
int slicer_load_config_from_json(SlicerContext* ctx, const char* config_json);

/**
 * @brief Load preset configuration
 * 
 * Loads a named preset from the bundled configurations.
 * 
 * @param ctx Slicer context
 * @param printer Printer preset name (e.g., "Bambu Lab A1 0.4 nozzle")
 * @param filament Filament preset name (e.g., "Bambu PLA Basic @BBL A1")
 * @param process Process preset name (e.g., "0.20mm Standard @BBL A1")
 * @return SLICER_SUCCESS on success, error code otherwise
 * 
 * @note NULL can be passed for any parameter to skip loading that preset type
 */
int slicer_load_preset(
    SlicerContext* ctx,
    const char* printer,
    const char* filament,
    const char* process
);

/**
 * @brief Set a single configuration parameter
 * 
 * @param ctx Slicer context
 * @param key Configuration key (e.g., "layer_height")
 * @param value Configuration value as string
 * @return SLICER_SUCCESS on success, error code otherwise
 * 
 * @note This is a convenience function for setting individual parameters
 */
int slicer_set_config_param(
    SlicerContext* ctx,
    const char* key,
    const char* value
);

/* ============================================================================
 * Slicing Operations
 * ============================================================================ */

/**
 * @brief Process the loaded model with current configuration
 * 
 * This performs all slicing operations (slicing, perimeter generation,
 * infill, support material, etc.) but does not export G-code.
 * 
 * @param ctx Slicer context
 * @return SLICER_SUCCESS on success, error code otherwise
 * 
 * @note Model and configuration must be loaded before calling this
 * @note This can take significant time for complex models
 */
int slicer_process(SlicerContext* ctx);

/**
 * @brief Export G-code to file
 * 
 * @param ctx Slicer context
 * @param output_path Path for output G-code file (UTF-8 encoded)
 * @return SLICER_SUCCESS on success, error code otherwise
 * 
 * @note slicer_process() must be called successfully before exporting
 */
int slicer_export_gcode(SlicerContext* ctx, const char* output_path);

/**
 * @brief Process and export in one step
 * 
 * Convenience function equivalent to calling slicer_process() followed
 * by slicer_export_gcode().
 * 
 * @param ctx Slicer context
 * @param output_path Path for output G-code file (UTF-8 encoded)
 * @return SLICER_SUCCESS on success, error code otherwise
 */
int slicer_slice_and_export(SlicerContext* ctx, const char* output_path);

/* ============================================================================
 * Statistics & Results
 * ============================================================================ */

/**
 * @brief Get slicing statistics as JSON string
 * 
 * Returns statistics about the sliced model including print time,
 * filament usage, weight, cost, etc.
 * 
 * @param ctx Slicer context
 * @return JSON string, or NULL on error
 * 
 * @note The returned string is owned by the context and will be
 *       invalidated on the next call to this function or when
 *       the context is destroyed. Copy if needed.
 * @note Only valid after successful call to slicer_process()
 * 
 * @example Return format:
 * {
 *   "estimated_print_time": "1h 23m",
 *   "total_used_filament": 1234.5,
 *   "total_extruded_volume": 9876.5,
 *   "total_weight": 15.3,
 *   "total_cost": 2.45,
 *   "total_toolchanges": 0,
 *   "filament_stats": {
 *     "0": 1234.5
 *   }
 * }
 */
const char* slicer_get_stats_json(SlicerContext* ctx);

/**
 * @brief Get the full resolved configuration as JSON string
 *
 * Returns all configuration keys and their serialized values after presets
 * and custom parameters are applied.
 *
 * @param ctx Slicer context
 * @return JSON string, or NULL on error
 *
 * @note The returned string is owned by the context and will be
 *       invalidated on the next call to this function or when
 *       the context is destroyed. Copy if needed.
 */
const char* slicer_get_config_json(SlicerContext* ctx);

/**
 * @brief Get selected preset names as JSON string
 *
 * Returns a JSON object with printer/filament/process preset names
 * that were actually selected.
 *
 * @param ctx Slicer context
 * @return JSON string, or NULL on error
 *
 * @note The returned string is owned by the context and will be
 *       invalidated on the next call to this function or when
 *       the context is destroyed. Copy if needed.
 */
const char* slicer_get_preset_info_json(SlicerContext* ctx);

/* ============================================================================
 * Error Handling
 * ============================================================================ */

/**
 * @brief Get last error message
 * 
 * @param ctx Slicer context
 * @return Error message string, or NULL if no error
 * 
 * @note The returned string is owned by the context and will be
 *       invalidated on the next operation or when the context
 *       is destroyed. Copy if needed.
 * @note The error message is cleared at the start of each operation
 */
const char* slicer_get_last_error(SlicerContext* ctx);

/**
 * @brief Clear the last error message
 * 
 * @param ctx Slicer context
 */
void slicer_clear_error(SlicerContext* ctx);

/* ============================================================================
 * Version Information
 * ============================================================================ */

/**
 * @brief Get API version string
 * 
 * @return Version string (e.g., "1.0.0")
 */
const char* slicer_get_version(void);

/**
 * @brief Get BambuStudio version string
 * 
 * @return Version string from the underlying BambuStudio build
 */
const char* slicer_get_bambu_version(void);

#ifdef __cplusplus
}
#endif

#endif /* BAMBU_SLICER_C_API_H */
