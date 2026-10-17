// App version and project links (About, status bar, command palette).
import { getVersion } from '@tauri-apps/api/app'
import { inTauri, openLink } from './ipc'

export const REPO_URL = 'https://github.com/Muneeb-Ahmad-Ch/TaskBoard'
export const CHANGELOG_URL = `${REPO_URL}/blob/main/CHANGELOG.md`
export const RELEASES_URL = `${REPO_URL}/releases`
export const LICENSE_URL = `${REPO_URL}/blob/main/LICENSE`

let cached: Promise<string> | null = null

/** The installed version, from the app's own build metadata (tauri.conf.json). */
export function appVersion(): Promise<string> {
  cached ??= inTauri ? getVersion().catch(() => '') : Promise.resolve('')
  return cached
}

export function openChangelog() {
  void openLink(CHANGELOG_URL)
}
