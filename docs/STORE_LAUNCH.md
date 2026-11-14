# Store launch checklist

Where things stand (November 2026): CI builds the desktop app for macOS, Windows and Linux, and unsigned MSIX packages for the Microsoft Store. All workflows pass. What's left is mostly account setup and the Store listing.

## 1. Test a real Windows build

- [ ] Download the Windows installer from the latest **Desktop app** run (GitHub → Actions → run → Artifacts) and install it on a Windows PC.
- [ ] **Move / Click / Type:** confirm each works.
- [ ] **Type → Specific keys:** test in the game you have in mind. This also shows whether its anti-cheat accepts simulated input.
- [ ] **Keep awake:** while it runs, `powercfg /requests` (admin terminal) should list Automau5.

## 2. Partner Center

- [ ] Create a Partner Center account at <https://storedeveloper.microsoft.com>. Individual registration is reportedly free; confirm when signing up.
- [ ] Reserve the app name (**Automau5**, or your choice).
- [ ] From **Product identity**, add three repository variables (GitHub → Settings → Secrets and variables → Actions → Variables):
  - `MSIX_IDENTITY_NAME`: Package/Identity/Name
  - `MSIX_PUBLISHER`: Package/Identity/Publisher (starts with `CN=`)
  - `MSIX_PUBLISHER_DISPLAY_NAME`

## 3. Build the real Store packages

- [ ] Actions → **MSIX (Microsoft Store)** → **Run workflow** on `main`. No new tag is needed; 0.1.0 is fine for the first upload.
- [ ] Check the run shows no "placeholder identity" warning.
- [ ] Download the `automau5-msix-x64` and `automau5-msix-arm64` artifacts.

## 4. Privacy policy

The page is [`docs/privacy.html`](privacy.html). Host it with GitHub Pages:

- [ ] GitHub → Settings → Pages → Source: **Deploy from a branch** → `main` / `/docs`.
- [ ] Confirm it loads at <https://ltratcliff.github.io/automau5/privacy.html>.
- [ ] Use that URL in the Store listing.

## 5. Store listing

- [ ] Description that says plainly what the app does: "keeps your PC awake; can simulate mouse movement, clicks and key presses on a timer". Reviewers are more comfortable with a utility that explains itself.
- [ ] Screenshots of each mode.
- [ ] Age-rating questionnaire.
- [ ] Privacy policy URL from step 4.

## 6. Submit

- [ ] Partner Center → **Start submission** → **Packages** → upload both `.msix` files.
- [ ] Submit for certification (typically 1–3 business days).

**For later updates:** bump `version` in `app/src-tauri/tauri.conf.json`, push a matching tag (`git tag v0.1.1 && git push origin v0.1.1`), and upload the new packages. Each upload needs a higher version.

## Before launch

- [x] **Icon:** an "A" with mouse ears. The source is `app/src-tauri/app-icon.svg`. To change it, edit the SVG and run `rsvg-convert -w 1024 -h 1024 src-tauri/app-icon.svg -o src-tauri/app-icon.png && npx tauri icon src-tauri/app-icon.png -o src-tauri/icons` from `app/` (in `nix-shell`), then copy `Square150x150Logo.png`, `Square44x44Logo.png` and `StoreLogo.png` into `app/msix/Assets/`.
- [ ] **App identifier:** replace `com.automau5.app` in `tauri.conf.json` with a reverse-DNS id you own. Windows doesn't use it (MSIX identity comes from Partner Center), but the Mac App Store does, and it can't change after the first Mac submission.

## Housekeeping

- [x] Delete the merged branch: `git push origin --delete desktop-app && git branch -d desktop-app`.
- [x] Delete the empty `package-lock.json` in the repo root.

## Signed macOS DMG (outside the App Store)

The `macOS (signed + notarized)` workflow is ready. It needs:

- [ ] Apple Developer Program membership ($99/yr).
- [ ] The five `APPLE_*` repository secrets. Setup steps are in [`app/README.md`](../app/README.md#signed-macos-builds-developer-id).
- [ ] A test run (Actions → **macOS (signed + notarized)** → **Run workflow**). Then download the DMG and open it on a Mac with no Gatekeeper warning.

## After Windows: Mac App Store

The `Mac App Store` workflow is ready. Setup details are in [`app/README.md`](../app/README.md#mac-app-store).

- [ ] Apple Developer Program membership ($99/yr).
- [ ] Final `identifier` in `tauri.conf.json`, registered as an App ID, with an app record in App Store Connect.
- [ ] Apple Distribution + Mac Installer Distribution certificates, exported together as one `.p12`.
- [ ] Mac App Store Connect provisioning profile.
- [ ] Secrets `APPLE_MAS_CERTIFICATE`, `APPLE_MAS_CERTIFICATE_PASSWORD`, `APPLE_MAS_PROVISION_PROFILE`, and variables `APPLE_TEAM_ID`, `APPLE_APP_ID` (plus the `APPLE_API_*` secrets for uploading).
- [ ] Run the workflow with **upload** ticked, then install the build from TestFlight.
- [ ] Test every mode in the sandboxed TestFlight build, especially Move/Click/Type with Accessibility allowed.
- [ ] Store listing (screenshots, description, privacy policy URL, age rating) and submit for review. App Review is the uncertain step for the input modes; keep awake should be fine.
