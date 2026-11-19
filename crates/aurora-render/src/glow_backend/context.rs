//! OpenGL context and window surface management via `glutin` + `glow`.

use glow::HasContext;
use glutin::config::{Config, ConfigTemplateBuilder};
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version};
use glutin::display::{Display, DisplayApiPreference};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use std::num::NonZeroU32;
use std::sync::Arc;

/// Whether the `glow` compatibility backend is compiled for this target.
pub const fn is_compiled_in() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux", target_os = "macos"))
}

/// Basic OpenGL device/driver metadata queried after context creation.
#[derive(Debug, Clone)]
pub struct GlowInfo {
    pub renderer: String,
    pub vendor: String,
    pub version: String,
    pub max_texture_size: u32,
}

/// Owns the `glutin` OpenGL context, window surface, and `glow::Context`.
pub struct GlowContext {
    pub gl: Arc<glow::Context>,
    pub info: GlowInfo,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    _display: Display,
}

impl GlowContext {
    /// Creates an OpenGL 3.3+ Core context and window surface for `window`.
    pub fn new<W>(window: &W, width: u32, height: u32, vsync: bool) -> Result<Self, String>
    where
        W: wgpu::rwh::HasWindowHandle + wgpu::rwh::HasDisplayHandle,
    {
        let raw_window = window
            .window_handle()
            .map_err(|e| format!("failed to read window handle for OpenGL: {e}"))?
            .as_raw();
        let raw_display = window
            .display_handle()
            .map_err(|e| format!("failed to read display handle for OpenGL: {e}"))?
            .as_raw();

        let preference = display_preference(raw_window);
        // SAFETY: `raw_display` and `raw_window` are borrowed from `window` for
        // the duration of display/surface creation; `Renderer` keeps `window` alive.
        let display = unsafe { Display::new(raw_display, preference) }.map_err(|e| format!("failed to open OpenGL display: {e}"))?;

        let config = pick_config(&display, raw_window)?;

        let w = NonZeroU32::new(width.max(1)).expect("max(1) is non-zero");
        let h = NonZeroU32::new(height.max(1)).expect("max(1) is non-zero");
        let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_window, w, h);
        // SAFETY: `raw_window` is a valid live window handle.
        let surface = unsafe { display.create_window_surface(&config, &surface_attrs) }
            .map_err(|e| format!("failed to create OpenGL window surface: {e}"))?;

        let core_33 = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(Some(raw_window));
        let fallback_attrs = ContextAttributesBuilder::new().build(Some(raw_window));

        // SAFETY: `config` and `raw_window` belong to `display`.
        let not_current = unsafe { display.create_context(&config, &core_33) }
            .or_else(|_| unsafe { display.create_context(&config, &fallback_attrs) })
            .map_err(|e| format!("failed to create OpenGL context: {e}"))?;

        let context = not_current
            .make_current(&surface)
            .map_err(|e| format!("failed to make OpenGL context current: {e}"))?;

        // SAFETY: `display` is current on this thread and provides valid GL symbols.
        let gl = unsafe { glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name)) };

        // SAFETY: `gl` is current on this thread.
        let info = unsafe {
            let renderer = gl.get_parameter_string(glow::RENDERER);
            let vendor = gl.get_parameter_string(glow::VENDOR);
            let version = gl.get_parameter_string(glow::VERSION);
            let max_tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE).max(1024) as u32;
            log::info!("OpenGL probed: renderer='{renderer}', vendor='{vendor}', version='{version}', max_tex={max_tex}");
            GlowInfo {
                renderer,
                vendor,
                version,
                max_texture_size: max_tex,
            }
        };

        let ctx = Self {
            gl: Arc::new(gl),
            info,
            surface,
            context,
            _display: display,
        };
        ctx.set_vsync(vsync);
        Ok(ctx)
    }

    /// Makes this OpenGL context current on the calling thread.
    pub fn make_current(&self) -> Result<(), String> {
        self.context
            .make_current(&self.surface)
            .map_err(|e| format!("failed to make OpenGL context current: {e}"))
    }

    /// Updates the swap interval (vsync) on the window surface.
    pub fn set_vsync(&self, vsync: bool) {
        let interval = if vsync {
            SwapInterval::Wait(NonZeroU32::new(1).expect("1 is non-zero"))
        } else {
            SwapInterval::DontWait
        };
        if let Err(e) = self.surface.set_swap_interval(&self.context, interval) {
            log::debug!("OpenGL set_swap_interval({vsync}) ignored: {e}");
        }
    }

    /// Notifies the surface of a new drawable size in physical pixels.
    pub fn resize(&self, width: u32, height: u32) {
        let w = NonZeroU32::new(width.max(1)).expect("max(1) is non-zero");
        let h = NonZeroU32::new(height.max(1)).expect("max(1) is non-zero");
        self.surface.resize(&self.context, w, h);
        // SAFETY: `gl` is current on this thread.
        unsafe {
            self.gl.viewport(0, 0, width.max(1) as i32, height.max(1) as i32);
        }
    }

    /// Presents the backbuffer to the window surface.
    pub fn swap_buffers(&self) -> Result<(), String> {
        self.surface
            .swap_buffers(&self.context)
            .map_err(|e| format!("OpenGL swap_buffers failed: {e}"))
    }
}

fn display_preference(_raw_window: wgpu::rwh::RawWindowHandle) -> DisplayApiPreference {
    #[cfg(target_os = "windows")]
    {
        DisplayApiPreference::Wgl(Some(_raw_window))
    }
    #[cfg(target_os = "macos")]
    {
        DisplayApiPreference::Cgl
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        DisplayApiPreference::Egl
    }
}

fn pick_config(display: &Display, raw_window: wgpu::rwh::RawWindowHandle) -> Result<Config, String> {
    #[cfg(target_os = "windows")]
    let builder = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_depth_size(24)
        .compatible_with_native_window(raw_window);
    #[cfg(not(target_os = "windows"))]
    let builder = {
        let _ = raw_window;
        ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(24)
    };

    let template = builder.build();
    // SAFETY: `template` is valid for `display`.
    let configs = unsafe { display.find_configs(template) }.map_err(|e| format!("failed to enumerate OpenGL pixel formats: {e}"))?;

    configs
        .reduce(|best, cfg| {
            // Prefer hardware-accelerated pixel formats over GDI Generic (OpenGL 1.1),
            // and 0-sample configs for the default framebuffer.
            let cfg_hw = cfg.hardware_accelerated();
            let best_hw = best.hardware_accelerated();
            if cfg_hw != best_hw {
                return if cfg_hw { cfg } else { best };
            }
            if cfg.num_samples() < best.num_samples() { cfg } else { best }
        })
        .ok_or_else(|| "no suitable OpenGL pixel format found".to_owned())
}
