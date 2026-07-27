//! Attache l'icone et les metadonnees a l'executable Windows.
//!
//! L'echec de cette etape n'est pas bloquant: sans compilateur de ressources
//! disponible, le binaire se construit quand meme, avec l'icone par defaut.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");

        // La detection automatique du compilateur de ressources echoue sur
        // certaines installations du SDK Windows: on le localise nous-memes.
        if let Some(toolkit) = find_resource_compiler() {
            resource.set_toolkit_path(&toolkit);
        }
        resource.set("ProductName", "Ferrite");
        resource.set("FileDescription", "Ferrite, nettoyage de workspace");
        resource.set("CompanyName", "infinition");
        resource.set("LegalCopyright", "infinition");

        if let Err(error) = resource.compile() {
            println!("cargo:warning=icone non embarquee: {error}");
        }
    }
}

/// Cherche le dossier contenant `rc.exe`, en prenant la version de SDK la plus
/// recente. Retourne `None` si aucun compilateur de ressources n'est present.
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
    candidates.pop().map(|path| path.to_string_lossy().to_string())
}
