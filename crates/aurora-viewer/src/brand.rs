//! Downstream branding configuration loaded at build time from
//! `brand.local.json` (gitignored, or `AURORA_BRAND_CONFIG`).
//!
//! When no local brand file is present, every constant falls back to the
//! upstream Aurora Viewer defaults. Modified builds must keep Aurora Viewer's
//! GPLv3 copyright and licence notices (GPLv3 §§4–5, `TRADEMARKS.md`) and use
//! their own app name and icon.

include!(concat!(env!("OUT_DIR"), "/brand_generated.rs"));

/// Main raster brand icon (window icon, login/loading screens, About, notifications).
pub const LOGO_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/brand_logo.png"));

/// Monochrome white vector brand silhouette (top-left viewer menu button).
pub const LOGO_WHITE_SVG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/brand_logo_white.svg"));

/// Pre-configured grids bundled via `grids.local.json` / `brand.local.json`,
/// plus any external `grids.json` placed in the viewer's data directory or
/// beside the executable at runtime.
pub fn default_grids() -> Vec<crate::grids::Grid> {
    let mut all = parse_default_grids(DEFAULT_GRIDS_JSON);
    for path in runtime_grids_paths() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            seed_grids_from(&mut all, &parse_default_grids(&text));
        }
    }
    all
}

fn runtime_grids_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        paths.push(dir.join("grids.json"));
    }
    if let Some(dir) = crate::settings::Settings::path().parent() {
        paths.push(dir.join("grids.json"));
    }
    paths
}

fn parse_default_grids(json: &str) -> Vec<crate::grids::Grid> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(error) => {
            log::warn!("brand default_grids ignored: {error}");
            return Vec::new();
        }
    };
    let array = if let Some(arr) = value.as_array() {
        arr
    } else if let Some(arr) = value.get("grids").and_then(serde_json::Value::as_array) {
        arr
    } else {
        return Vec::new();
    };
    let mut valid = Vec::with_capacity(array.len());
    for entry in array {
        match serde_json::from_value::<crate::grids::Grid>(entry.clone()) {
            Ok(mut grid) if grid.validate().is_ok() => valid.push(grid),
            Ok(grid) => log::warn!("brand default_grids entry '{}' failed validation", grid.name),
            Err(error) => log::warn!("brand default_grids entry skipped: {error}"),
        }
    }
    valid
}

/// Ensures every grid configured in `brand.local.json` exists in `grids`
/// without duplicating or overwriting user-edited entries.
pub fn seed_default_grids(grids: &mut Vec<crate::grids::Grid>) {
    seed_grids_from(grids, &default_grids());
}

fn seed_grids_from(grids: &mut Vec<crate::grids::Grid>, defaults: &[crate::grids::Grid]) {
    let mut added = false;
    for default in defaults {
        let default_login = crate::grids::service(&default.login).ok();
        let exists = grids.iter().any(|existing| {
            existing.service == default.service || (default_login.is_some() && crate::grids::service(&existing.login).ok() == default_login)
        });
        if !exists {
            grids.push(default.clone());
            added = true;
        }
    }
    if added {
        grids.sort_by_key(|grid| crate::natural_order::key(&grid.name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_adds_missing_default_grid_without_duplicating_or_overwriting() {
        let defaults = parse_default_grids(
            r#"[{"name":"OliGrid","nickname":"oligrid","service":"http://hg.oligrid.net:8002","login":"http://hg.oligrid.net:8002/","gatekeeper":"http://hg.oligrid.net:8002"}]"#,
        );
        assert_eq!(defaults.len(), 1);

        let mut grids = Vec::new();
        seed_grids_from(&mut grids, &defaults);
        assert_eq!(grids.len(), 1);
        assert_eq!(grids[0].name, "OliGrid");
        assert_eq!(grids[0].login, "http://hg.oligrid.net:8002/");

        // User renames the grid: seeding again must keep the user's edit and not duplicate it.
        grids[0].name = "My OliGrid".to_owned();
        seed_grids_from(&mut grids, &defaults);
        assert_eq!(grids.len(), 1);
        assert_eq!(grids[0].name, "My OliGrid");
    }
}
