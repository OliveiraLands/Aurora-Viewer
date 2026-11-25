# Android port, responsive touch UI, and direct mobile VR

This document describes the architecture, build setup, and phased roadmap for
running OliViewer natively on Android (`aarch64-linux-android` /
`arm64-v8a`, Android 8.0+ / API 26), including touch-friendly responsive UI
for phones and tablets and direct mobile VR.

## Build and toolchain setup

1. Install the Rust Android target matching `rust-toolchain.toml`:

   ```powershell
   rustup target add aarch64-linux-android
   ```

2. Install `cargo-ndk`:

   ```powershell
   cargo install cargo-ndk --locked
   ```

3. Ensure `ANDROID_HOME` (or `ANDROID_NDK_HOME`) points to an Android SDK with
   NDK 26 or newer installed.

4. Cross-check the workspace for Android:

   ```powershell
   cargo ndk -t arm64-v8a --platform 26 check --workspace --locked
   ```

5. Build the Android `cdylib` (`libaurora_viewer.so`) into
   `packaging/android/app/src/main/jniLibs`:

   ```powershell
   cargo ndk -t arm64-v8a --platform 26 -o packaging/android/app/src/main/jniLibs build --lib -p aurora-viewer --profile ci --locked
   ```

   (`--profile ci` uses `opt-level = 0` without incremental state, avoiding
   Windows `rustc` stack/memory exhaustion on the `aurora-viewer` crate.)

6. Copy `libc++_shared.so` from the Android NDK sysroot into
   `packaging/android/app/src/main/jniLibs/arm64-v8a/` (required at runtime by
   C++ dependencies such as `meshopt`):

   ```powershell
   Copy-Item "$env:ANDROID_HOME\ndk\<version>\toolchains\llvm\prebuilt\windows-x86_64\sysroot\usr\lib\aarch64-linux-android\libc++_shared.so" "packaging\android\app\src\main\jniLibs\arm64-v8a\libc++_shared.so"
   ```

   Then open `packaging/android/` in Android Studio (or run Gradle) to build
   and install the APK.

## Phased implementation plan

### P0 — Cross-compilation and APK packaging

- **P0.1 / P0.2 (Windowing & Graphics):** Enable `winit`'s
  `android-native-activity` feature on Android and keep both `wgpu` (Vulkan)
  and `glow` (OpenGL ES) backends compilable for mobile GPUs.
- **P0.3 (Desktop-only dependency gating):**
  - Move desktop-only crates (`hidapi`, `gilrs-core`, `rfd`, `trash`, `keyring`,
    `mimalloc`, `libmimalloc-sys`) under
    `[target.'cfg(not(target_os = "android"))'.dependencies]` with
    `#[cfg(target_os = "android")]` stubs (`allocator.rs`, `joystick/backend.rs`,
    `credentials.rs`, `cache_key.rs`, `crash/github.rs`, `uninstall/system.rs`,
    `app/photo_reel.rs`, `app/graphics_start.rs`, `file_dialog.rs`, and
    `aurora-media/src/file_picker.rs`).
  - Gate POSIX shared memory (`shm_open` / `shm_unlink`) in
    `aurora-media/src/shm.rs` off on Android, where Bionic does not provide it.
  - Abstract desktop-only `winit::platform` traits (`modifier_supplement`,
    `scancode`) behind target helpers in `app.rs`.
- **P0.4 (Android entry point & APK):**
  - Expose a `cdylib` library target in `aurora-viewer` with
    `#[unsafe(no_mangle)] fn android_main(app: AndroidApp)` wired to
    `winit::platform::android::EventLoopBuilderExtAndroid`.
  - Resolve app data, cache, and log directories from `AndroidApp`
    (`internal_data_path` / `external_data_path`).
  - Add Gradle / `cargo-apk` manifest with `INTERNET`, `VIBRATE`, and OpenGL ES /
    Vulkan metadata.
- **P0.5 (Android lifecycle & logging):**
  - Handle `Resumed`, `Suspended`, `SurfaceDestroyed`, and `WindowResized`
    cleanly without panicking when the app moves to the background or rotates.
  - Route logs to Android logcat (`android_logger`).

### P1 — Touch-friendly & responsive UI (phones and tablets)

Aurora's `egui` interface must adapt cleanly across form factors without
requiring a mouse or physical keyboard:

- **Touch ergonomics:**
  - Minimum interactive hit target of **48 × 48 dp** across buttons, toggles,
    tabs, list rows, and close/action icons, with adequate spacing to avoid
    mis-taps.
  - Kinetic touch scrolling on all lists, chat logs, and inventory views;
    never rely on hover-only tooltips or right-click-only actions.
  - Long-press gestures open context menus (replacing right-click).
  - Automatic soft keyboard (IME) show/hide on text focus, shifting or
    resizing the active input area above the on-screen keyboard and respecting
    system display cutouts (notches) and navigation-bar safe insets.
- **Responsive breakpoints (Phone & Tablet):**
  - **Compact (Phone Portrait):** Full-width modal sheets/drawers instead of
    overlapping desktop floating windows; compact bottom/top navigation bar;
    collapsible HUD and chat overlay so the 3D world remains visible.
  - **Medium (Phone Landscape / Small Tablet):** Side rail or split-pane
    overlay keeping the center 3D viewport unobstructed.
  - **Expanded (Tablet):** Multi-pane layouts (e.g. inventory tree beside item
    inspector, dockable sidebars) and optional floating windows, while still
    supporting physical keyboards, mice, and Bluetooth gamepads seamlessly.
- **Touch world navigation & camera:**
  - Single tap to touch/interact with in-world objects; long-press for the
    object/avatar context menu.
  - Single-finger drag to orbit/look; two-finger pinch to zoom; two-finger drag
    to pan.
  - Optional on-screen virtual thumbsticks / movement overlay (walk, strafe,
    turn, fly/jump/crouch), auto-hidden when a physical controller or VR mode
    is active.

### P2 — Direct mobile VR (phone VR & standalone headsets)

Aurora on Android supports **direct phone VR** without a PC, reusing the
stereoscopic renderer pipeline:

- **Direct Phone VR (Cardboard / mobile VR viewers):**
  - Toggleable directly from the viewer UI / toolbar ("Mobile VR") so any
    phone placed in a VR headset holder can render side-by-side stereo directly
    on the phone screen.
  - Side-by-side dual-eye rendering (`aurora-render`) with configurable lens
    barrel distortion, chromatic aberration correction, and IPD / eye-relief
    presets.
  - **Sensor-driven 3DoF head tracking:** Use Android device sensors via NDK
    `ASensorManager` / `ASensorEventQueue` (preferring `TYPE_GAME_ROTATION_VECTOR`
    / `TYPE_ROTATION_VECTOR`, with fallback to `ASENSOR_TYPE_GYROSCOPE` +
    `ASENSOR_TYPE_ACCELEROMETER` + `ASENSOR_TYPE_MAGNETIC_FIELD` sensor fusion)
    sampled at high frequency (`SENSOR_DELAY_FASTEST`) to drive camera
    orientation in real time with low latency, landscape-axis remapping, and
    a quick yaw-recenter action. Keep the screen awake (`FLAG_KEEP_SCREEN_ON`)
    while Mobile VR is active.
  - **Hands-free / minimal input in VR:** Center gaze reticle for in-world
    touch/click (triggered by screen tap, headset capacitive button, volume
    button, or Bluetooth gamepad) plus a simple exit gesture/button back to
    standard touch mode.
- **Standalone Android OpenXR headsets (future/optional):**
  - `XR_KHR_android_create_instance` + Vulkan/GLES swapchain binding on
    standalone headsets, sharing the OpenXR architecture described in
    [Experimental VR scene viewing](VR.md).

### P3 — Platform integration and mobile performance

- Replace the `file_dialog.rs` stub with Android Storage Access Framework (SAF)
  file/photo picking via JNI.
- Replace `keyring` stubs with Android Keystore / encrypted app preferences for
  remembered login hashes and the sealed cache key.
- Add mobile graphics presets (resolution scaling, shadow quality, draw
  distance, and frame-rate/thermal caps) tuned for mobile Vulkan and OpenGL ES
  drivers.
