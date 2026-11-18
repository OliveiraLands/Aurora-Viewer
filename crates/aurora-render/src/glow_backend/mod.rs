//! OpenGL (`glow`) compatibility rendering backend for pre-Vulkan / non-bindless GPUs.
//!
//! Used automatically when no Vulkan/Metal adapter meets Aurora's primary
//! requirements, or explicitly via `AURORA_BACKEND=gl`.

pub mod context;
pub mod renderer;
pub mod scene;

pub use context::{GlowContext, GlowInfo, is_compiled_in};
pub use renderer::GlowRenderer;
