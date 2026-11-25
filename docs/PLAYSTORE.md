# Publishing OliViewer to Google Play Store

This guide covers building, signing, and preparing **OliViewer** as a release Android App Bundle (`.aab`) for Google Play Store distribution.

For general Android setup and debug APK builds, see [docs/ANDROID.md](ANDROID.md).

---

## 1. Prerequisites

- Rust target installed:
  ```powershell
  rustup target add aarch64-linux-android
  ```
- `cargo-ndk` installed:
  ```powershell
  cargo install cargo-ndk --locked
  ```
- Android SDK with **NDK 26+** (`ANDROID_HOME` set).
- `libc++_shared.so` copied from the NDK sysroot into `packaging/android/app/src/main/jniLibs/arm64-v8a/libc++_shared.so`.

---

## 2. Branding & Package Identity (`brand.local.json`)

Google Play requires a unique `applicationId` and your own app icon/name (not the upstream Aurora Viewer trademarks).

Create `brand.local.json` at the repository root (gitignored; use [brand.example.json](../brand.example.json) as a template) or point `AURORA_BRAND_CONFIG` to it:

```json
{
  "app_name": "OliViewer",
  "channel": "OliViewer",
  "user_agent_product": "OliViewer",
  "android_package": "com.oliveiralands.oliviewer",
  "icons": {
    "android_res_dir": "assets/branding/android/res"
  },
  "grids_file": "grids.local.json"
}
```

[packaging/android/app/build.gradle.kts](../packaging/android/app/build.gradle.kts) reads `brand.local.json` automatically to set `applicationId`, `app_name`, and `@mipmap/ic_launcher_brand`.

---

## 3. Build the Release Native Library (`arm64-v8a`)

Google Play requires 64-bit native libraries (`arm64-v8a`). Build `libaurora_viewer.so` in `--release` mode:

```powershell
cargo ndk -t arm64-v8a --platform 26 -o packaging/android/app/src/main/jniLibs build --lib -p aurora-viewer --release --locked
```

---

## 4. Signing Configuration (Keep Secrets Outside Git)

Never commit `.jks` / `.keystore` files or passwords to the repository.

1. Generate an upload keystore (if you do not already have one, or if using Play App Signing):
   ```powershell
   keytool -genkey -v -keystore C:\secure\oliviewer-upload.jks -keyalg RSA -keysize 2048 -validity 10000 -alias oliviewer
   ```
2. Configure signing in `packaging/android/app/build.gradle.kts` via environment variables or an untracked `packaging/android/keystore.properties` file:
   - `storeFile`
   - `storePassword`
   - `keyAlias`
   - `keyPassword`

---

## 5. Bump Version & Build the `.aab`

In `packaging/android/app/build.gradle.kts`:

- Increment `versionCode` for every Play Console upload.
- Update `versionName` (for example `"0.9.14"`).
- `minSdk = 26` and `targetSdk = 35` are already configured to meet Google Play requirements.

Build the signed Android App Bundle:

```powershell
.\packaging\android\gradlew.bat -p packaging\android bundleRelease
```

The output bundle is written to:
`packaging/android/app/build/outputs/bundle/release/app-release.aab`

Test the `.aab` locally with `bundletool` before uploading to Play Console (Internal Testing track first).

---

## 6. Google Play Console Compliance Checklist

- **GPLv3 Source Code Requirement:** Include a clear link to the complete corresponding source code (`https://github.com/OliveiraLands/Aurora-Viewer`) in the Play Store listing and app website.
- **Independent Fork & Trademarks:** State clearly that **OliViewer** is an independent, modified fork based on Aurora Viewer, is not an official Aurora Viewer build, and is not affiliated with or supported by Linden Lab (Second Life®).
- **Permissions & Data Safety:**
  - `INTERNET` — Second Life / OpenSim grid login, asset fetching, and voice.
  - `RECORD_AUDIO` — Optional WebRTC voice chat (declared in Data Safety as audio collected only during active voice calls when enabled by the user).
  - `VIBRATE` — Touch haptic feedback.
- **Privacy Policy URL:** Required in Play Console (especially because `RECORD_AUDIO` is declared in `AndroidManifest.xml`).
