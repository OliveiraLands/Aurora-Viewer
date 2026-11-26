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

## 3. Android Configuration (`android_signing` group)

1. Generate or locate your upload keystore (see [docs/PLAYSTORE.md](PLAYSTORE.md)).
2. In Codemagic **App settings → Code signing identities → Android keystores**, upload your `.jks` / `.keystore` file and note its reference name, or store its values in the `android_signing` environment variable group:
   - `CM_KEYSTORE_PATH` (automatically populated if uploaded via Codemagic Code Signing, or decoded in a custom step)
   - `CM_KEYSTORE_PASSWORD` (Secure)
   - `CM_KEY_PASSWORD` (Secure)
   - `CM_KEY_ALIAS` (e.g. `oliviewer-upload`)
3. Optionally, add a `publishing` block under `android-release` in `codemagic.yaml` with your Google Play service account JSON (`GCLOUD_SERVICE_ACCOUNT_CREDENTIALS`) to upload `.aab` directly to the `internal` track.

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
