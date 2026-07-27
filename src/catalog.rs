//! Catalogue des artefacts regenerables, par ecosysteme.
//!
//! Les regles les plus specifiques, celles qui portent une contrainte `under`
//! ou `requires`, sont placees avant les regles generiques: la premiere qui
//! correspond gagne. C'est ce qui permet de distinguer un `target` Cargo d'un
//! `target` Maven, ou un `vendor` Composer d'un `vendor` Go.

use std::collections::HashSet;

pub const SAFE: &str = "safe";
pub const CHECK: &str = "check";
pub const DATA: &str = "data";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

pub struct Rule {
    pub id: &'static str,
    pub cat: &'static str,
    pub risk: &'static str,
    pub kind: Kind,
    pub names: &'static [&'static str],
    pub globs: &'static [&'static str],
    pub under: Option<&'static str>,
    pub requires: &'static [&'static str],
    pub restore: &'static str,
    pub ignore: &'static str,
}

const N: &[&str] = &[];

/// Regle simple sur un nom de dossier.
macro_rules! d {
    ($id:expr, $cat:expr, $risk:expr, [$($n:expr),*], $restore:expr, $ign:expr) => {
        Rule { id: $id, cat: $cat, risk: $risk, kind: Kind::Dir, names: &[$($n),*],
               globs: N, under: None, requires: N, restore: $restore, ignore: $ign }
    };
}

/// Regle simple sur un nom de fichier.
macro_rules! f {
    ($id:expr, $cat:expr, $risk:expr, [$($n:expr),*], $restore:expr, $ign:expr) => {
        Rule { id: $id, cat: $cat, risk: $risk, kind: Kind::File, names: &[$($n),*],
               globs: N, under: None, requires: N, restore: $restore, ignore: $ign }
    };
}

/// Regle sur des motifs de nom de fichier.
macro_rules! fg {
    ($id:expr, $cat:expr, $risk:expr, [$($g:expr),*], $restore:expr, $ign:expr) => {
        Rule { id: $id, cat: $cat, risk: $risk, kind: Kind::File, names: N,
               globs: &[$($g),*], under: None, requires: N, restore: $restore, ignore: $ign }
    };
}

/// Regle de dossier conditionnee a la presence de marqueurs voisins.
macro_rules! dreq {
    ($id:expr, $cat:expr, $risk:expr, [$($n:expr),*], [$($r:expr),*], $restore:expr, $ign:expr) => {
        Rule { id: $id, cat: $cat, risk: $risk, kind: Kind::Dir, names: &[$($n),*],
               globs: N, under: None, requires: &[$($r),*], restore: $restore, ignore: $ign }
    };
}

/// Regle de dossier conditionnee au nom du dossier parent.
macro_rules! dunder {
    ($id:expr, $cat:expr, $risk:expr, [$($n:expr),*], $under:expr, $restore:expr, $ign:expr) => {
        Rule { id: $id, cat: $cat, risk: $risk, kind: Kind::Dir, names: &[$($n),*],
               globs: N, under: Some($under), requires: N, restore: $restore, ignore: $ign }
    };
}

pub static RULES: &[Rule] = &[
    // ================= JavaScript / Node =================
    d!("node_modules", "js", SAFE, ["node_modules"], "npm install", "node_modules/"),
    d!("next", "js", SAFE, [".next"], "npm run build", ".next/"),
    d!("nuxt", "js", SAFE, [".nuxt"], "npm run build", ".nuxt/"),
    d!("nitro_output", "js", SAFE, [".output"], "npm run build", ".output/"),
    d!("svelte_kit", "js", SAFE, [".svelte-kit"], "npm run build", ".svelte-kit/"),
    d!("astro", "js", SAFE, [".astro"], "npm run build", ".astro/"),
    d!("docusaurus", "js", SAFE, [".docusaurus"], "npm run build", ".docusaurus/"),
    d!("angular_cache", "js", SAFE, [".angular"], "ng build", ".angular/"),
    d!("parcel_cache", "js", SAFE, [".parcel-cache"], "npm run build", ".parcel-cache/"),
    d!("turbo", "js", SAFE, [".turbo"], "npm run build", ".turbo/"),
    d!("vite_cache", "js", SAFE, [".vite"], "npm run dev", ".vite/"),
    d!("rollup_cache", "js", SAFE, [".rollup.cache"], "npm run build", ".rollup.cache/"),
    d!("webpack_cache", "js", SAFE, [".webpack"], "npm run build", ".webpack/"),
    d!("serverless", "js", SAFE, [".serverless"], "serverless package", ".serverless/"),
    dunder!("yarn_cache", "js", SAFE, ["cache", "unplugged"], ".yarn", "yarn install", ".yarn/cache/"),
    d!("pnpm_store", "js", SAFE, [".pnpm-store"], "pnpm install", ".pnpm-store/"),
    d!("expo", "js", SAFE, [".expo", ".expo-shared"], "npx expo start", ".expo/"),
    d!("metro_cache", "js", SAFE, [".metro-cache"], "npx react-native start", ".metro-cache/"),
    d!("storybook_static", "js", SAFE, ["storybook-static"], "npm run build-storybook", "storybook-static/"),
    d!("nyc_output", "js", SAFE, [".nyc_output"], "npm test", ".nyc_output/"),
    f!("eslintcache", "js", SAFE, [".eslintcache", ".stylelintcache"], "npm run lint", ".eslintcache"),
    fg!("tsbuildinfo", "js", SAFE, ["*.tsbuildinfo"], "tsc -b", "*.tsbuildinfo"),
    d!("nx_cache", "js", SAFE, [".nx"], "npx nx build", ".nx/"),
    d!("swc_cache", "js", SAFE, [".swc"], "npm run build", ".swc/"),
    d!("legacy_modules", "js", SAFE, ["bower_components", "jspm_packages", "web_modules"], "bower install", "bower_components/"),
    d!("deploy_cache", "js", SAFE, [".vercel", ".netlify", ".wrangler", ".firebase"], "deploy CLI", ".vercel/"),
    dunder!("vitepress_out", "js", SAFE, ["cache", "dist"], ".vitepress", "npm run docs:build", ".vitepress/cache/"),
    dunder!("vuepress_out", "js", SAFE, ["dist", "temp"], ".vuepress", "npm run docs:build", ".vuepress/dist/"),
    d!("playwright", "js", SAFE, ["playwright-report", "test-results", ".playwright", "blob-report"], "npx playwright test", "playwright-report/"),
    dunder!("cypress_artifacts", "js", SAFE, ["videos", "screenshots", "downloads"], "cypress", "npx cypress run", "cypress/videos/"),
    Rule { id: "rpt2_cache", cat: "js", risk: SAFE, kind: Kind::Dir, names: &[".rpt2_cache"],
           globs: &[".rts2_cache_*"], under: None, requires: N, restore: "npm run build", ignore: ".rpt2_cache/" },
    d!("legacy_task_cache", "js", SAFE, [".grunt", ".fusebox", ".dynamodb", ".node_modules_cache"], "npm run build", ".grunt/"),
    f!("npm_misc", "js", SAFE, [".yarn-integrity", ".node_repl_history"], "npm install", ".yarn-integrity"),

    // ================= Python =================
    d!("pycache", "python", SAFE, ["__pycache__"], "python", "__pycache__/"),
    d!("pytest_cache", "python", SAFE, [".pytest_cache"], "pytest", ".pytest_cache/"),
    d!("mypy_cache", "python", SAFE, [".mypy_cache"], "mypy .", ".mypy_cache/"),
    d!("ruff_cache", "python", SAFE, [".ruff_cache"], "ruff check .", ".ruff_cache/"),
    d!("tox", "python", SAFE, [".tox", ".nox"], "tox", ".tox/"),
    d!("hypothesis", "python", SAFE, [".hypothesis"], "pytest", ".hypothesis/"),
    d!("venv", "python", SAFE, ["venv", ".venv", "virtualenv", "env310", "env311"],
       "python -m venv venv && pip install -r requirements.txt", "venv/"),
    Rule { id: "egg_info", cat: "python", risk: SAFE, kind: Kind::Dir, names: N,
           globs: &["*.egg-info"], under: None, requires: N, restore: "pip install -e .", ignore: "*.egg-info/" },
    d!("eggs", "python", SAFE, [".eggs"], "pip install -e .", ".eggs/"),
    d!("ipynb_checkpoints", "python", SAFE, [".ipynb_checkpoints"], "jupyter", ".ipynb_checkpoints/"),
    d!("htmlcov", "python", SAFE, ["htmlcov"], "coverage html", "htmlcov/"),
    Rule { id: "coverage_file", cat: "python", risk: SAFE, kind: Kind::File, names: &[".coverage"],
           globs: &[".coverage.*"], under: None, requires: N, restore: "coverage run", ignore: ".coverage" },
    dreq!("py_dist", "python", CHECK, ["dist", "build"], ["setup.py", "pyproject.toml", "setup.cfg"],
          "python -m build", "dist/"),
    fg!("pyc", "python", SAFE, ["*.pyc", "*.pyo", "*.pyd"], "python", "*.py[cod]"),
    dreq!("mkdocs_site", "python", SAFE, ["site"], ["mkdocs.yml", "mkdocs.yaml"], "mkdocs build", "site/"),
    dunder!("sphinx_build", "python", SAFE, ["_build"], "docs", "sphinx-build", "docs/_build/"),
    d!("pdm_packages", "python", SAFE, ["__pypackages__", ".pdm-build"], "pdm install", "__pypackages__/"),
    d!("type_checker_cache", "python", SAFE, [".pyre", ".pytype", "cython_debug"], "pyre check", ".pyre/"),
    f!("dmypy", "python", SAFE, [".dmypy.json", "dmypy.json"], "dmypy run", ".dmypy.json"),
    d!("scrapy_pybuilder", "python", SAFE, [".scrapy", ".pybuilder", "pip-wheel-metadata"], "pyb", ".scrapy/"),
    Rule { id: "egg_files", cat: "python", risk: SAFE, kind: Kind::File, names: &[".installed.cfg"],
           globs: &["*.egg"], under: None, requires: N, restore: "pip install -e .", ignore: "*.egg" },

    // ================= Rust =================
    dreq!("cargo_target", "rust", SAFE, ["target"], ["Cargo.toml"], "cargo build", "target/"),

    // ================= JVM =================
    dreq!("maven_target", "jvm", SAFE, ["target"], ["pom.xml"], "mvn package", "target/"),
    dreq!("gradle_build", "jvm", SAFE, ["build"], ["build.gradle", "build.gradle.kts", "settings.gradle"],
          "gradle build", "build/"),
    d!("gradle_cache", "jvm", SAFE, [".gradle"], "gradle build", ".gradle/"),
    d!("kotlin_cache", "jvm", SAFE, [".kotlin"], "gradle build", ".kotlin/"),
    fg!("class_files", "jvm", SAFE, ["*.class"], "javac", "*.class"),

    // ================= .NET =================
    dreq!("dotnet_bin", "dotnet", SAFE, ["bin", "obj"],
          ["*.csproj", "*.sln", "*.fsproj", "*.vbproj", "*.vcxproj"], "dotnet build", "bin/"),
    dreq!("dotnet_packages", "dotnet", SAFE, ["packages"], ["packages.config"], "nuget restore", "packages/"),
    d!("vs_cache", "dotnet", SAFE, [".vs"], "Visual Studio", ".vs/"),

    // ================= Go =================
    dreq!("go_vendor", "go", CHECK, ["vendor"], ["go.mod"], "go mod vendor", "vendor/"),

    // ================= PHP =================
    dreq!("composer_vendor", "php", SAFE, ["vendor"], ["composer.json"], "composer install", "vendor/"),

    // ================= Ruby =================
    dunder!("bundle_vendor", "ruby", SAFE, ["bundle"], "vendor", "bundle install", "vendor/bundle/"),
    d!("bundle_config", "ruby", SAFE, [".bundle"], "bundle install", ".bundle/"),

    // ================= C / C++ =================
    d!("cmake_files", "native", SAFE, ["CMakeFiles"], "cmake .", "CMakeFiles/"),
    Rule { id: "cmake_build", cat: "native", risk: SAFE, kind: Kind::Dir, names: N,
           globs: &["cmake-build-*"], under: None, requires: N, restore: "cmake --build .", ignore: "cmake-build-*/" },
    f!("cmake_cache", "native", SAFE, ["CMakeCache.txt"], "cmake .", "CMakeCache.txt"),
    d!("ccls_cache", "native", SAFE, [".ccls-cache", ".clangd"], "clangd", ".ccls-cache/"),
    fg!("object_files", "native", SAFE, ["*.o", "*.obj", "*.gch", "*.pch", "*.ilk", "*.pdb"], "make", "*.o"),
    dreq!("msvc_config", "native", SAFE, ["Debug", "Release", "x64", "Win32", "ARM64", "ipch"],
          ["*.vcxproj", "*.sln"], "msbuild", "Debug/"),
    d!("autotools", "native", SAFE, [".deps", ".libs", "autom4te.cache"], "./configure", ".deps/"),
    f!("autotools_files", "native", SAFE, ["config.status", "config.log"], "./configure", "config.status"),
    d!("vcpkg", "native", SAFE, ["vcpkg_installed"], "vcpkg install", "vcpkg_installed/"),

    // ================= Apple =================
    d!("derived_data", "apple", SAFE, ["DerivedData"], "xcodebuild", "DerivedData/"),
    dreq!("swift_build", "apple", SAFE, [".build"], ["Package.swift"], "swift build", ".build/"),
    dreq!("cocoapods", "apple", SAFE, ["Pods"], ["Podfile"], "pod install", "Pods/"),
    dunder!("carthage_build", "apple", SAFE, ["Build"], "Carthage", "carthage bootstrap", "Carthage/Build/"),
    Rule { id: "xcuserdata", cat: "apple", risk: SAFE, kind: Kind::Dir, names: &["xcuserdata"],
           globs: &["*.xcuserdatad"], under: None, requires: N, restore: "Xcode", ignore: "xcuserdata/" },
    d!("swiftpm_state", "apple", SAFE, [".swiftpm"], "swift build", ".swiftpm/"),

    // ================= Mobile / Elixir =================
    d!("dart_tool", "mobile", SAFE, [".dart_tool"], "flutter pub get", ".dart_tool/"),
    f!("flutter_plugins", "mobile", SAFE, [".flutter-plugins", ".flutter-plugins-dependencies"],
       "flutter pub get", ".flutter-plugins"),
    dreq!("elixir_build", "mobile", SAFE, ["_build"], ["mix.exs"], "mix compile", "_build/"),
    dreq!("elixir_deps", "mobile", SAFE, ["deps"], ["mix.exs"], "mix deps.get", "deps/"),
    d!("android_cxx", "mobile", SAFE, [".cxx"], "gradle build", ".cxx/"),

    // ================= Jeux video =================
    dreq!("unity_library", "gamedev", SAFE, ["Library", "Temp", "Obj", "Logs", "UserSettings"],
          ["Assets", "ProjectSettings"], "Unity Editor", "Library/"),
    d!("godot_cache", "gamedev", SAFE, [".godot", ".import"], "Godot Editor", ".godot/"),
    dreq!("unreal_build", "gamedev", SAFE, ["Binaries", "Intermediate", "DerivedDataCache", "Saved"],
          ["*.uproject"], "Unreal Build Tool", "Binaries/"),
    dreq!("unity_builds", "gamedev", CHECK, ["Build", "Builds", "MemoryCaptures", "Recordings"],
          ["Assets", "ProjectSettings"], "Unity Editor", "Builds/"),

    // ================= Machine learning =================
    dunder!("hf_cache", "ml", DATA, ["huggingface"], ".cache", "huggingface-cli download", ".cache/"),
    fg!("hf_incomplete", "ml", SAFE, ["*.incomplete"], "huggingface-cli download", "*.incomplete"),
    fg!("model_weights", "ml", DATA, ["*.safetensors", "*.ckpt", "*.pth", "*.pt", "*.gguf", "*.onnx", "*.h5"],
        "huggingface-cli download", "*.safetensors"),
    d!("wandb", "ml", DATA, ["wandb", "mlruns", "lightning_logs"], "training run", "wandb/"),
    d!("tracking_runs", "ml", DATA, ["runs", "multirun", ".neptune", "catboost_info", ".aim"],
       "training run", "runs/"),

    // ================= Infrastructure =================
    d!("terraform", "infra", SAFE, [".terraform"], "terraform init", ".terraform/"),
    d!("terragrunt", "infra", SAFE, [".terragrunt-cache"], "terragrunt init", ".terragrunt-cache/"),
    d!("pulumi", "infra", SAFE, [".pulumi"], "pulumi up", ".pulumi/"),
    d!("vagrant", "infra", SAFE, [".vagrant"], "vagrant up", ".vagrant/"),

    // ================= Documentation =================
    dreq!("jekyll_site", "docs", SAFE, ["_site", ".jekyll-cache"], ["_config.yml"], "jekyll build", "_site/"),
    dreq!("hugo_public", "docs", SAFE, ["public", "resources"], ["hugo.toml", "config.toml", "hugo.yaml"],
          "hugo", "public/"),
    d!("sass_cache", "docs", SAFE, [".sass-cache"], "sass", ".sass-cache/"),
    d!("quarto", "docs", SAFE, [".quarto", "_freeze"], "quarto render", ".quarto/"),
    d!("gitbook", "docs", SAFE, ["_book", ".docz"], "gitbook build", "_book/"),

    // ================= IDE =================
    d!("idea", "ide", CHECK, [".idea"], "JetBrains IDE", ".idea/"),
    d!("vscode_history", "ide", SAFE, [".history"], "VS Code Local History", ".history/"),
    d!("direnv", "ide", SAFE, [".direnv"], "direnv allow", ".direnv/"),
    fg!("editor_swap", "ide", SAFE, ["*.swp", "*.swo", "*.swn", "*~"], "editeur", "*.sw[nop]"),

    // ================= Systeme =================
    Rule { id: "os_junk", cat: "os", risk: SAFE, kind: Kind::File,
           names: &[".DS_Store", "Thumbs.db", "ehthumbs.db", "desktop.ini"],
           globs: &["._*"], under: None, requires: N, restore: "OS", ignore: ".DS_Store" },
    d!("os_junk_dirs", "os", SAFE, [".Spotlight-V100", ".Trashes", ".fseventsd", "$RECYCLE.BIN"],
       "OS", ".Spotlight-V100/"),

    // ================= Generique, toujours en dernier =================
    d!("generic_dist", "misc", CHECK, ["dist", "out"], "build", "dist/"),
    d!("generic_build", "misc", CHECK, ["build", "_build"], "build", "build/"),
    d!("generic_cache", "misc", CHECK, [".cache", ".temp", ".tmp"], "build", ".cache/"),
    d!("generic_tmp", "misc", CHECK, ["tmp", "temp"], "runtime", "tmp/"),
    d!("generic_coverage", "misc", SAFE, ["coverage"], "test runner", "coverage/"),
    d!("log_dir", "misc", CHECK, ["logs"], "runtime", "logs/"),
    fg!("log_files", "misc", CHECK, ["*.log", "npm-debug.log*", "yarn-error.log*"], "runtime", "*.log"),
    fg!("pid_files", "misc", SAFE, ["*.pid", "*.pid.lock", "*.seed"], "runtime", "*.pid"),
    fg!("patch_leftovers", "misc", CHECK, ["*.orig", "*.rej", "*.bak", "*.rs.bk"], "outil de patch", "*.orig"),
];

/// Marqueurs qui identifient un dossier comme projet lors de la decouverte.
pub static PROJECT_MARKERS: &[&str] = &[
    ".git", "package.json", "Cargo.toml", "pyproject.toml", "setup.py",
    "requirements.txt", "go.mod", "pom.xml", "build.gradle", "build.gradle.kts",
    "composer.json", "Gemfile", "mix.exs", "Package.swift", "pubspec.yaml",
    "CMakeLists.txt", "Makefile", "*.sln", "*.csproj", "*.uproject",
    "ProjectSettings", "project.godot", "Dockerfile",
];

/// Correspondance de motif limitee a `*` et `?`, insensible a la casse.
///
/// Ce sous-ensemble couvre tous les motifs du catalogue et evite d'embarquer
/// une dependance de globbing complete.
pub fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();

    let (mut p, mut n) = (0usize, 0usize);
    let (mut star, mut backtrack) = (usize::MAX, 0usize);

    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = p;
            backtrack = n;
            p += 1;
        } else if star != usize::MAX {
            p = star + 1;
            backtrack += 1;
            n = backtrack;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

fn name_matches(rule: &Rule, name: &str) -> bool {
    if rule.names.contains(&name) {
        return true;
    }
    rule.globs.iter().any(|pattern| wildcard_match(pattern, name))
}

fn has_marker(siblings: &HashSet<String>, markers: &[&str]) -> bool {
    markers.iter().any(|marker| {
        if marker.contains('*') || marker.contains('?') {
            siblings.iter().any(|sibling| wildcard_match(marker, sibling))
        } else {
            siblings.contains(*marker)
        }
    })
}

/// Premiere regle qui correspond a l'entree, ou `None`.
pub fn find_rule(
    name: &str,
    is_dir: bool,
    parent_name: &str,
    siblings: &HashSet<String>,
) -> Option<&'static Rule> {
    let kind = if is_dir { Kind::Dir } else { Kind::File };

    RULES.iter().find(|rule| {
        rule.kind == kind
            && name_matches(rule, name)
            && rule.under.map_or(true, |under| under == parent_name)
            && (rule.requires.is_empty() || has_marker(siblings, rule.requires))
    })
}

pub fn rule_by_id(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|rule| rule.id == id)
}

/// Libelle affiche: le motif lui-meme, qui n'est pas du texte traduisible.
pub fn display_name(rule: &Rule) -> String {
    let mut parts: Vec<String> = rule
        .names
        .iter()
        .chain(rule.globs.iter())
        .map(|part| match rule.under {
            Some(under) => format!("{under}/{part}"),
            None => part.to_string(),
        })
        .collect();

    let overflow = parts.len() > 4;
    parts.truncate(4);
    let mut label = parts.join(" ");
    if overflow {
        label.push_str(" ...");
    }
    label
}
