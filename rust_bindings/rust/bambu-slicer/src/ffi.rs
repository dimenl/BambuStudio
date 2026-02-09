//! Raw FFI bindings to the C API
//!
//! This module contains auto-generated bindings from bindgen.
//! Use the safe wrappers in the parent module instead of calling these directly.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

// Include the generated bindings
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    #[test]
    fn test_ffi_module_compiles() {
        // This test just verifies the FFI module compiles
        // Actual symbol testing requires the C library to be linked
        assert!(true);
    }
}
