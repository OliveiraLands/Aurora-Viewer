mod build_support;
mod git_watch;

use std::path::Path;

use git_watch::git;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_support.rs");
    println!("cargo:rerun-if-changed=git_watch.rs");
    println!("cargo:rerun-if-env-changed=AURORA_VERSION_TAG");
    println!("cargo:rerun-if-env-changed=AURORA_UPDATE_TEST_VERSION");
    println!("cargo:rerun-if-env-changed=AURORA_BRAND_CONFIG");
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo supplies CARGO_MANIFEST_DIR");
    let root = Path::new(&manifest).join("../..");
    emit_brand(&root);

    // Cargo must rebuild after a commit, a checkout or a tag of this
    // checkout, and not after the Git commands of another worktree.
    for path in git_watch::watch_list(&root) {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let tag = std::env::var("AURORA_VERSION_TAG").ok();
    let description = git(
        &root,
        &["describe", "--tags", "--match", "v[0-9]*.[0-9]*.[0-9]*", "--long", "--abbrev=9"],
    );
    let commit = git(&root, &["rev-parse", "--short=9", "HEAD"]);
    let count = git(&root, &["rev-list", "--count", "HEAD"])
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let fixture = std::env::var("AURORA_UPDATE_TEST_VERSION").ok();
    if fixture.is_some() && (std::env::var("PROFILE").as_deref() != Ok("debug") || tag.is_some()) {
        panic!("Update fixtures require a dev build without AURORA_VERSION_TAG");
    }
    // Two disposable installations exercise real version-to-version replacement
    // without creating Git tags or publishing releases. Such binaries refuse
    // normal startup; aurora-updater requires an isolated loopback fixture.
    let version = build_support::resolve(
        fixture.as_deref().or(tag.as_deref()),
        if fixture.is_some() { None } else { description.as_deref() },
        count,
        commit.as_deref(),
    )
    .unwrap_or_else(|error| panic!("Cannot determine Aurora's version: {error}"));
    println!(
        "cargo:rustc-env=AURORA_UPDATE_TEST_BUILD={}",
        if fixture.is_some() { "1" } else { "0" }
    );
    println!("cargo:rustc-env=AURORA_COMPILED_VERSION={}", version.display);
    println!("cargo:rustc-env=AURORA_LOGIN_VERSION={}", version.login);
    println!("cargo:rustc-env=AURORA_BUILD_COMMIT={}", commit.as_deref().unwrap_or("unknown"));
    // `links` metadata, read by aurora-viewer's build script as
    // DEP_AURORA_VERSION_DISPLAY / _NUMERIC (version block of the exe).
    println!("cargo:display={}", version.display);
    println!("cargo:numeric={}", version.login);
}

/// Reads optional downstream branding overrides from `AURORA_BRAND_CONFIG` or
/// `<repo>/brand.local.json` without adding build-time dependencies.
fn emit_brand(root: &Path) {
    let brand_path = std::env::var_os("AURORA_BRAND_CONFIG")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("brand.local.json"));
    println!("cargo:rerun-if-changed={}", brand_path.display());

    let text = std::fs::read_to_string(&brand_path).ok();
    let app_name = text
        .as_deref()
        .and_then(|t| json_string_field(t, "app_name"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Aurora Viewer".to_owned());
    let channel = text
        .as_deref()
        .and_then(|t| json_string_field(t, "channel"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| app_name.clone());
    let ua_product = text
        .as_deref()
        .and_then(|t| json_string_field(t, "user_agent_product"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "AuroraViewer".to_owned());

    println!("cargo:rustc-env=AURORA_BRAND_APP_NAME={app_name}");
    println!("cargo:rustc-env=AURORA_BRAND_CHANNEL={channel}");
    println!("cargo:rustc-env=AURORA_BRAND_USER_AGENT_PRODUCT={ua_product}");
}

/// Extracts a top-level string value `"key": "value"` from JSON text.
fn json_string_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let idx = json.find(&needle)?;
    let rest = &json[idx + needle.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    let mut chars = rest.chars();
    if chars.next()? != '"' {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '"' | '\\' | '/' => out.push(ch),
                _ => out.push(ch),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}
