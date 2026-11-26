//! Build script: tells the viewer which cargo profile built it, for the
//! window title ("Aurora Viewer <version> (Dev)"): cargo only exposes `debug` /
//! `release` (PROFILE), while the profile's target directory names the
//! custom ones (`debugging`, `ci`). It also reads optional local branding
//! overrides from `brand.local.json` (gitignored, or `AURORA_BRAND_CONFIG`),
//! compresses bundled spelling dictionaries, and on Windows embeds the
//! resources of aurora-viewer.exe: its icon and its version block.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // OUT_DIR = <target>/<profile dir>/build/<crate>-<hash>/out
    let out = std::env::var("OUT_DIR").unwrap_or_default();
    let parts: Vec<String> = Path::new(&out)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let dir = parts
        .iter()
        .rposition(|p| p == "build")
        .and_then(|i| i.checked_sub(1))
        .map(|i| parts[i].clone())
        .unwrap_or_default();
    println!("cargo:rustc-env=AURORA_BUILD_PROFILE={dir}");
    let brand = branding();
    dictionaries();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources(&brand);
        // The hybrid-GPU hints of main.rs (gpu_hints) in the exe's export
        // table, where the NVIDIA and AMD drivers look for them: a Rust
        // executable exports nothing on its own
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            for symbol in ["NvOptimusEnablement", "AmdPowerXpressRequestHighPerformance"] {
                println!("cargo:rustc-link-arg-bin=aurora-viewer=/EXPORT:{symbol},DATA");
            }
        }
    }
}

struct BrandBuild {
    app_name: String,
    windows_ico: PathBuf,
}

fn resolve_repo_path(root: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() { p.to_path_buf() } else { root.join(p) }
}

/// Reads optional local branding overrides (`AURORA_BRAND_CONFIG` or
/// `<repo>/brand.local.json`), copies the active logo assets into `OUT_DIR`,
/// and generates `$OUT_DIR/brand_generated.rs`.
fn branding() -> BrandBuild {
    println!("cargo:rerun-if-env-changed=AURORA_BRAND_CONFIG");
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("Cargo supplies CARGO_MANIFEST_DIR");
    let root = Path::new(&manifest).join("../..");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("Cargo supplies OUT_DIR"));

    let config_path = std::env::var_os("AURORA_BRAND_CONFIG")
        .filter(|v| !v.is_empty())
        .map(|v| resolve_repo_path(&root, &v.to_string_lossy()))
        .unwrap_or_else(|| root.join("brand.local.json"));
    println!("cargo:rerun-if-changed={}", config_path.display());

    let doc: serde_json::Value = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);

    let str_field = |key: &str, default: &str| -> String {
        doc.get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(default)
            .to_owned()
    };
    let link_field = |key: &str, default: &str| -> String {
        doc.get("links")
            .and_then(|l| l.get(key))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(default)
            .to_owned()
    };
    let icon_path = |key: &str, default_rel: &str| -> PathBuf {
        if let Some(custom) = doc
            .get("icons")
            .and_then(|i| i.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            let candidate = resolve_repo_path(&root, custom);
            if candidate.exists() {
                return candidate;
            }
        }
        root.join(default_rel)
    };

    let app_name = str_field("app_name", "Aurora Viewer");
    let modified_notice = doc
        .get("modified_notice")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let is_modified = app_name != "Aurora Viewer" || !modified_notice.is_empty();
    let third_party_notice = doc
        .get("third_party_notice")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let android_package = str_field("android_package", "org.auroraviewer.aurora");
    let desktop_id = str_field("desktop_id", "aurora-viewer");

    let github = link_field("github", "https://github.com/OliveiraLands/Aurora-Viewer");
    let new_issue = link_field("new_issue", "https://github.com/OliveiraLands/Aurora-Viewer/issues/new/choose");
    let wiki = link_field("wiki", "https://github.com/Aurora-Viewer/Aurora-Viewer/wiki");
    let discord = link_field("discord", "https://discord.gg/TK4nbMGmKt");
    let website = link_field("website", "https://oligrid.net");
    let privacy_policy = link_field("privacy_policy", "https://aurora-viewer.org/privacy/");
    let news_feed = link_field(
        "news_feed",
        "https://raw.githubusercontent.com/Aurora-Viewer/Aurora-Viewer/main/news/",
    );
    let raw_files = link_field("raw_files", "https://raw.githubusercontent.com/Aurora-Viewer/Aurora-Viewer/");

    println!("cargo:rerun-if-env-changed=AURORA_GRIDS_CONFIG");
    let grids_path = std::env::var_os("AURORA_GRIDS_CONFIG")
        .filter(|v| !v.is_empty())
        .map(|v| resolve_repo_path(&root, &v.to_string_lossy()))
        .or_else(|| {
            doc.get("grids_file")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.is_empty())
                .map(|s| resolve_repo_path(&root, s))
        })
        .unwrap_or_else(|| root.join("grids.local.json"));
    println!("cargo:rerun-if-changed={}", grids_path.display());

    let mut merged_grids: Vec<serde_json::Value> = Vec::new();
    if let Some(arr) = doc.get("default_grids").and_then(serde_json::Value::as_array) {
        merged_grids.extend(arr.iter().cloned());
    }
    if let Ok(text) = std::fs::read_to_string(&grids_path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&text)
    {
        if let Some(arr) = val.as_array() {
            merged_grids.extend(arr.iter().cloned());
        } else if let Some(arr) = val.get("grids").and_then(serde_json::Value::as_array) {
            merged_grids.extend(arr.iter().cloned());
        }
    }
    let default_grids_json = serde_json::to_string(&merged_grids).unwrap_or_else(|_| "[]".to_owned());

    let logo_png = icon_path("logo_png", "assets/branding/aurora-logo.png");
    let logo_white_svg = icon_path("logo_white_svg", "assets/branding/aurora-logo-white.svg");
    let windows_ico = icon_path("windows_ico", "assets/branding/aurora.ico");

    println!("cargo:rerun-if-changed={}", logo_png.display());
    println!("cargo:rerun-if-changed={}", logo_white_svg.display());
    std::fs::copy(&logo_png, out_dir.join("brand_logo.png"))
        .unwrap_or_else(|e| panic!("Cannot copy brand logo {}: {e}", logo_png.display()));
    std::fs::copy(&logo_white_svg, out_dir.join("brand_logo_white.svg"))
        .unwrap_or_else(|e| panic!("Cannot copy brand white SVG {}: {e}", logo_white_svg.display()));

    let generated = format!(
        "pub const APP_NAME: &str = {app_name:?};\n\
         pub const IS_MODIFIED: bool = {is_modified};\n\
         pub const MODIFIED_NOTICE: &str = {modified_notice:?};\n\
         pub const THIRD_PARTY_NOTICE: &str = {third_party_notice:?};\n\
         pub const ANDROID_PACKAGE: &str = {android_package:?};\n\
         pub const DESKTOP_ID: &str = {desktop_id:?};\n\
         pub const UPSTREAM_GITHUB: &str = \"https://github.com/Aurora-Viewer/Aurora-Viewer\";\n\
         pub const UPSTREAM_WEBSITE: &str = \"https://aurora-viewer.org\";\n\
         pub const GITHUB: &str = {github:?};\n\
         pub const NEW_ISSUE: &str = {new_issue:?};\n\
         pub const WIKI: &str = {wiki:?};\n\
         pub const DISCORD: &str = {discord:?};\n\
         pub const WEBSITE: &str = {website:?};\n\
         pub const PRIVACY_POLICY: &str = {privacy_policy:?};\n\
         pub const NEWS_FEED: &str = {news_feed:?};\n\
         pub const RAW_FILES: &str = {raw_files:?};\n\
         pub const DEFAULT_GRIDS_JSON: &str = {default_grids_json:?};\n"
    );
    std::fs::write(out_dir.join("brand_generated.rs"), generated).expect("the build folder is writable");

    BrandBuild { app_name, windows_ico }
}

/// The spelling dictionaries (assets/dictionaries/<name>/<name>.aff and
/// .dic, kept as text in the repository) gzipped into OUT_DIR, where
/// spelling/dictionaries.rs embeds them: 22 MB of words take about 5 MB of
/// the exe, inflated on the spelling thread only for the languages in use.
fn dictionaries() {
    use std::io::Write as _;
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("Cargo supplies CARGO_MANIFEST_DIR");
    let source = Path::new(&manifest).join("assets/dictionaries");
    println!("cargo:rerun-if-changed={}", source.display());
    let out = Path::new(&std::env::var("OUT_DIR").expect("Cargo supplies OUT_DIR")).join("dictionaries");
    std::fs::create_dir_all(&out).expect("the build folder is writable");
    let folders = std::fs::read_dir(&source).expect("assets/dictionaries is in the repository");
    for folder in folders.flatten().filter(|entry| entry.path().is_dir()) {
        for file in std::fs::read_dir(folder.path()).expect("a dictionary folder is readable").flatten() {
            let path = file.path();
            if !matches!(path.extension().and_then(|e| e.to_str()), Some("aff" | "dic")) {
                continue;
            }
            let bytes = std::fs::read(&path).expect("a dictionary file is readable");
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gz.write_all(&bytes).expect("gzip in memory");
            let name = format!("{}.gz", path.file_name().and_then(|n| n.to_str()).expect("an ASCII file name"));
            std::fs::write(out.join(name), gz.finish().expect("gzip in memory")).expect("the build folder is writable");
        }
    }
}

/// The window sets its icon at run time (app.rs), but the file itself needs
/// an icon resource: without one, Explorer, shortcuts and the taskbar pin
/// show the generic executable icon. The version block comes with it:
/// Properties > Details, and the name Windows gives the program (Task
/// Manager, volume mixer, firewall prompts) is its FileDescription.
///
/// The version is the one aurora-version resolved from the Git tag (its
/// `links` metadata), never CARGO_PKG_VERSION. A failure stops the build: a
/// release must not ship without its icon again.
fn windows_resources(brand: &BrandBuild) {
    let icon = &brand.windows_ico;
    println!("cargo:rerun-if-changed={}", icon.display());

    let display = std::env::var("DEP_AURORA_VERSION_DISPLAY").expect("aurora-version's build script supplies the version");
    let numeric = std::env::var("DEP_AURORA_VERSION_NUMERIC").expect("aurora-version's build script supplies the version");
    // "major.minor.patch.distance" packed as four 16-bit fields
    let packed = numeric
        .split('.')
        .chain(std::iter::repeat("0"))
        .take(4)
        .fold(0u64, |acc, part| (acc << 16) | part.parse::<u64>().unwrap_or(0).min(0xFFFF));

    let mut res = winresource::WindowsResource::new();
    res.set_icon(&icon.to_string_lossy())
        .set("ProductName", &brand.app_name)
        .set("FileDescription", &brand.app_name)
        .set("OriginalFilename", "aurora-viewer.exe")
        .set("ProductVersion", &display)
        .set("FileVersion", &display)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed)
        .set_version_info(winresource::VersionInfo::FILEVERSION, packed);
    // the viewer only: the test binaries of this crate need no resources
    if let Err(error) = res.compile_for("aurora-viewer") {
        panic!("Cannot embed the icon and version of aurora-viewer.exe (rc.exe of the Windows SDK): {error}");
    }
}
