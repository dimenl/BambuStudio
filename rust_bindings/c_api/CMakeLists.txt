# C API Build Configuration

cmake_minimum_required(VERSION 3.13)

# Add the C API sources to the slicer library
set(SLICER_C_API_SOURCES
    ${CMAKE_CURRENT_LIST_DIR}/slicer_c_api.cpp
    ${CMAKE_CURRENT_LIST_DIR}/slicer_c_api.h
)

# This file is meant to be included from src/libslic3r/CMakeLists.txt
# It provides a clean way to add the C API without modifying core files

# Add sources to libslic3r target (will be included from parent CMakeLists.txt)
# The parent CMakeLists.txt should do:
# include(${CMAKE_SOURCE_DIR}/rust_bindings/c_api/CMakeLists.txt)
# target_sources(libslic3r PRIVATE ${SLICER_C_API_SOURCES})

# Install the C API header for Rust bindgen
install(FILES slicer_c_api.h 
        DESTINATION include/bambu_slicer)
