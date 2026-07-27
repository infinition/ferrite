//! Attaches the icon and the version metadata to the Windows executable.
//!
//! A failure here is not fatal: with no resource compiler available the binary
//! still builds, only with the default icon.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let version = env!("CARGO_PKG_VERSION");

        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        resource.set("ProductName", "Ferrite");
        resource.set("FileDescription", "Ferrite, workspace cleanup");
        resource.set("CompanyName", "infinition");
        resource.set("LegalCopyright", "Copyright (c) infinition. MIT licensed.");
        resource.set("OriginalFilename", "Ferrite.exe");
        resource.set("InternalName", "Ferrite");
        resource.set("ProductVersion", version);
        resource.set("FileVersion", version);

        // Automatic detection of the resource compiler fails on some Windows
        // SDK layouts, so locate it explicitly.
        if let Some(toolkit) = find_resource_compiler() {
            resource.set_toolkit_path(&toolkit);
        }

        if let Err(error) = resource.compile() {
            println!("cargo:warning=icon not embedded: {error}");
        }
    }
}

/// Finds the directory holding `rc.exe`, preferring the most recent SDK.
/// Returns `None` when no resource compiler is installed.
#[cfg(windows)]
fn find_resource_compiler() -> Option<String> {
    let roots = [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ];

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    for root in roots {
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let directory = entry.path().join("x64");
            if directory.join("rc.exe").is_file() {
                candidates.push(directory);
            }
        }
    }

    candidates.sort();
    candidates
        .pop()
        .map(|path| path.to_string_lossy().to_string())
}
