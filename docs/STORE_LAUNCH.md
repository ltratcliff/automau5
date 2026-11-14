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

- [ ] **Icon:** the cursor icon is a generated placeholder. Put a 1024×1024 PNG at `app/src-tauri/app-icon.png`, run `npx tauri icon src-tauri/app-icon.png -o src-tauri/icons` from `app/`, then copy `Square150x150Logo.png`, `Square44x44Logo.png` and `StoreLogo.png` into `app/msix/Assets/` again.
- [ ] **App identifier:** replace `com.automau5.app` in `tauri.conf.json` with a reverse-DNS id you own. Windows doesn't use it (MSIX identity comes from Partner Center), but the Mac App Store does, and it can't change after the first Mac submission.

## Housekeeping

- [ ] Delete the merged branch: `git push origin --delete desktop-app && git branch -d desktop-app`.
- [ ] Delete the empty `package-lock.json` in the repo root.

## After Windows: Mac App Store

The bigger job. Details are in [`app/README.md`](../app/README.md#mac-app-store).

- [ ] Apple Developer Program membership ($99/yr).
- [ ] Register the bundle id (matching `identifier`) and create the app in App Store Connect.
- [ ] Mac App Store provisioning profile at `app/src-tauri/embedded.provisionprofile`; replace `TEAM_ID` in `Entitlements.plist`.
- [ ] Signing: build on a Mac, or add signing certificates as CI secrets for a signed macOS workflow.
- [ ] App Review is the uncertain step for Move/Click/Type (input generation needs Accessibility permission). Keep awake should be fine. If review objects, ship with keep awake only.
