// In-app updates (Settings → Updates, status bar). Checks GitHub quietly at
// startup and every few hours unless turned off, remembers a skipped
// version, and installs a verified update: download with progress, a backup,
// then the installer closes Taskboard and opens the new version.
import { errorMessage, inTauri, ipc, type UpdateInfo } from './ipc'
import { app } from './store.svelte'
import { toast } from './toast.svelte'

export type UpdatePhase = 'idle' | 'checking' | 'none' | 'available' | 'downloading' | 'installing' | 'error'

const KEY = 'taskboard.updates'
/** How often the automatic check runs (it also runs at startup when due). */
export const CHECK_EVERY = 6 * 3_600_000

type Prefs = {
  /** Check automatically. */
  auto: boolean
  /** A version the user chose to skip: no reminder for it. */
  skipped: string
  /** Last successful check (Unix ms). */
  checkedAt: number
  /** The version the last check found, so a restart checks again at once. */
  found: string
}

function loadPrefs(): Prefs {
  try {
    const p = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Prefs>
    return {
      auto: p.auto !== false,
      skipped: typeof p.skipped === 'string' ? p.skipped : '',
      checkedAt: Number.isFinite(p.checkedAt) ? Number(p.checkedAt) : 0,
      found: typeof p.found === 'string' ? p.found : '',
    }
  } catch {
    return { auto: true, skipped: '', checkedAt: 0, found: '' }
  }
}

export const updates = $state({
  phase: 'idle' as UpdatePhase,
  info: null as UpdateInfo | null,
  error: '',
  /** Download progress in bytes; `total` is null when the server doesn't say. */
  downloaded: 0,
  total: null as number | null,
  ...loadPrefs(),
})

function save() {
  try {
    const { auto, skipped, checkedAt, found } = updates
    localStorage.setItem(KEY, JSON.stringify({ auto, skipped, checkedAt, found }))
  } catch {
    // per-device convenience only
  }
}

export const updateBusy = () => updates.phase === 'checking' || updates.phase === 'downloading' || updates.phase === 'installing'

export function openUpdates() {
  app.settingsSection = 'updates'
  app.settingsOpen = true
}

/**
 * Looks for a newer version. A quiet (automatic) check keeps errors to
 * itself and mentions a new version once, with a toast.
 */
export async function checkForUpdates(quiet = false): Promise<void> {
  if (!inTauri || updateBusy()) return
  const before = updates.phase
  updates.phase = 'checking'
  if (!quiet) updates.error = ''
  try {
    const info = await ipc.checkUpdate()
    updates.checkedAt = Date.now()
    const isNew = !!info && info.version !== updates.found
    updates.found = info?.version ?? ''
    save()
    updates.info = info
    updates.phase = info ? 'available' : 'none'
    updates.error = ''
    if (info && quiet && isNew && updates.skipped !== info.version)
      toast(`Taskboard ${info.version} is available`, { action: { label: 'View', run: openUpdates } })
  } catch (err) {
    if (quiet) {
      updates.phase = before === 'checking' ? 'idle' : before
    } else {
      updates.phase = 'error'
      updates.error = errorMessage(err)
    }
  }
}

/** Download, verify, back up, install. On Windows the app closes and reopens. */
export async function installUpdate(): Promise<void> {
  if (!inTauri || !updates.info || updateBusy()) return
  updates.phase = 'downloading'
  updates.downloaded = 0
  updates.total = null
  updates.error = ''
  // Let an edit in progress (a title, notes) save before the app closes.
  ;(document.activeElement as HTMLElement | null)?.blur?.()
  await new Promise((r) => setTimeout(r, 400))
  try {
    await ipc.installUpdate((p) => {
      if (p.event === 'downloading') {
        updates.downloaded = p.downloaded
        updates.total = p.total
      } else updates.phase = 'installing'
    })
  } catch (err) {
    updates.phase = 'error'
    updates.error = errorMessage(err)
  }
}

export function skipVersion() {
  if (!updates.info) return
  updates.skipped = updates.info.version
  save()
}

export function setAutoCheck(on: boolean) {
  updates.auto = on
  save()
  if (on && isDue()) void checkForUpdates(true)
}

/** Due when automatic checks are on and the last one is old, or found something. */
export function isDue(now = Date.now(), prefs: Pick<Prefs, 'auto' | 'checkedAt' | 'found'> = updates): boolean {
  if (!prefs.auto) return false
  return !!prefs.found || now - prefs.checkedAt >= CHECK_EVERY
}

let timer: ReturnType<typeof setInterval> | undefined

/** Startup: a quiet check shortly after launch when due, then again every so often. */
export function startUpdateChecks() {
  if (!inTauri) return
  clearInterval(timer)
  setTimeout(() => {
    if (isDue()) void checkForUpdates(true)
  }, 8_000)
  timer = setInterval(() => {
    if (updates.auto && Date.now() - updates.checkedAt >= CHECK_EVERY) void checkForUpdates(true)
  }, 15 * 60_000)
}

/** "12.3 MB" style sizes for the download progress. */
export function megabytes(bytes: number): string {
  return `${(bytes / 1_048_576).toFixed(bytes < 10 * 1_048_576 ? 1 : 0)} MB`
}
