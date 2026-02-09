use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../../../rust_bindings/c_api/slicer_c_api.h");
    println!("cargo:rerun-if-env-changed=BAMBU_BUILD_DIR");

    // Get build directory from environment or use default
    let build_dir = env::var("BAMBU_BUILD_DIR").unwrap_or_else(|_| {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("build")
            .to_str()
            .unwrap()
            .to_string()
    });

    println!("cargo:warning=Using build directory: {}", build_dir);

    // Path to the C API header
    let c_api_header = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("rust_bindings/c_api/slicer_c_api.h");

    // Generate bindings
    let bindings = bindgen::Builder::default()
        .header(c_api_header.to_str().unwrap())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // Only generate bindings for our API
        .allowlist_function("slicer_.*")
        .allowlist_type("Slicer.*")
        .allowlist_var("SLICER_.*")
        // Generate docstrings
        .generate_comments(true)
        .generate()
        .expect("Unable to generate bindings");

    // Write bindings to file
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");
    bindings
        .write_to_file(&out_path)
        .expect("Couldn't write bindings!");

    println!("cargo:warning=✓ Bindings generated successfully");

    // Construct absolute paths based on build_dir (which is /BambuStudio/build_rust)
    let build_path = PathBuf::from(&build_dir);
    let project_root = build_path.parent().unwrap(); // /BambuStudio
    let src_dir = project_root.join("src"); // /BambuStudio/src
    let deps_include_dir = project_root.join("deps/build/destdir/usr/local/include");

    // Check if C API library exists
    let lib_path = PathBuf::from(&build_dir).join("src/libslic3r/liblibslic3r.a");

    if !lib_path.exists() {
        println!("cargo:warning=");
        println!("cargo:warning===========================================");
        println!("cargo:warning=  C API Library Not Built Yet");
        println!("cargo:warning===========================================");
        println!("cargo:warning=");
        println!("cargo:warning=The Rust bindings compiled successfully, but the");
        println!("cargo:warning=C API library needs to be built before linking.");
        println!("cargo:warning=");
        println!("cargo:warning=To build the C API:");
        println!(
            "cargo:warning=  1. Install dependencies: brew install boost opencv cgal tbb eigen"
        );
        println!("cargo:warning=  2. cd {}", project_root.display());
        println!("cargo:warning=  3. mkdir -p build_rust && cd build_rust");
        println!("cargo:warning=  4. cmake .. -DCMAKE_BUILD_TYPE=Release -DBUILD_RUST_BINDINGS=ON");
        println!("cargo:warning=  5. cmake --build . --target libslic3r -j8");
        println!("cargo:warning=");
        println!("cargo:warning=Expected library at: {}", lib_path.display());
        println!("cargo:warning===========================================");
        println!("cargo:warning=");

        // Don't fail the build, just warn
        // This allows cargo check to work even without the C library
        return;
    }

    println!(
        "cargo:warning=✓ Found C API library at: {}",
        lib_path.display()
    );

    // --- Compile Helper C++ Sources (BBLUtil, Http, NanoSVG) ---
    // These are needed because libslic3r depends on them (via LogSink and NSVGUtils),
    // but they are normally part of libslic3r_gui which we don't want to link fully.

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let nanosvg_impl_path = out_path.join("nanosvg_impl.c");
    std::fs::write(
        &nanosvg_impl_path,
        r#"
        #define NANOSVG_IMPLEMENTATION
        #include "nanosvg/nanosvg.h"
        #define NANOSVGRAST_IMPLEMENTATION
        #include "nanosvg/nanosvgrast.h"
    "#,
    )
    .expect("Failed to write nanosvg_impl.c");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file(src_dir.join("slic3r/Utils/BBLUtil.cpp"))
        .file(src_dir.join("slic3r/Utils/Http.cpp"))
        .file(&nanosvg_impl_path)
        // Include paths
        .include(&src_dir) // for "slic3r/Utils/..." includes (root of src)
        .include(src_dir.join("libslic3r")) // for source headers
        .include(build_path.join("src/libslic3r")) // for generated libslic3r_version.h
        .include(&deps_include_dir) // for boost, curl, openssl, etc
        .include("/usr/include") // system headers
        // Definitions
        .define("SLIC3R_STATIC", None)
        .define("BBL_RELEASE_TO_PUBLIC", Some("1")) // Assume 1
        .warnings(false)
        .compile("bbl_utils");

    // Link search paths
    // 1. libslic3r location
    println!("cargo:rustc-link-search=native={}/src/libslic3r", build_dir);

    // 2. Deps location & Internal libs location
    let mut search_paths = vec![
        // Try to find the deps directory (deps/build/destdir/usr/local/lib) relative to build_dir
        PathBuf::from(&build_dir)
            .parent()
            .unwrap()
            .join("deps/build/destdir/usr/local/lib"),
        // Fallback absolute path in container
        PathBuf::from("/BambuStudio/deps/build/destdir/usr/local/lib"),
        // System paths
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/usr/lib"),
        PathBuf::from("/usr/lib/aarch64-linux-gnu"), // For gmp/mpfr/fontconfig on arm64 docker
        PathBuf::from("/usr/lib/x86_64-linux-gnu"),  // For amd64
    ];

    // Add internal source dirs that might contain static libs
    let internal_dirs = vec![
        "admesh",
        "boost", // contains libnowide.a
        "clipper",
        "clipper2",
        "glu-libtess",
        "libslic3r", // contains liblibslic3r_cgal.a
        "mcut",
        "miniz",
        "semver",
    ];
    for dir in internal_dirs {
        search_paths.push(PathBuf::from(&build_dir).join("src").join(dir));
    }

    // Print all search paths
    for path in search_paths {
        if path.exists() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }

    // Link C++ Standard Library
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=c++");

    #[cfg(not(target_os = "macos"))]
    println!("cargo:rustc-link-lib=stdc++");

    // Link libslic3r (Core Slicer)
    println!("cargo:rustc-link-lib=static=libslic3r");

    // Link Dependencies
    let libs = vec![
        // --- Internal Static Libs ---
        "libslic3r_cgal",
        "miniz_static",
        "semver",
        "admesh",
        "clipper",
        "Clipper2",
        "nowide",
        "glu-libtess",
        "mcut",
        // --- Core Dependencies ---
        "tbb",
        "tbbmalloc", // Needed for scalable_malloc
        "z",
        "expat",
        "curl",
        // --- OpenSSL (for MD5, etc.) ---
        "ssl",
        "crypto",
        // --- GMP & MPFR (for CGAL) ---
        "gmp",
        "mpfr",
        // --- OCCT (OpenCASCADE) ---
        "TKXDESTEP",
        "TKXDEIGES",
        "TKSTEP",
        "TKSTEP209",
        "TKSTEPAttr",
        "TKSTEPBase",
        "TKIGES",
        "TKXSBase",
        "TKXCAF",
        "TKVCAF",
        "TKCAF",
        "TKLCAF",
        "TKCDF",
        "TKXmlXCAF",
        "TKXmlL",
        "TKXml",
        "TKBinXCAF",
        "TKBinL",
        "TKBin",
        "TKSTL",
        "TKVRML",
        "TKRWMesh",
        "TKMesh",
        "TKHLR",
        "TKShHealing",
        "TKTopAlgo",
        "TKGeomAlgo",
        "TKGeomBase",
        "TKBRep",
        "TKPrim",
        "TKBO",
        "TKBool",
        "TKG3d",
        "TKG2d",
        "TKMath",
        "TKService",
        "TKV3d",
        "TKernel",
        // --- Multimedia / Graphics ---
        // Moved after OCCT because TKService depends on FreeType/FontConfig
        "png16",
        "freetype",
        "fontconfig",
        "qhullcpp",
        "qhullstatic_r",
        // --- Boost ---
        "boost_system",
        "boost_filesystem",
        "boost_thread",
        "boost_log",
        "boost_locale",
        "boost_regex",
        "boost_chrono",
        "boost_atomic",
        "boost_date_time",
        "boost_iostreams",
    ];

    for lib in libs {
        println!("cargo:rustc-link-lib={}", lib);
    }

    println!("cargo:warning=Linking against updated list of dependencies including compiled utils and OCCT/Multimedia libs.");
}
