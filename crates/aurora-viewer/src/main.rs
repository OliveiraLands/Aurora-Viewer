//! Aurora Viewer — desktop executable entry point.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]


/// Hybrid-GPU hints (#894): on a PC with an integrated and a dedicated GPU,
/// the NVIDIA (Optimus) and AMD (PowerXpress) drivers run a program that
/// exports these on the dedicated one. The Legacy Viewer exports the same
/// (llgl.cpp). Under Vulkan the NVIDIA driver may not read them; the
/// renderer's own choice (`aurora_render::adapter_choice`) does the rest.
/// build.rs adds them to the export table (MSVC).
#[cfg(all(windows, target_env = "msvc"))]
mod gpu_hints {
    #[unsafe(no_mangle)]
    #[used]
    pub static NvOptimusEnablement: u32 = 1;
    #[unsafe(no_mangle)]
    #[used]
    pub static AmdPowerXpressRequestHighPerformance: i32 = 1;
}

fn main() -> anyhow::Result<()> {
    aurora_viewer::desktop_main()
}
