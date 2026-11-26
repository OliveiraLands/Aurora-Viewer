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
- `minSdk = 26` and `targetSdk = 36` are already configured to meet Google Play requirements.

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
- **Privacy Policy URL:** Required in Play Console (especially because `RECORD_AUDIO` is declared in `AndroidManifest.xml`). Use `https://oligrid.net/privacy-oliviewer`.

---

## 7. Play Console → App Content Forms (Truthful & Review-Safe Guide)

In Google Play Console, go to **Policy → App content** and complete each declaration truthfully as follows to avoid rejections for inaccurate disclosures or unmoderated user-generated content:

### 7.1. Privacy Policy
- **URL:** `https://oligrid.net/privacy-oliviewer`
- Must remain publicly accessible over HTTPS and match `com.oliveiralands.oliviewer`.

### 7.2. App Access
- Select **"All or some functionality in my app is restricted"** (signing in to a Second Life or OpenSimulator grid is required to enter 3D regions).
- Add **working test instructions and credentials** for a dedicated test avatar account (for example, on `OliGrid`: provide username, password, and note to select `OliGrid` on the login screen) so Google reviewers can log in and inspect the 3D viewer without failing review.

### 7.3. Ads
- Select **"No, my app does not contain ads"** (OliViewer includes no AdMob or advertising SDKs).

### 7.4. Content Rating (IARC Questionnaire)
- **Category:** Social Networking / User-Generated Content (or Utility/Communication).
- Answer **Yes** to:
  - Users can interact or exchange content with each other (text chat, IM, optional voice chat).
  - The app gives access to user-generated content / external servers not moderated by the app itself (3D worlds, textures, and chat come from user-selected Second Life / OpenSimulator grids).
- Answer **No** to: real-money gambling, app-bundled violence/gore.
- **Expected rating:** **16+ / 18+ (Teen / Mature)**. Never aim for "Everyone", because external virtual world grids can host Moderate or Adult regions.

### 7.5. Target Audience and Content
- **Target age groups:** Select **18 and over** (or **16–17 and 18+**). **Never** select ages under 13.
- **Appeal to children:** **No**.

### 7.6. News Apps / Government Apps / Financial Features / Health
- Select **No** for all of these.
- OliViewer does not provide banking, crypto trading, or real-money financial services (any in-world virtual credits belong to the external grid servers).

### 7.7. Data Safety
- **Does your app collect or share any of the required user data types?** → **Yes** (login credentials, in-world chat, and optional WebRTC voice audio are transmitted to the virtual world grid chosen by the user).
- **Is all user data encrypted in transit?** → **Yes** (login and grid capabilities use HTTPS/TLS; the viewer warns if a user manually enters a plain `http://` OpenSim URI).
- **Data deletion:** Explain that local settings, cache, and saved logins are deleted by clearing app data or uninstalling, while grid account deletion is handled by the chosen grid operator (for OliGrid accounts, via `https://oligrid.net`).
- **Data types to declare (Purpose: App functionality; Not sold; Not used for ads/analytics):**
  - **Personal info → User IDs / Account info:** Avatar login credentials and grid selection sent to the user's chosen grid server.
  - **Audio → Voice or sound recordings:** Declared because `RECORD_AUDIO` is in `AndroidManifest.xml`; used only in real time when the user enables WebRTC voice chat (never recorded or stored by OliveiraLands).
  - **Messages → Other in-app messages:** Text chat and IMs exchanged with the connected grid.

### 7.8. Advertising ID (`AD_ID`)
- Select **No** — OliViewer does not declare `com.google.android.gms.permission.AD_ID` and does not use an Advertising ID.

---

## 8. OliViewer Versioning Rule (Every Push / Release)

Bump the OliViewer version (`app_version` in `brand.local.json` / `OLIVIEWER_VERSION`, plus `MARKETING_VERSION` in `packaging/ios/project.yml` when releasing iOS) on every push/release according to the size of the changes:

- **Patch (`0.1.X` → `0.1.X+1`)**: bug fixes, small UI/touch adjustments, CI/packaging tweaks, or documentation updates.
- **Minor (`0.X.0` → `0.X+1.0`)**: new user-facing features, new platform support, or significant `upstream/main` feature merges.
- **Major (`X.0.0` → `X+1.0.0`)**: major milestones (such as `1.0.0` for initial public store launch) or large architectural changes.

In Codemagic, `BUILD_NUMBER` is automatically appended (`versionCode` on Android and `(build N)` in OliViewer's About dialog), while the About window continues to reference the underlying upstream Aurora Viewer version.
