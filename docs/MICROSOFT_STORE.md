# Publishing OliViewer to the Microsoft Store

Windows desktop builds (`aurora-viewer.exe`) can be distributed on the **Microsoft Store** either as an **MSIX package** (recommended for automatic updates and clean install/uninstall) or as a **traditional Win32 installer** (`.exe`/`.msi`) hosted on your own CDN.

---

## 1. Prerequisites

- **Microsoft Partner Center** developer account ([partner.microsoft.com](https://partner.microsoft.com)) with Windows app publishing enabled.
- **Windows 10/11 SDK** installed (provides `makeappx.exe`, `signtool.exe`, and **Windows App Certification Kit**).
- Release build of `aurora-viewer.exe`:
  ```powershell
  ./scripts/build-release.ps1
  ```

---

## 2. Reserve Your App Name & Identity in Partner Center

1. In Partner Center → **Apps and games → New product → MSIX or PWA app**.
2. Reserve your app name (e.g. `OliViewer`).
3. Open **Product management → Product identity** and record:
   - **Package/Identity/Name** (e.g. `OliveiraLands.OliViewer`)
   - **Package/Identity/Publisher** (e.g. `CN=XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX`)
   - **Package/Properties/PublisherDisplayName**

---

## 3. Option A — Package as MSIX (Recommended)

1. Create a staging folder (e.g. `dist/msix/`) containing:
   - `aurora-viewer.exe` (from `target/release/aurora-viewer.exe`)
   - `Assets/` with required Store tile PNGs (`Square44x44Logo.png`, `Square150x150Logo.png`, `StoreLogo.png`, `Wide310x150Logo.png`)
   - `AppxManifest.xml` declaring `<Identity Name="..." Publisher="..." Version="0.1.0.0" ProcessorArchitecture="x64" />`, `<Capabilities><Capability Name="internetClient" /><DeviceCapability Name="microphone" /></Capabilities>`, and `runFullTrust` restricted capability for desktop Win32 execution.
2. Build the `.msix` package:
   ```powershell
   makeappx pack /d dist/msix /p dist/OliViewer-x64.msix
   ```
3. **Note on signing for Store upload:** When uploading `.msix` to Microsoft Partner Center, Microsoft signs the final Store package for you. For local sideload testing before upload, sign with a self-signed test certificate matching your manifest `Publisher` and install it into `LocalMachine\TrustedPeople`.
4. Run **Windows App Certification Kit (WACK)** locally against the `.msix` to catch manifest or dependency issues before submission.

---

## 4. Option B — Win32 Installer Submission (No MSIX)

If you already distribute an installer (`.exe` / `.msi`) over HTTPS (for example via GitHub Releases):

1. In Partner Center, choose **Traditional desktop app (EXE or MSI)** during product creation.
2. Provide the direct HTTPS download URL to your signed installer and silent install switches.
3. Microsoft Store will index the listing and trigger your installer directly.

---

## 5. Store Certification Notes for OliViewer

- **Restricted capability `runFullTrust`**: Required for desktop Win32 apps in MSIX; explain briefly in the Partner Center **Notes for certification** why full trust is needed (3D Vulkan/OpenGL rendering, local texture cache, and optional media plugins).
- **Privacy policy URL**: Required because the viewer connects to external grids and optionally uses the microphone for WebRTC voice.
- **Age rating & content**: Complete the IARC questionnaire accurately (user-generated 3D worlds and online chat).
