//! Build script to handle protoc dependency for LanceDB on Windows.

use std::env;
use std::path::PathBuf;

fn main() {
    // Only need to handle this on Windows
    if env::var("CARGO_CFG_WINDOWS").is_ok() {
        // If PROTOC is already set, don't override it
        if env::var("PROTOC").is_ok() {
            return;
        }

        // Check if protoc is already in PATH
        if which::which("protoc.exe").is_ok() || which::which("protoc").is_ok() {
            return;
        }

        // Try to find protoc in common locations
        let common_paths = vec![
            PathBuf::from("C:\\Program Files\\protobuf\\bin\\protoc.exe"),
            PathBuf::from("C:\\msys64\\mingw64\\bin\\protoc.exe"),
            PathBuf::from("C:\\msys64\\ucrt64\\bin\\protoc.exe"),
        ];

        for path in common_paths {
            if path.exists() {
                println!("cargo:warning=Found protoc at: {:?}", path);
                env::set_var("PROTOC", path);
                return;
            }
        }

        // Not found - provide helpful error message
        println!("cargo:warning=protoc not found!");
        println!("cargo:warning=LanceDB requires protoc for compilation.");
        println!("cargo:warning=Please install protoc:");
        println!("cargo:warning=  1. Using: choco install protoc");
        println!("cargo:warning=  2. Or download from: https://github.com/protocolbuffers/protobuf/releases");
        println!("cargo:warning=  3. Or set PROTOC environment variable to the protoc.exe path");
    }
}
