# Automau5 desktop app

Tauri 2 + React GUI on top of the `automau5` engine in [`../src/lib.rs`](../src/lib.rs) (the same engine the `am` CLI uses).

```
app/
├── src/               React UI (Vite + TypeScript)
└── src-tauri/
    ├── src/main.rs    Tauri commands: start_job / stop_job / job_status, emits "job-event"
    ├── tauri.conf.json
    ├── tauri.appstore.conf.json        overlay for Mac App Store builds
    ├── tauri.microsoftstore.conf.json  overlay for Microsoft Store builds
    ├── Entitlements.plist / Info.plist macOS sandbox + App Store metadata
    └── icons/         generated from app-icon.png with `npx tauri icon`
```

## Develop

Requires Node 20+, Rust, and the [Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
cd app
npm install
npm run tauri dev
```

On NixOS, run `nix-shell` in the repo root first; [`shell.nix`](../shell.nix) provides all the system libraries.

## Release builds

```sh
npm run tauri build            # installers for the current OS
```

The icon source is `src-tauri/app-icon.svg`. After editing it, regenerate every size: `rsvg-convert -w 1024 -h 1024 src-tauri/app-icon.svg -o src-tauri/app-icon.png && npx tauri icon src-tauri/app-icon.png -o src-tauri/icons`, then copy the three Store logos into `msix/Assets/`.

Before submitting to either store, change `identifier` in `tauri.conf.json` (currently the placeholder `com.automau5.app`) to a reverse-DNS id you own. It can't be changed after the first store submission.

## Signed macOS builds (Developer ID)

For distributing outside the App Store (website, GitHub releases). The [`macOS (signed + notarized)` workflow](../.github/workflows/macos-signed.yml) builds a universal DMG signed with your Developer ID and notarized by Apple, so it opens without Gatekeeper warnings. Run it from the Actions tab or push a `v*` tag; the DMG is in the run's artifacts as `automau5-macos-universal-dmg`.

It needs an Apple Developer Program membership and five repository secrets (Settings → Secrets and variables → Actions → **Secrets**):

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | Your Developer ID Application certificate and private key, exported as `.p12` and base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set when exporting the `.p12` |
| `APPLE_API_ISSUER` | App Store Connect API issuer ID |
| `APPLE_API_KEY` | App Store Connect API key ID |
| `APPLE_API_KEY_P8` | Full contents of the downloaded `AuthKey_<KEYID>.p8` file |

**Certificate** (needs a Mac, done once):
1. In [Certificates, IDs & Profiles](https://developer.apple.com/account/resources/certificates/list), create a **Developer ID Application** certificate. Only the account holder can create one. Follow the prompts to upload a certificate signing request from Keychain Access, then download the certificate and double-click it to install.
2. In Keychain Access → login → **My Certificates**, right-click "Developer ID Application: …" → **Export** → save as `.p12` with a password.
3. Base64-encode it and paste the output into `APPLE_CERTIFICATE`:
   ```sh
   openssl base64 -A -in Certificates.p12 | pbcopy
   ```

**Notarization key:**
1. In [App Store Connect → Users and Access → Integrations → App Store Connect API](https://appstoreconnect.apple.com/access/integrations/api), generate a **Team Key** with the **Developer** role.
2. Copy the **Issuer ID** (above the table) into `APPLE_API_ISSUER` and the key's **Key ID** into `APPLE_API_KEY`.
3. Download the key (`AuthKey_<KEYID>.p8`, you can only download it once) and paste its full contents, including the `BEGIN`/`END` lines, into `APPLE_API_KEY_P8`.

The workflow imports the certificate into a temporary keychain, lets `tauri build` sign the app (hardened runtime) and notarize it, then also notarizes and staples the DMG. Before uploading, it checks the result with `codesign`, `spctl` (Gatekeeper) and `stapler`. A run with missing secrets fails at the first step and names the missing ones.

## Mac App Store

The [`Mac App Store` workflow](../.github/workflows/mac-app-store.yml) builds a sandboxed, universal app signed for the App Store and wraps it in a signed `.pkg`. App Store builds aren't notarized; Apple reviews them instead.

- **Tag pushes (`v*`)** build the `.pkg` as the `automau5-mac-app-store-pkg` artifact only.
- **Manual runs** (Actions → **Mac App Store** → **Run workflow**) can also upload it to App Store Connect if you tick **upload**. Uploads then show up under **TestFlight** after processing.

Each run uses its run number as the build number (`CFBundleVersion`), so every upload is unique. The version shown to users (`CFBundleShortVersionString`) comes from `version` in `tauri.conf.json`.

### One-time setup

**1. Bundle id and app record**
1. Pick the final `identifier` in `src-tauri/tauri.conf.json` (it can't change after the first submission).
2. In [Certificates, IDs & Profiles → Identifiers](https://developer.apple.com/account/resources/identifiers/list), register an explicit App ID with that bundle id. Platform: macOS.
3. In [App Store Connect](https://appstoreconnect.apple.com/apps), create a new macOS app with that bundle id. Its numeric **Apple ID** (App Information page) goes in the `APPLE_APP_ID` variable.

**2. Certificates** (on a Mac). Create both in [Certificates](https://developer.apple.com/account/resources/certificates/list) and install them:
- **Apple Distribution** (or the older **Mac App Distribution**) for signing the app.
- **Mac Installer Distribution** for signing the `.pkg`. Its identity is named "3rd Party Mac Developer Installer: …".

Then in Keychain Access → login → **My Certificates**, select **both** entries, right-click → **Export 2 items…** → save one `.p12` with a password:
```sh
openssl base64 -A -in Certificates.p12 | pbcopy   # → APPLE_MAS_CERTIFICATE
```

**3. Provisioning profile.** In [Profiles](https://developer.apple.com/account/resources/profiles/list), create a **Mac App Store Connect** profile for your App ID and the distribution certificate, then download it:
```sh
openssl base64 -A -in Automau5.provisionprofile | pbcopy   # → APPLE_MAS_PROVISION_PROFILE
```

**4. Secrets and variables** (Settings → Secrets and variables → Actions):

| Name | Kind | Value |
|---|---|---|
| `APPLE_MAS_CERTIFICATE` | secret | base64 `.p12` with both identities (step 2) |
| `APPLE_MAS_CERTIFICATE_PASSWORD` | secret | its export password |
| `APPLE_MAS_PROVISION_PROFILE` | secret | base64 provisioning profile (step 3) |
| `APPLE_TEAM_ID` | variable | your 10-character Team ID (Membership details) |
| `APPLE_APP_ID` | variable | the app's numeric Apple ID (step 1), only for uploads |
| `APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_P8` | secrets | App Store Connect API key, only for uploads. Same secrets as the [signed DMG](#signed-macos-builds-developer-id). The key needs at least the **Developer** role |

The workflow checks these first and names any that are missing. It also fails if the provisioning profile doesn't match `APPLE_TEAM_ID` plus the bundle id. After building, it verifies the sandbox entitlement, the embedded profile, the build number and the package signature.

`src-tauri/Entitlements.plist` is a template: CI replaces `TEAM_ID` and `BUNDLE_ID`. For a local App Store build, replace them by hand, put the profile at `src-tauri/embedded.provisionprofile` (gitignored), and use `--config src-tauri/tauri.appstore.conf.json`. See <https://v2.tauri.app/distribute/app-store/>.

### Before submitting for review

Test the TestFlight build on a Mac. This is the first build that runs inside the **App Sandbox**:
- **Keep awake** uses IOKit power assertions, which are allowed in the sandbox and need no permission.
- **Move/Click/Type** need **Accessibility** permission (System Settings → Privacy & Security → Accessibility). The app shows the system prompt the first time Start is pressed. Sandboxed apps can post input once allowed, but check it really works in the TestFlight build. If it doesn't, or App Review objects, you can ship with keep awake only.

## Microsoft Store

Shipped as an unsigned **MSIX** that the Store signs, so you don't need a code-signing certificate or an updater. The [`MSIX` workflow](../.github/workflows/msix.yml) builds x64 and arm64 packages.

1. Create a Partner Center account and reserve the app name.
2. Copy the values from Partner Center → your app → **Product identity** into repository variables (Settings → Secrets and variables → Actions → Variables):
   - `MSIX_IDENTITY_NAME` (Package/Identity/Name)
   - `MSIX_PUBLISHER` (Package/Identity/Publisher, starts with `CN=`)
   - `MSIX_PUBLISHER_DISPLAY_NAME`
3. Bump `version` in `src-tauri/tauri.conf.json` (each Store upload needs a higher version), then either run the workflow from the Actions tab or push a matching tag:
   ```sh
   git tag v0.1.0 && git push origin v0.1.0
   ```
4. Download the `automau5-msix-x64` / `automau5-msix-arm64` artifacts and upload the `.msix` files in Partner Center → **Start submission** → **Packages**.

The manifest template is [`msix/Package.appxmanifest`](msix/Package.appxmanifest); CI fills in the identity, version (`MAJOR.MINOR.PATCH.0`) and architecture.

**Testing locally on Windows.** The CI packages are unsigned, so they won't install. Build a self-signed one with [winapp CLI](https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/tauri):

```powershell
winget install Microsoft.winappcli --source winget
npm run tauri build -- --no-bundle
mkdir dist-msix; copy ..\target\release\automau5-app.exe dist-msix\; xcopy /E /I msix\Assets dist-msix\Assets
winapp cert generate --manifest msix\Package.appxmanifest --if-exists skip
winapp cert install .\devcert.pfx          # as administrator
winapp pack dist-msix --manifest msix\Package.appxmanifest --cert .\devcert.pfx
Add-AppxPackage .\Automau5.Dev_0.1.0.0_x64.msix
```

`src-tauri/tauri.microsoftstore.conf.json` is for the alternative MSI route (signed installer hosted at your own URL). You don't need it for MSIX.
