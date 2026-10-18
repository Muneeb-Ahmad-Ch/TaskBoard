# Releasing Taskboard (Windows)

## 1. Prepare

1. Make sure `main` is green in CI (`CI` and `Secret scan` workflows).
2. Bump the version in `Cargo.toml` (`[workspace.package] version`),
   `apps/taskboard-app/src-tauri/tauri.conf.json` (`version`), and the
   `taskboard-core` dependency pins in `crates/taskboard-cli/Cargo.toml` and
   `apps/taskboard-app/src-tauri/Cargo.toml`, then run `cargo check` to update
   `Cargo.lock`. The app shows this number in the status bar and Settings → About.
3. Update `CHANGELOG.md`: rename **[Unreleased]** to **[X.Y.Z] - date**,
   start a new empty **[Unreleased]** section, and update the compare links
   at the bottom. The app's "What's new" opens this file on GitHub.
4. Commit: `chore: release vX.Y.Z`.

## 2. Build

Locally:

```bash
cd apps/taskboard-app/ui
pnpm install --frozen-lockfile
# The update packages are signed, so a release build needs the key (section 4):
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/taskboard.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat ~/.tauri/taskboard.key.password)"
pnpm app:build
```

Without the key (a contributor's build, a quick local test), use
`pnpm app:build:unsigned`: the same installers, without update packages.

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

## 4. In-app updates

Installed copies (0.5 and later) update themselves from Settings → Updates:
they read
`https://github.com/Muneeb-Ahmad-Ch/TaskBoard/releases/latest/download/latest.json`,
download the new `-setup.exe`, verify it against the **public key** in
`tauri.conf.json` (`plugins.updater.pubkey`, with `requireSignedVersion`, so
an older signed package can't be passed off as a newer one), back up the
database (`pre-update`), and run the installer passively; it closes
Taskboard and reopens the new version.

**The signing key** is a minisign key pair made with
`pnpm tauri signer generate`. The private key and its password live outside
the repo, in `%USERPROFILE%\.tauri\` (`taskboard.key`,
`taskboard.key.password`). **Back both up** (a password manager): without
them no update can be signed, and installed copies can't update until users
reinstall by hand.

**One-time setup** on GitHub → repository → Settings → Secrets and variables
→ Actions → New repository secret:

- `TAURI_SIGNING_PRIVATE_KEY`: the contents of `taskboard.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the contents of `taskboard.key.password`

The release workflow stops early with a clear error when they're missing.
With them, `tauri-action` signs the installers and uploads `latest.json`
(pointing at the NSIS installer) and the `.sig` files to the draft. Users see
the update only once you **publish** the draft: `latest/download` follows the
latest published release. The release notes (the version's CHANGELOG
section) are what the app shows under "What's new".

Checking an update by hand: after publishing, `latest.json` on the release
should list `windows-x86_64` with the new version; an installed older copy
finds it from Settings → Updates → Check for updates.

## 5. Release checklist

- [ ] The release has `latest.json` and `.sig` files; after publishing, an
      installed previous version finds the update, installs it and reopens
      on the new version with its data (and a `pre-update` backup).
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
