# Releasing Taskboard (Windows)

## 1. Prepare

1. Make sure `main` is green in CI (`CI` and `Secret scan` workflows).
2. Bump the version in **both** `Cargo.toml` (`[workspace.package] version`)
   and `apps/taskboard-app/src-tauri/tauri.conf.json` (`version`).
3. Commit: `chore: release vX.Y.Z`.

## 2. Build

Locally:

```bash
cd apps/taskboard-app/ui
pnpm install --frozen-lockfile
pnpm app:build
```

Installers land in `target/release/bundle/`:

- `nsis/Taskboard_X.Y.Z_x64-setup.exe` (recommended; per-user install, no admin)
- `msi/Taskboard_X.Y.Z_x64_en-US.msi` (for managed deployment)

Or push a tag and let CI build them:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

`.github/workflows/release.yml` builds both installers on `windows-latest` and
attaches them to a **draft** GitHub release. Review and publish it by hand.

## 3. Code signing (recommended, not yet configured)

Unsigned installers trigger SmartScreen warnings. To sign:

1. Get a code-signing option: **Azure Trusted Signing** (cheapest for
   individuals) or an OV/EV Authenticode certificate.
2. Store the credentials as GitHub Actions secrets. Never commit
   certificates or keys: `.gitignore` blocks `*.pfx`, `*.p12`, `*.pem`,
   `*.key`, and `.github/workflows/secrets.yml` runs gitleaks.
3. Configure `bundle.windows.signCommand` (Trusted Signing) or
   `certificateThumbprint` in `tauri.conf.json`, as described in
   <https://tauri.app/distribute/sign/windows/>.

## 4. Auto-update (optional, not yet configured)

Tauri's updater needs a signing key pair that **you** own:

```bash
cd apps/taskboard-app/ui
pnpm tauri signer generate -w %USERPROFILE%\.tauri\taskboard.key
```

- Keep the private key **out of the repo**. Store it and its password as the
  GitHub secrets `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Put the **public** key in `tauri.conf.json` under `plugins.updater.pubkey`,
  set the endpoint to
  `https://github.com/Muneeb-Ahmad-Ch/TaskBoard/releases/latest/download/latest.json`,
  and add `tauri-plugin-updater` to the app.
- `tauri-action` then uploads `latest.json` and signatures with each release.

## 5. Release checklist

- [ ] Fresh install on a clean Windows 11 VM: first run shows the welcome
      screen; the sample workspace adds and removes.
- [ ] Upgrade over the previous version: data is intact and a `pre-migrate`
      backup exists.
- [ ] Tray icon, close-to-tray, autostart (sign out and back in), reminders
      and the Join button.
- [ ] Measure and note in the release notes: cold start (log line
      `window shown`), idle memory (Task Manager, all Taskboard + WebView2
      processes), installer size.
- [ ] Light and dark themes look right on every view.
