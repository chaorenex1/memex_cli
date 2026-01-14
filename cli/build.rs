fn main() {
    // Only embed icon on Windows
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();

        // Set application icon (requires .ico format)
        // If app-icon.png exists, winres can convert it automatically
        if std::path::Path::new("../app-icon.png").exists() {
            res.set_icon("../app-icon.png");
        }

        // Get version from Cargo environment variables
        let version = env!("CARGO_PKG_VERSION");
        let major: Vec<&str> = version.split('.').collect();
        let version_string = format!(
            "{}.{}.{}.0",
            major.first().unwrap_or(&"1"),
            major.get(1).unwrap_or(&"0"),
            major.get(2).unwrap_or(&"0")
        );

        // Set application metadata with version information
        res.set("ProductName", "Memex CLI")
            .set("FileDescription", "Core logic for CodeCLI powered by Memex")
            .set("CompanyName", "chaorenex1")
            .set(
                "LegalCopyright",
                "Copyright © 2026 chaorenex1. Licensed under Apache-2.0",
            )
            .set("FileVersion", &version_string)
            .set("ProductVersion", version);

        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to compile Windows resources: {}", e);
        }
    }

    // Trigger rebuild when icon file changes
    println!("cargo:rerun-if-changed=../app-icon.png");
}
