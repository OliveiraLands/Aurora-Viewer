# OliViewer — fork task tracking

This file tracks OliViewer-specific features separately from upstream `TASKS.md` to avoid merge conflicts.

- 🟡 OpenGL (`glow`) compatibility backend for pre-Vulkan / non-bindless GPUs (task `glow-compat-backend`, 2026-11-17):
  - 🟡 `backend_choice.rs` selects between the primary native backend (`Vulkan` / `Metal`) and the compatibility-tier `OpenGl` (`glow` + `glutin`) backend via `AURORA_BACKEND=auto|vulkan|metal|gl`, falling back automatically when no compatible native adapter is available (`NoAdapters`, `IncompatibleAdapters`).
  - 🟡 `glow_backend/` (`context.rs`, `renderer.rs`, `scene.rs`) hosts an OpenGL 3.3+ Core context, `egui_glow::Painter` UI presentation, and a CPU-culled non-bindless 3D scene pipeline (sky gradient, terrain, opaque/alpha-test/blended/water passes, HUDs, and depth-buffer world picking) without requiring a `wgpu::Device`.
  - 🟡 Deliberate compatibility-tier differences from the primary `wgpu` renderer: no bindless texture arrays, compute culling, multi-draw-indirect, shadow cascades, SSR, GTAO, or OIT; rigged mesh skinning and offscreen snapshots remain follow-up work.
  - 🟡 Verified in demo mode on Intel(R) HD Graphics 4000 (OpenGL 3.3.0) with `AURORA_BACKEND=gl AURORA_DEMO=1` (`phase4-ui.png`, `phase5-scene.png`, `phase5-sky-hud.png`); `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `backend_choice` (5/5), and `aurora-assets` (133/133) pass.
  - ⬜ Pending before PR: install `cargo-about` and run `./scripts/third-party-licenses.ps1` to update `THIRD-PARTY-LICENSES.md` for `glutin` / `egui_glow`, plus human validation on legacy hardware / grid.
