//! Graphics backend selection and hardware fallback policy.
//!
//! Aurora's primary renderer targets Vulkan (Metal on macOS) via `wgpu` and
//! requires modern GPU features (bindless texture arrays, indirect first
//! instance, and fragment storage buffers). Pre-Vulkan GPUs such as Intel HD
//! 4000 (Ivy Bridge) and older families (HD 3000 / Sandy Bridge) lack Vulkan
//! on Windows or lack descriptor indexing / compute shaders, and therefore
//! fall back to a compatibility OpenGL (`glow`) backend when available.

/// Environment variable overriding automatic backend selection.
pub const ENV_VAR: &str = "AURORA_BACKEND";

/// Backend requested by the user or environment (`AURORA_BACKEND`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsBackend {
    /// Try the platform's native backend (Vulkan / Metal) first and fall back
    /// to OpenGL (`glow`) if no compatible native device can be opened.
    #[default]
    Auto,
    /// Require Vulkan (`wgpu`).
    Vulkan,
    /// Require Metal (`wgpu`, macOS).
    Metal,
    /// Use the OpenGL compatibility backend (`glow`).
    OpenGl,
}

impl GraphicsBackend {
    /// Parses `AURORA_BACKEND` values (`auto`, `vulkan`, `vk`, `metal`, `gl`, `opengl`, `glow`).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Some(Self::Auto),
            "vulkan" | "vk" => Some(Self::Vulkan),
            "metal" | "mtl" => Some(Self::Metal),
            "gl" | "opengl" | "glow" => Some(Self::OpenGl),
            _ => None,
        }
    }

    /// Reads the backend preference from `AURORA_BACKEND`, defaulting to `Auto`.
    pub fn from_env() -> Self {
        let Ok(raw) = std::env::var(ENV_VAR) else {
            return Self::Auto;
        };
        match Self::parse(&raw) {
            Some(backend) => backend,
            None => {
                log::warn!("{ENV_VAR}={raw}: unknown backend (expected auto, vulkan, metal, gl); using auto");
                Self::Auto
            }
        }
    }
}

/// Active rendering backend selected for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveBackend {
    Vulkan,
    Metal,
    OpenGl,
}

impl ActiveBackend {
    /// Platform's primary native backend.
    pub const fn platform_native() -> Self {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            Self::Metal
        } else {
            Self::Vulkan
        }
    }

    /// Maps a `wgpu::Backend` to `ActiveBackend`, if it is one of Aurora's supported backends.
    pub fn from_wgpu(backend: wgpu::Backend) -> Option<Self> {
        match backend {
            wgpu::Backend::Vulkan => Some(Self::Vulkan),
            wgpu::Backend::Metal => Some(Self::Metal),
            wgpu::Backend::Gl => Some(Self::OpenGl),
            _ => None,
        }
    }

    /// Human-readable backend name for logs, UI, and crash reports.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Vulkan => "Vulkan",
            Self::Metal => "Metal",
            Self::OpenGl => "OpenGL (glow compatibility)",
        }
    }

    /// Whether this backend runs in the reduced-feature compatibility tier
    /// (without bindless textures, compute passes, or GPU culling).
    pub const fn is_compatibility_tier(self) -> bool {
        matches!(self, Self::OpenGl)
    }
}

/// Outcome of probing the platform's primary native backend (`wgpu` on Vulkan/Metal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeProbeOutcome {
    /// At least one native adapter opened with all required features.
    Ready,
    /// No native adapters were enumerated (e.g. Intel HD 4000 on Windows).
    NoAdapters,
    /// Native adapters were found, but all failed required feature/device checks.
    IncompatibleAdapters,
}

/// Why the compatibility backend was selected instead of the primary native one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    /// Primary native backend succeeded or OpenGL was explicitly requested.
    None,
    /// No Vulkan/Metal adapter was available on the system.
    NoNativeAdapters,
    /// Native adapters lacked required features (bindless, indirect first instance, storage buffers).
    IncompatibleNativeHardware,
}

/// Final backend selection decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendDecision {
    pub selected: ActiveBackend,
    pub fallback: FallbackReason,
}

/// Chooses the active backend given the user preference, native probe outcome,
/// and whether the OpenGL (`glow`) compatibility backend is available.
pub fn decide(
    wanted: GraphicsBackend,
    platform_native: ActiveBackend,
    native: NativeProbeOutcome,
    gl_available: bool,
) -> Result<BackendDecision, &'static str> {
    match wanted {
        GraphicsBackend::OpenGl => {
            if gl_available {
                Ok(BackendDecision {
                    selected: ActiveBackend::OpenGl,
                    fallback: FallbackReason::None,
                })
            } else {
                Err("OpenGL (glow) backend requested via AURORA_BACKEND, but OpenGL is unavailable")
            }
        }
        GraphicsBackend::Vulkan => {
            if platform_native != ActiveBackend::Vulkan {
                return Err("Vulkan backend requested, but it is not the native backend on this platform");
            }
            match native {
                NativeProbeOutcome::Ready => Ok(BackendDecision {
                    selected: ActiveBackend::Vulkan,
                    fallback: FallbackReason::None,
                }),
                NativeProbeOutcome::NoAdapters => Err("Vulkan backend requested, but no Vulkan adapters were found"),
                NativeProbeOutcome::IncompatibleAdapters => Err("Vulkan backend requested, but no adapter meets the required features"),
            }
        }
        GraphicsBackend::Metal => {
            if platform_native != ActiveBackend::Metal {
                return Err("Metal backend requested, but it is only available on macOS");
            }
            match native {
                NativeProbeOutcome::Ready => Ok(BackendDecision {
                    selected: ActiveBackend::Metal,
                    fallback: FallbackReason::None,
                }),
                NativeProbeOutcome::NoAdapters => Err("Metal backend requested, but no Metal adapters were found"),
                NativeProbeOutcome::IncompatibleAdapters => Err("Metal backend requested, but no adapter meets the required features"),
            }
        }
        GraphicsBackend::Auto => match native {
            NativeProbeOutcome::Ready => Ok(BackendDecision {
                selected: platform_native,
                fallback: FallbackReason::None,
            }),
            NativeProbeOutcome::NoAdapters => {
                if gl_available {
                    Ok(BackendDecision {
                        selected: ActiveBackend::OpenGl,
                        fallback: FallbackReason::NoNativeAdapters,
                    })
                } else {
                    Err("no native GPU adapters found and OpenGL fallback is unavailable")
                }
            }
            NativeProbeOutcome::IncompatibleAdapters => {
                if gl_available {
                    Ok(BackendDecision {
                        selected: ActiveBackend::OpenGl,
                        fallback: FallbackReason::IncompatibleNativeHardware,
                    })
                } else {
                    Err("no compatible native GPU adapters found and OpenGL fallback is unavailable")
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backend_names() {
        assert_eq!(GraphicsBackend::parse(""), Some(GraphicsBackend::Auto));
        assert_eq!(GraphicsBackend::parse(" AUTO "), Some(GraphicsBackend::Auto));
        assert_eq!(GraphicsBackend::parse("vulkan"), Some(GraphicsBackend::Vulkan));
        assert_eq!(GraphicsBackend::parse("vk"), Some(GraphicsBackend::Vulkan));
        assert_eq!(GraphicsBackend::parse("metal"), Some(GraphicsBackend::Metal));
        assert_eq!(GraphicsBackend::parse("gl"), Some(GraphicsBackend::OpenGl));
        assert_eq!(GraphicsBackend::parse("OpenGL"), Some(GraphicsBackend::OpenGl));
        assert_eq!(GraphicsBackend::parse("glow"), Some(GraphicsBackend::OpenGl));
        assert_eq!(GraphicsBackend::parse("dx11"), None);
    }

    #[test]
    fn auto_prefers_native_when_ready() {
        let d = decide(GraphicsBackend::Auto, ActiveBackend::Vulkan, NativeProbeOutcome::Ready, true).unwrap();
        assert_eq!(d.selected, ActiveBackend::Vulkan);
        assert_eq!(d.fallback, FallbackReason::None);
        assert!(!d.selected.is_compatibility_tier());
    }

    #[test]
    fn auto_falls_back_to_opengl_on_legacy_hardware() {
        let no_vk = decide(GraphicsBackend::Auto, ActiveBackend::Vulkan, NativeProbeOutcome::NoAdapters, true).unwrap();
        assert_eq!(no_vk.selected, ActiveBackend::OpenGl);
        assert_eq!(no_vk.fallback, FallbackReason::NoNativeAdapters);
        assert!(no_vk.selected.is_compatibility_tier());

        let old_vk = decide(
            GraphicsBackend::Auto,
            ActiveBackend::Vulkan,
            NativeProbeOutcome::IncompatibleAdapters,
            true,
        )
        .unwrap();
        assert_eq!(old_vk.selected, ActiveBackend::OpenGl);
        assert_eq!(old_vk.fallback, FallbackReason::IncompatibleNativeHardware);
    }

    #[test]
    fn explicit_vulkan_does_not_fall_back_to_opengl() {
        let err = decide(GraphicsBackend::Vulkan, ActiveBackend::Vulkan, NativeProbeOutcome::NoAdapters, true).unwrap_err();
        assert!(err.contains("no Vulkan adapters"));
    }

    #[test]
    fn explicit_opengl_selects_compatibility_tier() {
        let d = decide(GraphicsBackend::OpenGl, ActiveBackend::Vulkan, NativeProbeOutcome::Ready, true).unwrap();
        assert_eq!(d.selected, ActiveBackend::OpenGl);
        assert_eq!(d.fallback, FallbackReason::None);
    }
}
