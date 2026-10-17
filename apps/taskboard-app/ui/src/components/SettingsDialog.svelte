<script lang="ts">
  import { Archive, Bell, Clock, Database, Download, FolderOpen, Info, Monitor, Palette, PanelLeft, RotateCcw, Upload } from '@lucide/svelte'
  import type { BackupInfo } from '../lib/bindings/BackupInfo'
  import { exportTo, importFrom } from '../lib/data'
  import type { AppSettings } from '../lib/bindings/AppSettings'
  import type { NotifySettings } from '../lib/bindings/NotifySettings'
  import { cal } from '../lib/cal.svelte'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { ACCENTS, setAccentHue, setThemePref, theme, type ThemePref } from '../lib/theme.svelte'
  import { toast } from '../lib/toast.svelte'
  import { app } from '../lib/store.svelte'
  import LayoutSettings from './LayoutSettings.svelte'
  import { appVersion, CHANGELOG_URL, LICENSE_URL, RELEASES_URL, REPO_URL } from '../lib/about'
  import { openLink } from '../lib/ipc'
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  let version = $state('')
  void appVersion().then((v) => (version = v))

  // Opened from "Customize sidebar…": jump to that section.
  let layoutSection = $state<HTMLElement | null>(null)
  let aboutSection = $state<HTMLElement | null>(null)
  $effect(() => {
    const target = app.settingsSection === 'layout' ? layoutSection : app.settingsSection === 'about' ? aboutSection : null
    if (target) {
      target.scrollIntoView({ block: 'start' })
      app.settingsSection = null
    }
  })

  let notify = $state<NotifySettings | null>(null)
  let appSettings = $state<AppSettings | null>(null)
  let saving = $state(false)
  let weekStart = $state(cal.weekStart)
  let backups = $state<BackupInfo[]>([])
  let hasSample = $state(false)
  let confirmRestore = $state<string | null>(null)

  async function loadData() {
    try {
      ;[backups, hasSample] = await Promise.all([ipc.listBackups(), ipc.hasSample()])
    } catch {
      backups = []
    }
  }
  $effect(() => {
    if (inTauri) void loadData()
  })

  async function backupNow() {
    try {
      await ipc.backupNow()
      toast('Backup saved')
      await loadData()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  async function restore(b: BackupInfo) {
    if (confirmRestore !== b.path) {
      confirmRestore = b.path
      return
    }
    confirmRestore = null
    try {
      await ipc.restoreBackup(b.path)
      toast('Restored. Your previous data was saved as a before-restore backup.')
      onclose()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  async function toggleSample() {
    try {
      if (hasSample) await ipc.removeSample()
      else await ipc.createSample()
      toast(hasSample ? 'Sample workspace removed' : 'Sample workspace added', { action: { label: 'Undo', run: () => void ipc.undo() } })
      hasSample = !hasSample
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  const when = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' })
  const size = (b: number) => (b > 1_048_576 ? `${(b / 1_048_576).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`)
  const KIND: Record<string, string> = { daily: 'Daily', manual: 'Manual', 'pre-migrate': 'Before upgrade', 'pre-import': 'Before import', 'pre-restore': 'Before restore' }

  $effect(() => {
    if (!inTauri) return
    Promise.all([ipc.notifySettings(), ipc.appSettings()])
      .then(([n, a]) => {
        notify = n
        appSettings = a
      })
      .catch((err) => toast(errorMessage(err), { kind: 'error' }))
  })

  async function save() {
    saving = true
    try {
      if (notify) await ipc.setNotifySettings(notify)
      if (appSettings) await ipc.setAppSettings(appSettings)
      cal.weekStart = weekStart
      try {
        localStorage.setItem('taskboard.weekStart', String(weekStart))
      } catch {
        // per-device convenience only
      }
      toast('Settings saved')
      onclose()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      saving = false
    }
  }

  const THEMES: { v: ThemePref; l: string }[] = [
    { v: 'system', l: 'System' },
    { v: 'light', l: 'Light' },
    { v: 'dark', l: 'Dark' },
  ]
</script>

<Modal title="Settings" {onclose} width={560}>
  <div class="flex flex-col gap-5">
    <section>
      <h3 class="sec"><Palette size={14} /> Appearance</h3>
      <div class="flex flex-wrap items-center gap-3">
        <div class="flex rounded-md border border-border bg-surface-1 p-0.5" role="radiogroup" aria-label="Theme">
          {#each THEMES as t (t.v)}
            <button role="radio" aria-checked={theme.pref === t.v} class="seg" onclick={() => setThemePref(t.v)}>{t.l}</button>
          {/each}
        </div>
        <div class="flex gap-1.5" role="radiogroup" aria-label="Accent color">
          {#each ACCENTS as a (a.hue)}
            <button role="radio" aria-checked={theme.accentHue === a.hue} aria-label={a.name} title={a.name} class="swatch" style="--s: oklch(0.6 0.17 {a.hue})" onclick={() => setAccentHue(a.hue)}></button>
          {/each}
        </div>
      </div>
    </section>

    <section bind:this={layoutSection}>
      <h3 class="sec"><PanelLeft size={14} /> Sidebar &amp; views</h3>
      <LayoutSettings />
    </section>

    {#if notify}
      <section>
        <h3 class="sec"><Bell size={14} /> Reminders</h3>
        <div class="flex flex-col gap-2 text-[13px]">
          <label class="row"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={notify.enabled} /> Show reminders</label>
          <div class="grid grid-cols-2 gap-2" class:opacity-50={!notify.enabled}>
            <label>
              <span class="field-label">Meetings — minutes before</span>
              <input type="number" class="field" min="0" max="1440" bind:value={notify.meeting_lead_min} disabled={!notify.enabled} />
            </label>
            <label>
              <span class="field-label">Timed tasks — minutes before</span>
              <input type="number" class="field" min="0" max="1440" bind:value={notify.task_lead_min} disabled={!notify.enabled} />
            </label>
          </div>
          <div class="flex items-center gap-2" class:opacity-50={!notify.enabled}>
            <label class="row"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={notify.digest} disabled={!notify.enabled} /> Morning summary at</label>
            <input type="time" class="field !h-7 !w-[110px]" aria-label="Morning summary time" bind:value={notify.digest_time} disabled={!notify.enabled || !notify.digest} />
          </div>
          <div>
            <button class="btn !h-7 text-xs" onclick={() => void ipc.testNotification().catch((err) => toast(errorMessage(err), { kind: 'error' }))}>Send a test notification</button>
          </div>
        </div>
      </section>
    {/if}

    {#if appSettings}
      <section>
        <h3 class="sec"><Monitor size={14} /> Windows</h3>
        <div class="flex flex-col gap-2 text-[13px]">
          <label class="row"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={appSettings.close_to_tray} /> Keep running in the tray when the window is closed <span class="text-subtle">(needed for reminders)</span></label>
          <label class="row"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={appSettings.autostart} /> Start with Windows (minimized to the tray)</label>
          <p class="text-xs text-subtle">Quick add from anywhere: Ctrl+Shift+Space.</p>
        </div>
      </section>
    {/if}

    <section>
      <h3 class="sec"><Database size={14} /> Data</h3>
      <p class="mb-2 text-xs text-muted">Everything stays on this computer. A backup is made automatically every day, and before every upgrade, import and restore.</p>
      <div class="flex flex-wrap gap-1.5">
        <button class="btn !h-7 text-xs" onclick={() => void backupNow()}><Archive size={13} /> Back up now</button>
        <button class="btn !h-7 text-xs" onclick={() => void ipc.openBackupsFolder().catch((err) => toast(errorMessage(err), { kind: 'error' }))}><FolderOpen size={13} /> Open backups folder</button>
      </div>
      {#if backups.length}
        <ul class="mt-2 flex max-h-40 flex-col overflow-y-auto rounded-md border border-border">
          {#each backups.slice(0, 12) as b (b.path)}
            <li class="flex h-8 items-center gap-2 border-b border-border px-2 text-xs last:border-0">
              <span class="tabular w-36">{when.format(b.created_at)}</span>
              <span class="flex-1 text-muted">{KIND[b.kind] ?? b.kind}</span>
              <span class="tabular text-subtle">{size(b.size_bytes)}</span>
              <button class="btn !h-6 !px-2 text-xs {confirmRestore === b.path ? 'btn-danger' : ''}" onclick={() => void restore(b)}>
                <RotateCcw size={12} /> {confirmRestore === b.path ? 'Click to confirm' : 'Restore'}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="mt-3 grid grid-cols-2 gap-2 text-xs">
        <div>
          <span class="field-label">Export</span>
          <div class="flex flex-wrap gap-1.5">
            <button class="btn !h-7 text-xs" onclick={() => void exportTo('json')}><Download size={13} /> Everything (JSON)</button>
            <button class="btn !h-7 text-xs" onclick={() => void exportTo('csv')}>Tasks (CSV)</button>
            <button class="btn !h-7 text-xs" onclick={() => void exportTo('ics')}>Calendar (ICS)</button>
          </div>
        </div>
        <div>
          <span class="field-label">Import</span>
          <div class="flex flex-wrap gap-1.5">
            <button class="btn !h-7 text-xs" onclick={() => void importFrom('json')}><Upload size={13} /> Taskboard JSON</button>
            <button class="btn !h-7 text-xs" title="Todoist, Trello, Notion or any CSV with a title column" onclick={() => void importFrom('csv')}>CSV (Todoist, Trello, Notion…)</button>
            <button class="btn !h-7 text-xs" title="Calendar export from Outlook, Google Calendar…" onclick={() => void importFrom('ics')}>Calendar (ICS)</button>
          </div>
        </div>
      </div>
      <div class="mt-3">
        <button class="btn !h-7 text-xs" onclick={() => void toggleSample()}>{hasSample ? 'Remove sample workspace' : 'Add sample workspace'}</button>
      </div>
    </section>

    <section>
      <h3 class="sec"><Clock size={14} /> Calendar</h3>
      <div class="flex flex-wrap items-center gap-3 text-[13px]">
        <label class="flex items-center gap-2">
          Week starts on
          <select class="field !h-7 !w-[120px]" bind:value={weekStart}>
            <option value={1}>Monday</option>
            <option value={0}>Sunday</option>
            <option value={6}>Saturday</option>
          </select>
        </label>
        <button class="btn !h-7 text-xs" onclick={() => { onclose(); cal.workingHoursOpen = true }}>Working hours &amp; days off…</button>
      </div>
    </section>

    <section bind:this={aboutSection}>
      <h3 class="sec"><Info size={14} /> About</h3>
      <p class="text-[13px]"><strong>Taskboard {version ? `v${version}` : ''}</strong> <span class="text-muted">· © 2026 Muneeb Ahmad</span></p>
      <p class="mt-0.5 text-xs text-muted">Free for personal use; commercial use needs permission.</p>
      <div class="mt-2 flex flex-wrap gap-2">
        <button class="btn !h-7 text-xs" onclick={() => void openLink(CHANGELOG_URL)}>What's new</button>
        <button class="btn !h-7 text-xs" onclick={() => void openLink(RELEASES_URL)}>All releases</button>
        <button class="btn !h-7 text-xs" onclick={() => void openLink(REPO_URL)}>Source on GitHub</button>
        <button class="btn !h-7 text-xs" onclick={() => void openLink(LICENSE_URL)}>License</button>
      </div>
    </section>
  </div>

  {#snippet footer()}
    <span class="text-xs text-subtle">Settings are stored in your local database and included in backups.</span>
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={saving} onclick={() => void save()}>Save</button>
  {/snippet}
</Modal>

<style>
  .sec {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-bottom: 8px;
    font-size: 13px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }
  .seg {
    height: 26px;
    padding-inline: 0.7rem;
    border-radius: 5px;
    color: var(--text-muted);
  }
  .seg[aria-checked='true'] {
    background: var(--surface-3);
    color: var(--text);
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 99px;
    background: var(--s);
  }
  .swatch[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface-2),
      0 0 0 4px var(--s);
  }
</style>
