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

/** Who makes Taskboard (Settings → About). */
export const DEVELOPER = {
  name: 'Muneeb Ahmad',
  email: 'muneeb.ahmad.freelancing@gmail.com',
  github: { label: 'Muneeb-Ahmad-Ch', url: 'https://github.com/Muneeb-Ahmad-Ch' },
  sites: [
    { label: 'ldta.io', url: 'https://ldta.io', main: true },
    { label: 'letsdotheautomation.com', url: 'https://letsdotheautomation.com', main: false },
    { label: 'mpysolutions.com', url: 'https://mpysolutions.com', main: false },
  ],
} as const

export const SHORTCUTS_URL = `${REPO_URL}/blob/main/docs/SHORTCUTS.md`
