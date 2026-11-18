//! Compatibility-tier OpenGL (`glow`) renderer host and `egui` UI presenter.

use super::context::GlowContext;
use super::scene::GlowScene;
use crate::arena::{GeometryArena, RecordStore};
use crate::textures::TextureTable;
use crate::types::SkinBind;
use crate::{DrawLists, EguiFrame, FrameParams, RenderStats};
use glam::Vec3;
use glow::HasContext;
use std::any::Any;
use std::sync::Arc;

/// OpenGL compatibility renderer owned by `aurora_render::Renderer`.
pub struct GlowRenderer {
    context: GlowContext,
    scene: GlowScene,
    painter: egui_glow::Painter,
    size: (u32, u32),
    vsync: bool,
    palettes: Vec<[[f32; 4]; 4]>,
    skin_binds: Vec<SkinBind>,
    palettes_dirty: bool,
    skin_binds_dirty: bool,
    /// Keeps the native window alive until after `context` and `painter` drop.
    _window: Arc<dyn Any + Send + Sync>,
}

impl GlowRenderer {
    /// Initializes the OpenGL context, compatibility 3D scene pipeline, and `egui_glow` painter for `window`.
    pub fn new<W>(window: Arc<W>, width: u32, height: u32, vsync: bool) -> Result<Self, String>
    where
        W: wgpu::rwh::HasWindowHandle + wgpu::rwh::HasDisplayHandle + Send + Sync + 'static,
    {
        let context = GlowContext::new(&*window, width, height, vsync)?;
        let scene = GlowScene::new(Arc::clone(&context.gl))?;
        let painter = egui_glow::Painter::new(Arc::clone(&context.gl), "", None, true)
            .map_err(|e| format!("failed to initialize egui_glow painter: {e}"))?;
        let keep_alive: Arc<dyn Any + Send + Sync> = window;
        Ok(Self {
            context,
            scene,
            painter,
            size: (width.max(1), height.max(1)),
            vsync,
            palettes: Vec::new(),
            skin_binds: Vec::new(),
            palettes_dirty: false,
            skin_binds_dirty: false,
            _window: keep_alive,
        })
    }

    pub fn set_skin_binds(&mut self, binds: &[SkinBind]) {
        self.skin_binds.clear();
        self.skin_binds.extend_from_slice(binds);
        self.skin_binds_dirty = true;
    }

    pub fn set_palettes(&mut self, mats: &[[[f32; 4]; 4]]) {
        self.palettes.clear();
        self.palettes.extend_from_slice(mats);
        self.palettes_dirty = true;
    }

    pub fn set_palette_range(&mut self, mats: &[[[f32; 4]; 4]], range: std::ops::Range<usize>) {
        if mats.len() > self.palettes.len() {
            self.set_palettes(mats);
            return;
        }
        let a = range.start.min(mats.len());
        let b = range.end.min(mats.len());
        if a < b {
            self.palettes[a..b].copy_from_slice(&mats[a..b]);
            self.palettes_dirty = true;
        }
    }

    pub fn info(&self) -> &super::context::GlowInfo {
        &self.context.info
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        if self.size == (w, h) {
            return;
        }
        self.size = (w, h);
        if let Err(e) = self.context.make_current() {
            log::warn!("OpenGL make_current on resize failed: {e}");
            return;
        }
        self.context.resize(w, h);
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        if self.vsync == vsync {
            return;
        }
        self.vsync = vsync;
        if let Err(e) = self.context.make_current() {
            log::warn!("OpenGL make_current on set_vsync failed: {e}");
            return;
        }
        self.context.set_vsync(vsync);
    }

    /// Clears the frame, draws the 3D scene, reads depth picks, paints `egui` UI,
    /// optionally reads back RGBA8 pixels, and swaps buffers.
    #[allow(clippy::too_many_arguments)]
    pub fn render_frame(
        &mut self,
        params: &FrameParams,
        lists: &DrawLists,
        records: &RecordStore,
        geometry: &mut GeometryArena,
        textures: &mut TextureTable,
        hover: Option<(u32, u32)>,
        picks: &[(u64, u32, u32)],
        ui: Option<EguiFrame>,
        capture: bool,
    ) -> (RenderStats, Option<Vec3>, Vec<(u64, Option<Vec3>)>, Option<(u32, u32, Vec<u8>)>) {
        if let Err(e) = self.context.make_current() {
            log::error!("OpenGL make_current failed: {e}");
            let failed_picks = picks.iter().map(|&(t, _, _)| (t, None)).collect();
            return (RenderStats::default(), None, failed_picks, None);
        }

        let (w, h) = self.size;
        let gl = &self.context.gl;
        let clear = params.clear_color.to_array();
        // SAFETY: `context` is current on this thread.
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.viewport(0, 0, w as i32, h as i32);
            gl.clear_color(clear[0], clear[1], clear[2], clear[3]);
            // Aurora uses Reverse-Z (near = 1.0, far = 0.0).
            gl.clear_depth_f32(0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }

        let palettes_dirty = std::mem::take(&mut self.palettes_dirty);
        let skin_binds_dirty = std::mem::take(&mut self.skin_binds_dirty);
        let (draws, hover_hit, pick_hits) = self.scene.draw(
            w,
            h,
            params,
            lists,
            records,
            geometry,
            textures,
            &self.palettes,
            &self.skin_binds,
            palettes_dirty,
            skin_binds_dirty,
            hover,
            picks,
        );
        let mut stats = RenderStats {
            draws,
            ..Default::default()
        };

        if let Some(mut frame) = ui {
            let mut delta = std::mem::take(&mut frame.textures_delta);
            let mut ui_frame = frame.tessellate();
            // Drop wgpu-only paint callbacks (e.g. UI glass/blur) so egui_glow does not spam warnings.
            ui_frame
                .primitives
                .retain(|p| !matches!(&p.primitive, egui::epaint::Primitive::Callback(cb) if !cb.callback.is::<egui_glow::CallbackFn>()));
            stats.ui_shapes = ui_frame.shapes as u32;
            stats.ui_vertices = ui_frame.vertices as u32;
            self.painter
                .paint_and_update_textures([w, h], ui_frame.pixels_per_point, &ui_frame.primitives, &mut delta);
        }

        let captured = capture.then(|| readback_rgba(gl, w, h));

        match self.context.swap_buffers() {
            Ok(()) => stats.presented = true,
            Err(e) => log::error!("{e}"),
        }

        (stats, hover_hit, pick_hits, captured)
    }

    /// Reads the last rendered depth buffer at `(x, y)` using `inv_vp`.
    pub fn pick_world(&self, x: u32, y: u32, inv_vp: glam::Mat4) -> Option<Vec3> {
        if self.context.make_current().is_err() {
            return None;
        }
        self.scene.pick_at(self.size.0, self.size.1, x, y, inv_vp)
    }
}

impl Drop for GlowRenderer {
    fn drop(&mut self) {
        if self.context.make_current().is_ok() {
            self.painter.destroy();
        }
    }
}

fn readback_rgba(gl: &glow::Context, width: u32, height: u32) -> (u32, u32, Vec<u8>) {
    let row_bytes = width as usize * 4;
    let mut pixels = vec![0u8; row_bytes * height as usize];
    // SAFETY: `gl` is current and `pixels` has exact `width * height * 4` capacity.
    unsafe {
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(
            0,
            0,
            width as i32,
            height as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
    }
    // OpenGL's origin is bottom-left; flip rows to match Aurora's top-left RGBA8 captures.
    for y in 0..(height as usize / 2) {
        let top = y * row_bytes;
        let bot = (height as usize - 1 - y) * row_bytes;
        let (head, tail) = pixels.split_at_mut(bot);
        head[top..top + row_bytes].swap_with_slice(&mut tail[..row_bytes]);
    }
    (width, height, pixels)
}
