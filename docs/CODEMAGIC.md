# Building OliViewer with Codemagic (Android & iOS)

This repository includes [codemagic.yaml](../codemagic.yaml) with two release workflows:

- `android-release` — compiles `libaurora_viewer.so` (`arm64-v8a`) on Linux and packages a signed `.aab` + `.apk`.
- `ios-release` — compiles `libaurora_viewer.a` (`aarch64-apple-ios`) on an Apple Silicon macOS runner (`mac_mini_m2`), generates the Xcode project from [packaging/ios/project.yml](../packaging/ios/project.yml), and exports a signed `.ipa`.

---

## 1. Connect the Repository in Codemagic

1. Sign in to [codemagic.io](https://codemagic.io) and click **Add application**.
2. Select GitHub and choose `OliveiraLands/Aurora-Viewer`.
3. Choose **codemagic.yaml** as the configuration source.

---

## 2. Optional Branding Override (`brand_config` group)

Because `brand.local.json` is gitignored, you can supply it to Codemagic as a base64-encoded environment variable:

1. Encode your local `brand.local.json`:
   ```powershell
   [Convert]::ToBase64String([IO.File]::ReadAllBytes("brand.local.json")) | Set-Clipboard
   ```
2. In Codemagic, go to **App settings → Environment variables**, create a group named `brand_config`, and add:
   - `BRAND_LOCAL_JSON` = *(paste base64 string, mark Secure)*.

If `BRAND_LOCAL_JSON` is omitted, the build uses the default links and `Aurora Viewer` fallback branding.

---

## 3. Android Configuration (`android_signing: "ecostura"` & `Google-Credential`)

`codemagic.yaml` is preconfigured to reuse the existing Codemagic Android keystore and Google Play credentials:

1. **Keystore (`android_signing: ["ecostura"]`)**:
   - In Codemagic **Team / App settings → Code signing identities → Android keystores**, ensure the keystore reference named `ecostura` is available to this app.
   - Codemagic automatically injects `CM_KEYSTORE_PATH`, `CM_KEYSTORE_PASSWORD`, `CM_KEY_ALIAS`, and `CM_KEY_PASSWORD`, which are written to `packaging/android/key.properties` so Gradle signs both `.aab` and `.apk`.
2. **Google Play publishing (`Google-Credential` group)**:
   - Attach the `Google-Credential` environment variable group containing `GCP_SERVICE_ACCOUNT_KEY` (service account JSON).
   - `codemagic.yaml` publishes the signed `.aab` to the `internal` track (`versionCode` is automatically set from Codemagic's `BUILD_NUMBER`).

---

## 4. iOS Configuration (`ios_signing` + App Store Connect)

1. **Apple Developer Program**: Requires an active Apple Developer account and an App ID matching `com.oliveiralands.oliviewer` in [developer.apple.com](https://developer.apple.com).
2. **App Store Connect API Key**:
   - In App Store Connect → **Users and Access → Integrations → App Store Connect API**, generate a key with **App Manager** (or **Admin**) role.
   - Download the `.p8` file and note the **Key ID** and **Issuer ID**.
3. **Add integration in Codemagic**:
   - Go to **Teams / User settings → Integrations → App Store Connect**.
   - Name the integration `OliViewer_ASC` (matching `integrations.app_store_connect: OliViewer_ASC` in `codemagic.yaml`), and upload the `.p8` key, Key ID, and Issuer ID.
4. **Automatic code signing in Codemagic**:
   - In **App settings → Code signing identities → iOS**, either upload your Distribution Certificate + Provisioning Profile or let Codemagic fetch/create them automatically via the `OliViewer_ASC` integration (`xcode-project use-profiles`).
   - Create an empty/placeholder `ios_signing` environment variable group (or store `APP_STORE_APPLE_ID` / certificates there).

---

## 5. Triggering Builds

- Open Codemagic → **Start new build** → select `android-release` or `ios-release`.
- Download the `.aab` / `.apk` or `.ipa` from the build **Artifacts** tab.

---

## 6. Versioning Policy on Every Push

Before pushing changes meant for a new build/release, bump OliViewer's version according to the scope of the implementation:

- **Patch (`+0.0.1`)**: bug fixes, small UI tweaks, CI/docs updates.
- **Minor (`+0.1.0`)**: new features, major UI changes, or significant `upstream/main` merges.
- **Major (`+1.0.0`)**: milestone releases (e.g. `1.0.0` public store launch).
