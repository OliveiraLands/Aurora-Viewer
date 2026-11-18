//! Compatibility-tier 3D scene renderer in OpenGL 3.3 Core (`glow`).
//!
//! Draws CPU-culled `DrawLists` without bindless textures, compute shaders,
//! or multi-draw-indirect: each draw binds its diffuse `GL_TEXTURE_2D` slot
//! and uploads per-record uniforms.

use crate::arena::{GeometryArena, RecordStore};
use crate::textures::{CompatTexture, TextureTable, WHITE};
use crate::types::{SKELETON_ANCHOR, SkinBind, flags};
use crate::{DrawCmd, DrawLists, FrameParams, SkinVertex, Vertex};
use glam::{Mat4, Vec3, Vec4};
use glow::HasContext;
use std::sync::Arc;

const SKY_VERTEX_SHADER: &str = r#"#version 330 core
out vec2 v_ndc;
void main() {
    vec2 pos = vec2(
        (gl_VertexID == 1) ? 3.0 : -1.0,
        (gl_VertexID == 2) ? 3.0 : -1.0
    );
    v_ndc = pos;
    gl_Position = vec4(pos, 0.9999, 1.0);
}
"#;

const SKY_FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 v_ndc;

uniform mat4 u_inv_vp;
uniform vec3 u_camera_pos;
uniform vec3 u_sky_zenith;
uniform vec3 u_sky_horizon;
uniform vec3 u_ground_color;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_color;

out vec4 f_color;

void main() {
    vec4 world_far = u_inv_vp * vec4(v_ndc, 1.0, 1.0);
    vec3 target = world_far.xyz / max(abs(world_far.w), 1e-5);
    vec3 dir = normalize(target - u_camera_pos);

    float up = dir.z;
    vec3 sky = (up >= 0.0)
        ? mix(u_sky_horizon, u_sky_zenith, pow(clamp(up, 0.0, 1.0), 0.55))
        : mix(u_sky_horizon, u_ground_color, pow(clamp(-up, 0.0, 1.0), 0.45));

    float sun_dot = max(dot(dir, normalize(u_sun_dir)), 0.0);
    sky += u_sun_color * pow(sun_dot, 256.0) * 1.5;

    sky = sky / (sky + vec3(1.0));
    sky = pow(max(sky, vec3(0.0)), vec3(1.0 / 2.2));
    f_color = vec4(sky, 1.0);
}
"#;

const VERTEX_SHADER: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec4 a_normal;
layout(location = 2) in vec2 a_uv;
layout(location = 3) in vec4 a_joints;
layout(location = 4) in vec4 a_weights;

uniform mat4 u_vp;
uniform mat4 u_model;
uniform vec4 u_uv_st;
uniform float u_rot;
uniform int u_is_pbr;
uniform int u_skinned;
uniform int u_palette_base;
uniform int u_skin_base;
uniform sampler2D u_palettes;
uniform sampler2D u_skin_binds;

out vec3 v_world_pos;
out vec3 v_world_normal;
out vec2 v_uv;

vec4 fetch_texel(sampler2D tex, int index) {
    return texelFetch(tex, ivec2(index & 255, index >> 8), 0);
}

mat4 fetch_palette(int mat_index) {
    int t = mat_index * 4;
    return mat4(
        fetch_texel(u_palettes, t),
        fetch_texel(u_palettes, t + 1),
        fetch_texel(u_palettes, t + 2),
        fetch_texel(u_palettes, t + 3)
    );
}

mat4 joint_matrix(int j) {
    int t = (u_skin_base + j) * 5;
    mat4 inv_bind = mat4(
        fetch_texel(u_skin_binds, t),
        fetch_texel(u_skin_binds, t + 1),
        fetch_texel(u_skin_binds, t + 2),
        fetch_texel(u_skin_binds, t + 3)
    );
    int pal_joint = int(fetch_texel(u_skin_binds, t + 4).x + 0.5);
    return fetch_palette(u_palette_base + pal_joint) * inv_bind;
}

mat4 skin_matrix(ivec4 j, vec4 w) {
    float ws = max(w.x + w.y + w.z + w.w, 1e-4);
    mat4 m = joint_matrix(j.x) * w.x;
    if (w.y > 0.0) { m += joint_matrix(j.y) * w.y; }
    if (w.z > 0.0) { m += joint_matrix(j.z) * w.z; }
    if (w.w > 0.0) { m += joint_matrix(j.w) * w.w; }
    return m * (1.0 / ws);
}

vec2 ll_xform(vec2 st, vec4 sto, float rot) {
    float s = st.x - 0.5;
    float t = st.y - 0.5;
    float ca = cos(rot);
    float sa = sin(rot);
    float tmp = s;
    s = s * ca + t * sa;
    t = -tmp * sa + t * ca;
    s = s * sto.x + sto.z + 0.5;
    t = t * sto.y + sto.w + 0.5;
    return vec2(s, t);
}

vec2 ll_uv(vec2 st, vec4 sto, float rot) {
    vec2 r = ll_xform(st, sto, rot);
    return vec2(r.x, 1.0 - r.y);
}

vec2 pbr_uv(vec2 uv, vec4 so, float r) {
    vec2 st = vec2(uv.x, 1.0 - uv.y);
    float c = cos(r);
    float sn = sin(r);
    vec2 v = st * so.xy;
    vec2 rv = vec2(c * v.x + sn * v.y, -sn * v.x + c * v.y);
    return rv + so.zw;
}

void main() {
    vec3 local_pos = a_pos;
    vec3 local_n = a_normal.xyz;
    if (u_skinned != 0) {
        mat4 skin = skin_matrix(ivec4(a_joints + 0.5), a_weights);
        local_pos = (skin * vec4(local_pos, 1.0)).xyz;
        local_n = mat3(skin) * local_n;
    }
    vec4 world = u_model * vec4(local_pos, 1.0);
    v_world_pos = world.xyz;
    vec3 wn = mat3(u_model) * local_n;
    v_world_normal = length(wn) > 1e-5 ? normalize(wn) : vec3(0.0, 0.0, 1.0);
    v_uv = (u_is_pbr != 0) ? pbr_uv(a_uv, u_uv_st, u_rot) : ll_uv(a_uv, u_uv_st, u_rot);
    vec4 clip = u_vp * world;
    // Map wgpu/Vulkan [0, 1] NDC depth to OpenGL [-1, 1] NDC depth.
    clip.z = clip.z * 2.0 - clip.w;
    gl_Position = clip;
}
"#;

const FRAGMENT_SHADER: &str = r#"#version 330 core
in vec3 v_world_pos;
in vec3 v_world_normal;
in vec2 v_uv;

uniform sampler2D u_tex;
uniform vec4 u_base_color;
uniform vec4 u_emissive;
uniform float u_alpha_cutoff;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_color;
uniform vec3 u_sky_ambient;
uniform vec3 u_ground_ambient;
uniform vec3 u_fog_color;
uniform float u_fog_density;
uniform vec3 u_camera_pos;
uniform int u_is_water;
uniform vec3 u_water_color;
uniform vec2 u_water_fresnel;

out vec4 f_color;

void main() {
    vec3 n = length(v_world_normal) > 1e-5 ? normalize(v_world_normal) : vec3(0.0, 0.0, 1.0);
    if (u_is_water != 0) {
        vec3 view_dir = normalize(u_camera_pos - v_world_pos);
        float ndotv = max(abs(dot(n, view_dir)), 0.0);
        float fresnel = clamp(u_water_fresnel.x * pow(1.0 - ndotv, 3.0) + u_water_fresnel.y, 0.3, 0.88);
        vec3 water_lin = pow(max(u_water_color, vec3(0.0)), vec3(2.2));
        vec3 half_vec = normalize(normalize(u_sun_dir) + view_dir);
        float spec = pow(max(dot(n, half_vec), 0.0), 96.0);
        vec3 wlit = mix(water_lin, u_sky_ambient * 1.4, fresnel * 0.65) + u_sun_color * spec * 0.6;
        float dist = length(v_world_pos - u_camera_pos);
        float fog = 1.0 - exp(-max(u_fog_density, 0.0) * dist);
        vec3 wcol = mix(wlit, u_fog_color, clamp(fog, 0.0, 0.95));
        wcol = wcol / (wcol + vec3(1.0));
        wcol = pow(max(wcol, vec3(0.0)), vec3(1.0 / 2.2));
        f_color = vec4(wcol, fresnel);
        return;
    }

    vec4 texel = texture(u_tex, v_uv);
    vec4 albedo = texel * u_base_color;
    if (u_alpha_cutoff > 0.0 && albedo.a < u_alpha_cutoff) {
        discard;
    }

    float ndotl = max(dot(n, normalize(u_sun_dir)), 0.0);
    float hemi = clamp(n.z * 0.5 + 0.5, 0.0, 1.0);
    vec3 ambient = mix(u_ground_ambient, u_sky_ambient, hemi);
    vec3 lit = albedo.rgb * (ambient + u_sun_color * ndotl) + u_emissive.rgb;

    float dist = length(v_world_pos - u_camera_pos);
    float fog = 1.0 - exp(-max(u_fog_density, 0.0) * dist);
    vec3 color = mix(lit, u_fog_color, clamp(fog, 0.0, 0.95));

    // Simple Reinhardt tone-map + sRGB gamma for compatibility tier.
    color = color / (color + vec3(1.0));
    color = pow(max(color, vec3(0.0)), vec3(1.0 / 2.2));
    f_color = vec4(color, albedo.a);
}
"#;

/// OpenGL 3.3 Core scene pipeline, geometry VBO/IBO, and 2D texture cache.
pub struct GlowScene {
    gl: Arc<glow::Context>,
    sky_program: glow::Program,
    sky_u_inv_vp: glow::UniformLocation,
    sky_u_camera_pos: glow::UniformLocation,
    sky_u_zenith: glow::UniformLocation,
    sky_u_horizon: glow::UniformLocation,
    sky_u_ground: glow::UniformLocation,
    sky_u_sun_dir: glow::UniformLocation,
    sky_u_sun_color: glow::UniformLocation,
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    vbo_skin: glow::Buffer,
    ibo: glow::Buffer,
    palettes_tex: glow::Texture,
    skin_binds_tex: glow::Texture,
    uploaded_vertices: usize,
    uploaded_skin: usize,
    uploaded_indices: usize,
    textures: Vec<Option<glow::Texture>>,
    u_vp: glow::UniformLocation,
    u_model: glow::UniformLocation,
    u_uv_st: glow::UniformLocation,
    u_rot: glow::UniformLocation,
    u_is_pbr: glow::UniformLocation,
    u_skinned: glow::UniformLocation,
    u_palette_base: glow::UniformLocation,
    u_skin_base: glow::UniformLocation,
    u_palettes: glow::UniformLocation,
    u_skin_binds: glow::UniformLocation,
    u_base_color: glow::UniformLocation,
    u_emissive: glow::UniformLocation,
    u_alpha_cutoff: glow::UniformLocation,
    u_sun_dir: glow::UniformLocation,
    u_sun_color: glow::UniformLocation,
    u_sky_ambient: glow::UniformLocation,
    u_ground_ambient: glow::UniformLocation,
    u_fog_color: glow::UniformLocation,
    u_fog_density: glow::UniformLocation,
    u_camera_pos: glow::UniformLocation,
    u_is_water: glow::UniformLocation,
    u_water_color: glow::UniformLocation,
    u_water_fresnel: glow::UniformLocation,
    u_tex: glow::UniformLocation,
}

impl GlowScene {
    pub fn new(gl: Arc<glow::Context>) -> Result<Self, String> {
        // SAFETY: `gl` is current on this thread.
        unsafe {
            let sky_program = compile_program(&gl, SKY_VERTEX_SHADER, SKY_FRAGMENT_SHADER)?;
            let program = match compile_program(&gl, VERTEX_SHADER, FRAGMENT_SHADER) {
                Ok(p) => p,
                Err(e) => {
                    gl.delete_program(sky_program);
                    return Err(e);
                }
            };
            let vao = gl.create_vertex_array().map_err(|e| format!("gl create_vertex_array: {e}"))?;
            let vbo = gl.create_buffer().map_err(|e| format!("gl create_buffer (vbo): {e}"))?;
            let vbo_skin = gl.create_buffer().map_err(|e| format!("gl create_buffer (vbo_skin): {e}"))?;
            let ibo = gl.create_buffer().map_err(|e| format!("gl create_buffer (ibo): {e}"))?;
            let palettes_tex = make_data_texture(&gl)?;
            let skin_binds_tex = make_data_texture(&gl)?;

            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));

            let stride = std::mem::size_of::<Vertex>() as i32;
            // pos: [f32; 3] at offset 0
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
            // normal: [i8; 4] normalized at offset 12
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 4, glow::BYTE, true, stride, 12);
            // uv: [f32; 2] at offset 16
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 16);

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_skin));
            let skin_stride = std::mem::size_of::<SkinVertex>() as i32;
            // joints: [u8; 4] at offset 0 (unnormalized -> 0.0..255.0)
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 4, glow::UNSIGNED_BYTE, false, skin_stride, 0);
            // weights: [u16; 4] at offset 4 (normalized -> 0.0..1.0)
            gl.enable_vertex_attrib_array(4);
            gl.vertex_attrib_pointer_f32(4, 4, glow::UNSIGNED_SHORT, true, skin_stride, 4);

            gl.bind_vertex_array(None);

            let sky_loc = |name: &str| {
                gl.get_uniform_location(sky_program, name)
                    .ok_or_else(|| format!("missing uniform '{name}' in glow sky shader"))
            };
            let loc = |name: &str| {
                gl.get_uniform_location(program, name)
                    .ok_or_else(|| format!("missing uniform '{name}' in glow scene shader"))
            };

            Ok(Self {
                sky_u_inv_vp: sky_loc("u_inv_vp")?,
                sky_u_camera_pos: sky_loc("u_camera_pos")?,
                sky_u_zenith: sky_loc("u_sky_zenith")?,
                sky_u_horizon: sky_loc("u_sky_horizon")?,
                sky_u_ground: sky_loc("u_ground_color")?,
                sky_u_sun_dir: sky_loc("u_sun_dir")?,
                sky_u_sun_color: sky_loc("u_sun_color")?,
                u_vp: loc("u_vp")?,
                u_model: loc("u_model")?,
                u_uv_st: loc("u_uv_st")?,
                u_rot: loc("u_rot")?,
                u_is_pbr: loc("u_is_pbr")?,
                u_skinned: loc("u_skinned")?,
                u_palette_base: loc("u_palette_base")?,
                u_skin_base: loc("u_skin_base")?,
                u_palettes: loc("u_palettes")?,
                u_skin_binds: loc("u_skin_binds")?,
                u_base_color: loc("u_base_color")?,
                u_emissive: loc("u_emissive")?,
                u_alpha_cutoff: loc("u_alpha_cutoff")?,
                u_sun_dir: loc("u_sun_dir")?,
                u_sun_color: loc("u_sun_color")?,
                u_sky_ambient: loc("u_sky_ambient")?,
                u_ground_ambient: loc("u_ground_ambient")?,
                u_fog_color: loc("u_fog_color")?,
                u_fog_density: loc("u_fog_density")?,
                u_camera_pos: loc("u_camera_pos")?,
                u_is_water: loc("u_is_water")?,
                u_water_color: loc("u_water_color")?,
                u_water_fresnel: loc("u_water_fresnel")?,
                u_tex: loc("u_tex")?,
                gl,
                sky_program,
                program,
                vao,
                vbo,
                vbo_skin,
                ibo,
                palettes_tex,
                skin_binds_tex,
                uploaded_vertices: 0,
                uploaded_skin: 0,
                uploaded_indices: 0,
                textures: Vec::new(),
            })
        }
    }

    /// Synchronizes CPU-backed geometry and textures, draws the 3D scene lists,
    /// and reads opaque-pass depth picks before water/HUDs alter the depth buffer.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        width: u32,
        height: u32,
        params: &FrameParams,
        lists: &DrawLists,
        records: &RecordStore,
        arena: &mut GeometryArena,
        textures: &mut TextureTable,
        palettes: &[[[f32; 4]; 4]],
        skin_binds: &[SkinBind],
        palettes_dirty: bool,
        skin_binds_dirty: bool,
        hover: Option<(u32, u32)>,
        picks: &[(u64, u32, u32)],
    ) -> (u32, Option<Vec3>, Vec<(u64, Option<Vec3>)>) {
        // SAFETY: `gl` is current and all handles belong to this context.
        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
        }
        self.sync_geometry(arena);
        self.sync_textures(textures);
        if palettes_dirty {
            sync_palettes(&self.gl, self.palettes_tex, palettes);
        }
        if skin_binds_dirty {
            sync_skin_binds(&self.gl, self.skin_binds_tex, skin_binds);
        }

        let vp = params.proj * params.view;
        let inv_vp = vp.inverse();
        let gl = &self.gl;
        let mut draws = 0u32;

        let mut hover_hit = None;
        let mut pick_hits = Vec::new();

        // SAFETY: `gl` is current and all handles belong to this context.
        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            gl.front_face(glow::CCW);

            // 0. Sky gradient background
            gl.disable(glow::DEPTH_TEST);
            gl.depth_mask(false);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.sky_program));
            gl.uniform_matrix_4_f32_slice(Some(&self.sky_u_inv_vp), false, &inv_vp.to_cols_array());
            gl.uniform_3_f32_slice(Some(&self.sky_u_camera_pos), &params.camera_pos.to_array());
            gl.uniform_3_f32_slice(Some(&self.sky_u_zenith), &params.sky_zenith.to_array());
            gl.uniform_3_f32_slice(Some(&self.sky_u_horizon), &params.sky_horizon.to_array());
            gl.uniform_3_f32_slice(Some(&self.sky_u_ground), &params.ground_color.to_array());
            gl.uniform_3_f32_slice(Some(&self.sky_u_sun_dir), &params.sun_dir.to_array());
            gl.uniform_3_f32_slice(Some(&self.sky_u_sun_color), &(params.sun_color * params.sun_visible).to_array());
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            gl.use_program(Some(self.program));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.palettes_tex));
            gl.uniform_1_i32(Some(&self.u_palettes), 1);
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.skin_binds_tex));
            gl.uniform_1_i32(Some(&self.u_skin_binds), 2);
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(Some(&self.u_tex), 0);

            gl.uniform_matrix_4_f32_slice(Some(&self.u_vp), false, &vp.to_cols_array());
            gl.uniform_3_f32_slice(Some(&self.u_sun_dir), &params.sun_dir.to_array());
            gl.uniform_3_f32_slice(Some(&self.u_sun_color), &(params.sun_color * params.sun_visible).to_array());
            gl.uniform_3_f32_slice(Some(&self.u_sky_ambient), &(params.sky_zenith * 0.45).to_array());
            gl.uniform_3_f32_slice(Some(&self.u_ground_ambient), &(params.ground_color * 0.35).to_array());
            gl.uniform_3_f32_slice(Some(&self.u_fog_color), &params.fog_color.to_array());
            gl.uniform_1_f32(Some(&self.u_fog_density), params.fog_density);
            gl.uniform_3_f32_slice(Some(&self.u_camera_pos), &params.camera_pos.to_array());
            gl.uniform_1_i32(Some(&self.u_is_water), 0);
            gl.uniform_3_f32_slice(Some(&self.u_water_color), &params.water.fog_color.to_array());
            gl.uniform_2_f32(Some(&self.u_water_fresnel), params.water.fresnel_scale, params.water.fresnel_offset);

            gl.enable(glow::DEPTH_TEST);
            // Aurora's projection is Reverse-Z (near = 1.0, far = 0.0).
            gl.depth_func(glow::GEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);

            // 1. Single-sided opaque geometry + terrain
            gl.enable(glow::CULL_FACE);
            gl.cull_face(glow::BACK);
            draws += self.draw_list(&lists.terrain, records, palettes, 0.0);
            draws += self.draw_list(&lists.terrain_pbr, records, palettes, 0.0);
            draws += self.draw_list(&lists.opaque, records, palettes, 0.0);
            draws += self.draw_list(&lists.mask, records, palettes, 0.5);

            // 2. Two-sided opaque & alpha-tested geometry
            gl.disable(glow::CULL_FACE);
            draws += self.draw_list(&lists.opaque_two_sided, records, palettes, 0.0);
            draws += self.draw_list(&lists.mask_two_sided, records, palettes, 0.5);

            // Read opaque depth picks before water/blend or HUD depth clears run.
            if hover.is_some() || !picks.is_empty() {
                hover_hit = hover.and_then(|(x, y)| self.pick_at(width, height, x, y, inv_vp));
                pick_hits = picks
                    .iter()
                    .map(|&(t, x, y)| (t, self.pick_at(width, height, x, y, inv_vp)))
                    .collect();
            }

            // 3. Water & alpha-blended geometry
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.depth_mask(false);
            gl.uniform_1_i32(Some(&self.u_is_water), 1);
            draws += self.draw_list(&lists.water, records, palettes, 0.0);
            gl.uniform_1_i32(Some(&self.u_is_water), 0);
            draws += self.draw_list(&lists.blend, records, palettes, 0.0);

            // 4. HUD attachments in their own depth range
            if let Some(hud_view) = lists.hud_view
                && (!lists.hud_opaque.is_empty() || !lists.hud_blend.is_empty())
            {
                gl.depth_mask(true);
                gl.clear_depth_f32(0.0);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                gl.uniform_matrix_4_f32_slice(Some(&self.u_vp), false, &hud_view.matrix().to_cols_array());
                gl.uniform_1_f32(Some(&self.u_fog_density), 0.0);
                gl.uniform_3_f32_slice(Some(&self.u_sun_color), &[0.3, 0.3, 0.3]);
                gl.uniform_3_f32_slice(Some(&self.u_sky_ambient), &[0.85, 0.85, 0.85]);
                gl.uniform_3_f32_slice(Some(&self.u_ground_ambient), &[0.85, 0.85, 0.85]);
                gl.disable(glow::BLEND);
                draws += self.draw_list(&lists.hud_opaque, records, palettes, 0.0);
                gl.enable(glow::BLEND);
                gl.depth_mask(false);
                draws += self.draw_list(&lists.hud_blend, records, palettes, 0.0);
            }

            // Restore clean GL state before `egui_glow` paints on top.
            gl.depth_mask(true);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }

        (draws, hover_hit, pick_hits)
    }

    /// Reads the world-space position under pixel `(x, y)` from the GL depth buffer.
    pub fn pick_at(&self, width: u32, height: u32, x: u32, y: u32, inv_vp: Mat4) -> Option<Vec3> {
        if x >= width || y >= height {
            return None;
        }
        let mut depth = [1.0f32; 1];
        let gl_y = (height - 1 - y) as i32;
        // SAFETY: `self.gl` is current and `depth` holds 1 f32.
        unsafe {
            self.gl.read_pixels(
                x as i32,
                gl_y,
                1,
                1,
                glow::DEPTH_COMPONENT,
                glow::FLOAT,
                glow::PixelPackData::Slice(Some(bytemuck::cast_slice_mut(&mut depth))),
            );
        }
        let d = depth[0];
        // Reverse-Z: far plane is 0.0.
        if d <= 0.0001 {
            return None;
        }
        let ndc_x = (x as f32 + 0.5) / width as f32 * 2.0 - 1.0;
        let ndc_y = 1.0 - (y as f32 + 0.5) / height as f32 * 2.0;
        let world = inv_vp * Vec4::new(ndc_x, ndc_y, d, 1.0);
        if world.w.abs() < 1e-6 {
            return None;
        }
        Some(world.truncate() / world.w)
    }

    unsafe fn draw_list(&self, cmds: &[DrawCmd], records: &RecordStore, palettes: &[[[f32; 4]; 4]], default_cutoff: f32) -> u32 {
        let mut draws = 0u32;
        let gl = &self.gl;
        let fallback_tex = self.textures.get(WHITE as usize).and_then(|t| *t);
        for cmd in cmds {
            if cmd.index_count == 0 {
                continue;
            }
            let Some(rec) = records.get(cmd.record) else {
                continue;
            };
            let skinned = (rec.flags[0] & flags::SKINNED) != 0;
            let anchored = (rec.flags[0] & flags::ANCHORED) != 0;
            let model = if skinned {
                if anchored {
                    palettes
                        .get(rec.flags[1] as usize + SKELETON_ANCHOR as usize)
                        .map_or_else(|| Mat4::from_cols_array_2d(&rec.model), Mat4::from_cols_array_2d)
                } else {
                    Mat4::from_cols_array_2d(&rec.model)
                }
            } else if anchored {
                let anchor = palettes.get(rec.flags[1] as usize).map_or(Mat4::IDENTITY, Mat4::from_cols_array_2d);
                anchor * Mat4::from_cols_array_2d(&rec.model)
            } else {
                Mat4::from_cols_array_2d(&rec.model)
            };
            let tex = self.textures.get(rec.tex[0] as usize).and_then(|t| *t).or(fallback_tex);
            // SAFETY: caller ensures program/VAO are bound and `gl` is current.
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, tex);
                gl.uniform_matrix_4_f32_slice(Some(&self.u_model), false, &model.to_cols_array());
                gl.uniform_4_f32_slice(Some(&self.u_uv_st), &rec.uv_st);
                gl.uniform_1_f32(Some(&self.u_rot), rec.params[0]);
                gl.uniform_1_i32(Some(&self.u_is_pbr), if (rec.flags[0] & flags::PBR) != 0 { 1 } else { 0 });
                gl.uniform_1_i32(Some(&self.u_skinned), if skinned { 1 } else { 0 });
                gl.uniform_1_i32(Some(&self.u_palette_base), rec.flags[1] as i32);
                gl.uniform_1_i32(Some(&self.u_skin_base), rec.flags[2] as i32);
                gl.uniform_4_f32_slice(Some(&self.u_base_color), &rec.base_color);
                gl.uniform_4_f32_slice(Some(&self.u_emissive), &rec.emissive);
                gl.uniform_1_f32(Some(&self.u_alpha_cutoff), default_cutoff);
                gl.draw_elements_base_vertex(
                    glow::TRIANGLES,
                    cmd.index_count as i32,
                    glow::UNSIGNED_SHORT,
                    (cmd.first_index as usize * 2) as i32,
                    cmd.base_vertex,
                );
            }
            draws += 1;
        }
        draws
    }

    fn sync_geometry(&mut self, arena: &mut GeometryArena) {
        let dirty = arena.take_cpu_dirty();
        let (verts, skin, indices) = arena.cpu_slices();
        let gl = &self.gl;
        if dirty || verts.len() != self.uploaded_vertices {
            // SAFETY: `gl` is current and `vbo` is valid.
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(verts), glow::DYNAMIC_DRAW);
            }
            self.uploaded_vertices = verts.len();
        }
        if dirty || skin.len() != self.uploaded_skin {
            // SAFETY: `gl` is current and `vbo_skin` is valid.
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo_skin));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(skin), glow::DYNAMIC_DRAW);
            }
            self.uploaded_skin = skin.len();
        }
        if dirty || indices.len() != self.uploaded_indices {
            // SAFETY: `gl` is current and `ibo` is valid.
            unsafe {
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ibo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytemuck::cast_slice(indices), glow::DYNAMIC_DRAW);
            }
            self.uploaded_indices = indices.len();
        }
    }

    fn sync_textures(&mut self, textures: &mut TextureTable) {
        let updates = textures.take_compat_updates();
        if updates.is_empty() {
            return;
        }
        let gl = &self.gl;
        for (slot, data) in updates {
            let idx = slot as usize;
            if idx >= self.textures.len() {
                self.textures.resize(idx + 1, None);
            }
            match data {
                Some(tex_data) => upload_gl_texture(gl, &mut self.textures[idx], &tex_data),
                None => {
                    if let Some(old) = self.textures[idx].take() {
                        // SAFETY: `gl` is current and `old` was created on this context.
                        unsafe { gl.delete_texture(old) };
                    }
                }
            }
        }
    }
}

impl Drop for GlowScene {
    fn drop(&mut self) {
        // SAFETY: `GlowRenderer` makes the context current before dropping its fields.
        unsafe {
            for tex in self.textures.drain(..).flatten() {
                self.gl.delete_texture(tex);
            }
            self.gl.delete_texture(self.palettes_tex);
            self.gl.delete_texture(self.skin_binds_tex);
            self.gl.delete_buffer(self.vbo);
            self.gl.delete_buffer(self.vbo_skin);
            self.gl.delete_buffer(self.ibo);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_program(self.program);
            self.gl.delete_program(self.sky_program);
        }
    }
}

fn make_data_texture(gl: &glow::Context) -> Result<glow::Texture, String> {
    // SAFETY: `gl` is current.
    unsafe {
        let tex = gl.create_texture().map_err(|e| format!("gl create_texture (data): {e}"))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        Ok(tex)
    }
}

fn sync_palettes(gl: &glow::Context, tex: glow::Texture, palettes: &[[[f32; 4]; 4]]) {
    let texels_needed = palettes.len() * 4;
    upload_rgba32f_table(gl, tex, texels_needed, bytemuck::cast_slice(palettes));
}

fn sync_skin_binds(gl: &glow::Context, tex: glow::Texture, binds: &[SkinBind]) {
    let texels_needed = binds.len() * 5;
    let mut packed = Vec::with_capacity(texels_needed * 4);
    for b in binds {
        let m: &[f32] = bytemuck::cast_slice(&b.inverse_bind);
        packed.extend_from_slice(m);
        packed.extend_from_slice(&[b.joint[0] as f32, 0.0, 0.0, 0.0]);
    }
    upload_rgba32f_table(gl, tex, texels_needed, &packed);
}

fn upload_rgba32f_table(gl: &glow::Context, tex: glow::Texture, texels_needed: usize, floats: &[f32]) {
    if texels_needed == 0 {
        return;
    }
    const WIDTH: usize = 256;
    let height = texels_needed.div_ceil(WIDTH).max(1);
    let total_floats = WIDTH * height * 4;
    let mut padded;
    let slice = if floats.len() == total_floats {
        floats
    } else {
        padded = vec![0.0f32; total_floats];
        let copy = floats.len().min(total_floats);
        padded[..copy].copy_from_slice(&floats[..copy]);
        &padded
    };
    // SAFETY: `gl` is current and `slice` has `WIDTH * height * 4` f32 elements.
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA32F as i32,
            WIDTH as i32,
            height as i32,
            0,
            glow::RGBA,
            glow::FLOAT,
            glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(slice))),
        );
    }
}

fn upload_gl_texture(gl: &glow::Context, target: &mut Option<glow::Texture>, data: &CompatTexture) {
    // SAFETY: `gl` is current and `data.rgba` has at least `width * height * 4` bytes.
    unsafe {
        let tex = match *target {
            Some(t) => t,
            None => match gl.create_texture() {
                Ok(t) => {
                    *target = Some(t);
                    t
                }
                Err(e) => {
                    log::warn!("gl create_texture failed: {e}");
                    return;
                }
            },
        };
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::SRGB8_ALPHA8 as i32,
            data.width as i32,
            data.height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&data.rgba)),
        );
        gl.generate_mipmap(glow::TEXTURE_2D);
    }
}

unsafe fn compile_program(gl: &glow::Context, vs_src: &str, fs_src: &str) -> Result<glow::Program, String> {
    // SAFETY: caller ensures `gl` is current.
    unsafe {
        let vs = compile_shader(gl, glow::VERTEX_SHADER, vs_src)?;
        let fs = match compile_shader(gl, glow::FRAGMENT_SHADER, fs_src) {
            Ok(s) => s,
            Err(e) => {
                gl.delete_shader(vs);
                return Err(e);
            }
        };
        let program = gl.create_program().map_err(|e| format!("gl create_program: {e}"))?;
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        let ok = gl.get_program_link_status(program);
        let log = if ok { String::new() } else { gl.get_program_info_log(program) };
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        if !ok {
            gl.delete_program(program);
            return Err(format!("OpenGL program link failed: {log}"));
        }
        Ok(program)
    }
}

unsafe fn compile_shader(gl: &glow::Context, kind: u32, src: &str) -> Result<glow::Shader, String> {
    // SAFETY: caller ensures `gl` is current.
    unsafe {
        let shader = gl.create_shader(kind).map_err(|e| format!("gl create_shader: {e}"))?;
        gl.shader_source(shader, src);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("OpenGL shader compile failed: {log}"));
        }
        Ok(shader)
    }
}
