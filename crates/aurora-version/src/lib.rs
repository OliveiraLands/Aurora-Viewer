//! One compiled version for the viewer, its logs and its network clients.
//! Like LLVersionInfo (indra/newview/llversioninfo.cpp), every consumer
//! reads the same build information. Aurora's release tag is authoritative.

/// Release tag without `v`, with commit distance/hash metadata between tags.
pub const fn version() -> &'static str {
    env!("AURORA_COMPILED_VERSION")
}

/// Optional downstream branded version (`brand.local.json` `app_version` or
/// `OLIVIEWER_VERSION`, with optional CI `BUILD_NUMBER`).
pub const fn brand_version() -> Option<&'static str> {
    let v = env!("AURORA_BRAND_APP_VERSION");
    if v.is_empty() { None } else { Some(v) }
}

/// Primary version shown in the UI: the downstream brand version when set,
/// otherwise the compiled Aurora version.
pub const fn display_version() -> &'static str {
    match brand_version() {
        Some(v) => v,
        None => version(),
    }
}

/// Synthetic version binaries are permitted only in disposable update fixtures.
pub const fn update_test_build() -> bool {
    env!("AURORA_UPDATE_TEST_BUILD").as_bytes()[0] == b'1'
}

/// Build revision for a diagnostic report, including builds at an exact tag.
pub const fn commit() -> &'static str {
    env!("AURORA_BUILD_COMMIT")
}

/// Viewer display name (defaults to "Aurora Viewer", overridden by `brand.local.json`).
pub const fn app_name() -> &'static str {
    env!("AURORA_BRAND_APP_NAME")
}

/// Login channel reported to the grid (defaults to "Aurora Viewer").
pub const fn channel() -> &'static str {
    env!("AURORA_BRAND_CHANNEL")
}

/// HTTP clients use the same version as the interface.
pub const fn user_agent() -> &'static str {
    concat!(env!("AURORA_BRAND_USER_AGENT_PRODUCT"), "/", env!("AURORA_COMPILED_VERSION"))
}

/// SL's login uses four numeric components, as LLVersionInfo::getVersion:
/// major.minor.patch.build. The build is the distance from the release tag.
pub const fn login_version() -> &'static str {
    env!("AURORA_LOGIN_VERSION")
}
