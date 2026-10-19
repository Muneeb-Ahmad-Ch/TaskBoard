<script lang="ts">
  import ArchivedList from './ArchivedList.svelte'
  // Settings, organized by category (left) with the chosen page on the right.
  // Every change is saved immediately; there is no Save button.
  import { Archive, Bell, Bot, CalendarDays, Check, Columns3, Database, Download, ExternalLink, FolderOpen, FolderTree, Globe, Info, Mail, Minus, Monitor, Palette, PanelLeft, Plus, RefreshCw, RotateCcw, Search, Table2, Upload } from '@lucide/svelte'
  import { tick } from 'svelte'
  import { searchSettings, type SettingEntry } from '../lib/settingsIndex'
  import BoardColumnsSettings from './BoardColumnsSettings.svelte'
  import type { AppSettings } from '../lib/bindings/AppSettings'
  import type { BackupInfo } from '../lib/bindings/BackupInfo'
  import type { DbInfo } from '../lib/bindings/DbInfo'
  import type { NotifySettings } from '../lib/bindings/NotifySettings'
  import { appVersion, CHANGELOG_URL, DEVELOPER, LICENSE_URL, RELEASES_URL, REPO_URL, SHORTCUTS_URL } from '../lib/about'
  import { cal } from '../lib/cal.svelte'
  import { exportTo, importFrom } from '../lib/data'
  import { errorMessage, inTauri, ipc, openLink } from '../lib/ipc'
  import { app } from '../lib/store.svelte'
  import { ACCENTS, setAccentHue, setThemePref, theme, type ThemePref } from '../lib/theme.svelte'
  import { toast } from '../lib/toast.svelte'
  import AiSettings from './AiSettings.svelte'
  import Modal from './Modal.svelte'
  import SectionsSettings from './SectionsSettings.svelte'
  import SidebarSettings from './SidebarSettings.svelte'
  import Switch from './Switch.svelte'
  import UpdatesSettings from './UpdatesSettings.svelte'
  import ViewsSettings from './ViewsSettings.svelte'
  import { LEVELS, resetZoom, zoom, zoomIn, zoomOut } from '../lib/zoom.svelte'
  import { updates } from '../lib/updater.svelte'

  let { onclose }: { onclose: () => void } = $props()

  const PAGES = [
    { id: 'general', label: 'General', icon: Monitor },
    { id: 'appearance', label: 'Appearance', icon: Palette },
    { id: 'sidebar', label: 'Sidebar', icon: PanelLeft },
    { id: 'sections', label: 'Sections', icon: FolderTree },
    { id: 'views', label: 'Views & table', icon: Table2 },
    { id: 'columns', label: 'Board columns', icon: Columns3 },
    { id: 'calendar', label: 'Calendar', icon: CalendarDays },
    { id: 'reminders', label: 'Reminders', icon: Bell },
    { id: 'data', label: 'Data & backups', icon: Database },
    { id: 'ai', label: 'AI assistants', icon: Bot },
    { id: 'updates', label: 'Updates', icon: RefreshCw },
    { id: 'about', label: 'About', icon: Info },
  ] as const
  type PageId = (typeof PAGES)[number]['id']

  // "Customize sidebar…", "About Taskboard" and the like open a specific page.
  const OPEN_AT: Record<string, PageId> = {
    layout: 'sidebar',
    sidebar: 'sidebar',
    sections: 'sections',
    views: 'views',
    columns: 'columns',
    data: 'data',
    ai: 'ai',
    updates: 'updates',
    about: 'about',
  }
  let page = $state<PageId>(OPEN_AT[app.settingsSection ?? ''] ?? 'general')
  app.settingsSection = null

  // ---------- data ----------
  let notify = $state<NotifySettings | null>(null)
  let appSettings = $state<AppSettings | null>(null)
  let version = $state('')
  let dbInfo = $state<DbInfo | null>(null)
  let backups = $state<BackupInfo[]>([])
  let hasSample = $state(false)
  let confirmRestore = $state<string | null>(null)
  let savedAt = $state(0)

  void appVersion().then((v) => (version = v))

  $effect(() => {
    if (!inTauri) return
    Promise.all([ipc.notifySettings(), ipc.appSettings(), ipc.dbInfo()])
      .then(([n, a, d]) => {
        notify = n
        appSettings = a
        dbInfo = d
      })
      .catch((err) => toast(errorMessage(err), { kind: 'error' }))
    void loadBackups()
  })

  async function loadBackups() {
    try {
      ;[backups, hasSample] = await Promise.all([ipc.listBackups(), ipc.hasSample()])
    } catch {
      backups = []
    }
  }

  function saved() {
    savedAt = Date.now()
  }

  // ---------- immediate saving ----------
  async function setApp(patch: Partial<AppSettings>) {
    if (!appSettings) return
    const next = { ...appSettings, ...patch }
    try {
      appSettings = await ipc.setAppSettings(next)
      saved()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  let notifyTimer: ReturnType<typeof setTimeout> | undefined
  /** Number fields save after a short pause, so typing "15" saves once. */
  function setNotify(patch: Partial<NotifySettings>, delay = 0) {
    if (!notify) return
    notify = { ...notify, ...patch }
    clearTimeout(notifyTimer)
    const value = notify
    notifyTimer = setTimeout(() => {
      ipc
        .setNotifySettings(value)
        .then(saved)
        .catch((err) => toast(errorMessage(err), { kind: 'error' }))
    }, delay)
  }

  function minutesValue(input: string): number {
    const n = Math.round(Number(input))
    return Number.isFinite(n) ? Math.min(1440, Math.max(0, n)) : 0
  }

  function setWeekStart(value: number) {
    cal.weekStart = value
    try {
      localStorage.setItem('taskboard.weekStart', String(value))
    } catch {
      // per-device convenience only
    }
    saved()
  }

  // ---------- data actions ----------
  async function backupNow() {
    try {
      await ipc.backupNow()
      toast('Backup saved')
      await loadBackups()
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
  const KIND: Record<string, string> = { daily: 'Daily', manual: 'Manual', 'pre-migrate': 'Before upgrade', 'pre-update': 'Before update', 'pre-import': 'Before import', 'pre-restore': 'Before restore' }

  const THEMES: { v: ThemePref; l: string }[] = [
    { v: 'system', l: 'System' },
    { v: 'light', l: 'Light' },
    { v: 'dark', l: 'Dark' },
  ]
  const WEEK_STARTS = [
    { v: 1, l: 'Monday' },
    { v: 0, l: 'Sunday' },
    { v: 6, l: 'Saturday' },
  ]
  const current = $derived(PAGES.find((p) => p.id === page) ?? PAGES[0])

  // ---------- search ----------
  let query = $state('')
  let active = $state(0)
  let searchEl: HTMLInputElement | undefined = $state()
  const results = $derived(searchSettings(query))
  $effect(() => {
    void query
    active = 0
  })
  const pageLabel = (id: string) => PAGES.find((p) => p.id === id)?.label ?? ''

  /** Opens the setting's page and points it out. */
  async function go(r: SettingEntry | undefined) {
    if (!r) return
    query = ''
    page = r.page
    await tick()
    // Some rows appear once their page has loaded its values.
    for (let i = 0; i < 30; i++) {
      const el = document.querySelector<HTMLElement>(`#settings-panel [data-setting="${r.anchor}"]`)
      if (el) {
        el.scrollIntoView({ block: 'center' })
        el.classList.remove('flash')
        void el.offsetWidth
        el.classList.add('flash')
        setTimeout(() => el.classList.remove('flash'), 1700)
        return
      }
      await new Promise((resolve) => requestAnimationFrame(resolve))
    }
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && results.length) {
      e.preventDefault()
      active = (active + 1) % results.length
    } else if (e.key === 'ArrowUp' && results.length) {
      e.preventDefault()
      active = (active - 1 + results.length) % results.length
    } else if (e.key === 'Enter') {
      e.preventDefault()
      void go(results[active])
    } else if (e.key === 'Escape' && query) {
      // First Escape clears the search; the next one closes Settings.
      e.preventDefault()
      e.stopPropagation()
      query = ''
    }
  }
  const pct = (level: number) => `${Math.round(level * 100)}%`
  const zoomHint = $derived(
    zoom.applied < zoom.chosen
      ? `Set to ${pct(zoom.chosen)}; ${pct(zoom.applied)} is the most that fits this window.`
      : 'Ctrl + and Ctrl − (or Ctrl + mouse wheel). Ctrl 0 resets.',
  )
</script>

{#snippet row(title: string, hint: string)}
  <div class="min-w-0 flex-1">
    <div class="text-[13px] font-medium">{title}</div>
    {#if hint}<div class="mt-0.5 text-xs text-muted">{hint}</div>{/if}
  </div>
{/snippet}

<Modal title="Settings" {onclose} width={880} height={640} bare>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="flex min-h-0 flex-1"
    onkeydown={(e) => {
      // Ctrl+F searches the settings (not the task list behind them).
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault()
        e.stopPropagation()
        searchEl?.focus()
        searchEl?.select()
      }
    }}
  >
    <div class="flex w-[200px] shrink-0 flex-col border-r border-border max-[900px]:w-[168px]">
    <div class="p-2 pb-1">
      <label class="search">
        <Search size={13} class="shrink-0" />
        <input bind:this={searchEl} bind:value={query} type="search" placeholder="Search settings" aria-label="Search settings" onkeydown={onSearchKey} />
      </label>
    </div>
    {#if query.trim()}
      <div class="flex flex-col gap-0.5 overflow-y-auto p-2 pt-1" role="listbox" aria-label="Matching settings">
        {#each results as r, i (r.anchor)}
          <button role="option" aria-selected={i === active} class="result" onclick={() => void go(r)} onmouseenter={() => (active = i)}>
            <span class="truncate text-[13px]">{r.label}</span>
            <span class="truncate text-[11px] text-subtle">{pageLabel(r.page)}</span>
          </button>
        {:else}
          <p class="px-2 py-1 text-xs text-muted">No settings match “{query.trim()}”.</p>
        {/each}
      </div>
    {:else}
    <div class="flex flex-col gap-0.5 overflow-y-auto p-2 pt-1" role="tablist" aria-orientation="vertical" aria-label="Settings categories">
      {#each PAGES as p (p.id)}
        <button
          role="tab"
          id="settings-tab-{p.id}"
          aria-selected={page === p.id}
          aria-controls="settings-panel"
          tabindex={page === p.id ? 0 : -1}
          class="tab"
          onclick={() => (page = p.id)}
          onkeydown={(e) => {
            const i = PAGES.findIndex((x) => x.id === page)
            const next = e.key === 'ArrowDown' ? PAGES[(i + 1) % PAGES.length] : e.key === 'ArrowUp' ? PAGES[(i - 1 + PAGES.length) % PAGES.length] : null
            if (next) {
              e.preventDefault()
              page = next.id
              queueMicrotask(() => document.getElementById(`settings-tab-${next.id}`)?.focus())
            }
          }}
        >
          <p.icon size={15} />
          <span class="truncate">{p.label}</span>
        </button>
      {/each}
    </div>
    {/if}
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5" role="tabpanel" id="settings-panel" aria-labelledby="settings-tab-{page}">
      <div class="mb-4 flex items-center gap-3">
        <h3 class="font-display text-[17px] font-semibold">{current.label}</h3>
        {#if savedAt}
          {#key savedAt}
            <span class="saved flex items-center gap-1 text-xs text-muted" role="status"><Check size={13} /> Saved</span>
          {/key}
        {/if}
      </div>

      {#if page === 'general'}
        <section class="card">
          <h4 class="card-title">Windows</h4>
          {#if appSettings}
            <div class="set-row" data-setting="autostart">
              {@render row('Start with Windows', 'Opens minimized in the tray when you sign in.')}
              <Switch label="Start with Windows" checked={appSettings.autostart} onchange={(v) => void setApp({ autostart: v })} />
            </div>
            <div class="set-row" data-setting="tray">
              {@render row('Keep running in the tray', 'Closing the window keeps Taskboard in the tray, so reminders still arrive.')}
              <Switch label="Keep running in the tray" checked={appSettings.close_to_tray} onchange={(v) => void setApp({ close_to_tray: v })} />
            </div>
          {/if}
        </section>
        <section class="card">
          <h4 class="card-title">Keyboard</h4>
          <div class="set-row" data-setting="quick-add-shortcut">
            {@render row('Quick add from anywhere', 'Works even when Taskboard is in the background.')}
            <kbd class="kbd">Ctrl + Shift + Space</kbd>
          </div>
          <div class="set-row" data-setting="palette">
            {@render row('Search and commands', 'Jump to any section, group, project or task, or run any action.')}
            <kbd class="kbd">Ctrl + K</kbd>
          </div>
          <div class="set-row" data-setting="shortcuts">
            {@render row('All keyboard shortcuts', '')}
            <button class="btn !h-7 text-xs" onclick={() => void openLink(SHORTCUTS_URL)}><ExternalLink size={13} /> View list</button>
          </div>
        </section>
      {:else if page === 'appearance'}
        <section class="card">
          <div class="set-row" data-setting="theme">
            {@render row('Theme', 'System follows your Windows setting.')}
            <div class="flex rounded-md border border-border bg-surface-1 p-0.5" role="radiogroup" aria-label="Theme">
              {#each THEMES as t (t.v)}
                <button role="radio" aria-checked={theme.pref === t.v} class="seg" onclick={() => { setThemePref(t.v); saved() }}>{t.l}</button>
              {/each}
            </div>
          </div>
          <div class="set-row" data-setting="accent">
            {@render row('Accent color', 'Used for selection, buttons and highlights.')}
            <div class="flex max-w-[186px] flex-wrap justify-end gap-1.5" role="radiogroup" aria-label="Accent color">
              {#each ACCENTS as a (a.hue)}
                <button role="radio" aria-checked={theme.accentHue === a.hue} aria-label={a.name} title={a.name} class="swatch" style="--s: oklch(0.6 0.17 {a.hue})" onclick={() => { setAccentHue(a.hue); saved() }}></button>
              {/each}
            </div>
          </div>
          <div class="set-row" data-setting="zoom">
            {@render row('Zoom', zoomHint)}
            <div class="flex items-center gap-1" role="group" aria-label="Zoom">
              <button class="btn !h-7 !w-7 !px-0" aria-label="Zoom out" title="Zoom out (Ctrl −)" disabled={zoom.chosen <= LEVELS[0]} onclick={() => { zoomOut(); saved() }}><Minus size={14} /></button>
              <span class="tabular w-12 text-center text-[13px]">{pct(zoom.applied)}</span>
              <button class="btn !h-7 !w-7 !px-0" aria-label="Zoom in" title="Zoom in (Ctrl +)" disabled={zoom.chosen >= LEVELS[LEVELS.length - 1]} onclick={() => { zoomIn(); saved() }}><Plus size={14} /></button>
              <button class="btn !h-7 text-xs" disabled={zoom.chosen === 1} onclick={() => { resetZoom(); saved() }}>Reset</button>
            </div>
          </div>
        </section>
      {:else if page === 'sidebar'}
        <SidebarSettings onchange={saved} />
      {:else if page === 'sections'}
        <SectionsSettings onchange={saved} />
      {:else if page === 'views'}
        <ViewsSettings onchange={saved} />
      {:else if page === 'columns'}
        <BoardColumnsSettings onchange={saved} />
      {:else if page === 'updates'}
        <UpdatesSettings />
      {:else if page === 'calendar'}
        <section class="card">
          <div class="set-row" data-setting="week-start">
            {@render row('Week starts on', 'For the week and month views.')}
            <select class="field !h-8 !w-[140px]" aria-label="Week starts on" value={cal.weekStart} onchange={(e) => setWeekStart(Number(e.currentTarget.value))}>
              {#each WEEK_STARTS as w (w.v)}<option value={w.v}>{w.l}</option>{/each}
            </select>
          </div>
          <div class="set-row" data-setting="working-hours">
            {@render row('Working hours and days off', 'Used for daily capacity and the meeting-slot finder.')}
            <button class="btn !h-7 text-xs" onclick={() => { onclose(); cal.workingHoursOpen = true }}>Edit…</button>
          </div>
        </section>
      {:else if page === 'reminders'}
        {#if notify}
          <section class="card">
            <div class="set-row" data-setting="reminders">
              {@render row('Show reminders', 'Windows notifications for meetings and timed tasks.')}
              <Switch label="Show reminders" checked={notify.enabled} onchange={(v) => setNotify({ enabled: v })} />
            </div>
            <div class="set-row" data-setting="meeting-reminder" class:dim={!notify.enabled}>
              {@render row('Meetings', 'Minutes before the meeting starts.')}
              <input type="number" class="field !h-8 !w-[90px]" min="0" max="1440" aria-label="Meeting reminder, minutes before" value={notify.meeting_lead_min} disabled={!notify.enabled} oninput={(e) => setNotify({ meeting_lead_min: minutesValue(e.currentTarget.value) }, 500)} />
            </div>
            <div class="set-row" data-setting="task-reminder" class:dim={!notify.enabled}>
              {@render row('Timed tasks', 'Minutes before the due time (0 = at the due time).')}
              <input type="number" class="field !h-8 !w-[90px]" min="0" max="1440" aria-label="Task reminder, minutes before" value={notify.task_lead_min} disabled={!notify.enabled} oninput={(e) => setNotify({ task_lead_min: minutesValue(e.currentTarget.value) }, 500)} />
            </div>
            <div class="set-row" data-setting="digest" class:dim={!notify.enabled}>
              {@render row('Morning summary', "A notification with today's meetings and due tasks.")}
              <div class="flex items-center gap-2">
                <input type="time" class="field !h-8 !w-[116px]" aria-label="Morning summary time" value={notify.digest_time} disabled={!notify.enabled || !notify.digest} onchange={(e) => setNotify({ digest_time: e.currentTarget.value || '09:00' })} />
                <Switch label="Morning summary" checked={notify.digest} disabled={!notify.enabled} onchange={(v) => setNotify({ digest: v })} />
              </div>
            </div>
          </section>
          <section class="card">
            <div class="set-row" data-setting="test-notification">
              {@render row('Test', 'Check that Windows shows Taskboard notifications.')}
              <button class="btn !h-7 text-xs" onclick={() => void ipc.testNotification().catch((err) => toast(errorMessage(err), { kind: 'error' }))}>Send a test notification</button>
            </div>
          </section>
        {/if}
      {:else if page === 'data'}
        <p class="mb-3 text-xs text-muted">Everything stays on this computer. A backup is made automatically every day, and before every upgrade, import and restore.</p>
        <section class="card">
          <h4 class="card-title">Backups</h4>
          <div class="set-row" data-setting="backups">
            {@render row('Back up now', 'Kept: 7 daily, 4 weekly and 12 monthly backups.')}
            <div class="flex flex-wrap justify-end gap-1.5">
              <button class="btn !h-7 text-xs" onclick={() => void backupNow()}><Archive size={13} /> Back up now</button>
              <button class="btn !h-7 text-xs" onclick={() => void ipc.openBackupsFolder().catch((err) => toast(errorMessage(err), { kind: 'error' }))}><FolderOpen size={13} /> Open folder</button>
            </div>
          </div>
          {#if backups.length}
            <ul class="backups" aria-label="Backups">
              {#each backups.slice(0, 12) as b (b.path)}
                <li>
                  <span class="tabular w-40 shrink-0">{when.format(b.created_at)}</span>
                  <span class="flex-1 truncate text-muted">{KIND[b.kind] ?? b.kind}</span>
                  <span class="tabular text-subtle">{size(b.size_bytes)}</span>
                  <button class="btn !h-6 !px-2 text-xs {confirmRestore === b.path ? 'btn-danger' : ''}" onclick={() => void restore(b)}>
                    <RotateCcw size={12} /> {confirmRestore === b.path ? 'Click to confirm' : 'Restore'}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
        <section class="card">
          <h4 class="card-title">Export</h4>
          <div class="set-row" data-setting="export">
            {@render row('Save a copy of your data', 'JSON has everything and can be imported back.')}
            <div class="flex flex-wrap justify-end gap-1.5">
              <button class="btn !h-7 text-xs" onclick={() => void exportTo('json')}><Download size={13} /> Everything (JSON)</button>
              <button class="btn !h-7 text-xs" onclick={() => void exportTo('csv')}>Tasks (CSV)</button>
              <button class="btn !h-7 text-xs" onclick={() => void exportTo('ics')}>Calendar (ICS)</button>
            </div>
          </div>
        </section>
        <section class="card">
          <h4 class="card-title">Import</h4>
          <div class="set-row" data-setting="import">
            {@render row('Bring in data', 'From a Taskboard export, a Todoist / Trello / Notion CSV, or a calendar file.')}
            <div class="flex flex-wrap justify-end gap-1.5">
              <button class="btn !h-7 text-xs" onclick={() => void importFrom('json')}><Upload size={13} /> Taskboard JSON</button>
              <button class="btn !h-7 text-xs" title="Todoist, Trello, Notion or any CSV with a title column" onclick={() => void importFrom('csv')}>CSV</button>
              <button class="btn !h-7 text-xs" title="Calendar export from Outlook, Google Calendar…" onclick={() => void importFrom('ics')}>Calendar (ICS)</button>
            </div>
          </div>
        </section>
        <ArchivedList />
        <section class="card">
          <div class="set-row" data-setting="sample">
            {@render row('Sample workspace', 'A small example client in a Clients section, with projects, tasks and meetings to explore.')}
            <button class="btn !h-7 text-xs" onclick={() => void toggleSample()}>{hasSample ? 'Remove' : 'Add'}</button>
          </div>
        </section>
      {:else if page === 'ai'}
        <AiSettings settings={appSettings} onchange={(patch) => void setApp(patch)} />
      {:else if page === 'about'}
        <section class="card" data-setting="about">
          <div class="flex items-center gap-4 p-4">
            <img src="/favicon.svg" alt="" class="size-14 shrink-0" />
            <div class="min-w-0 flex-1">
              <div class="font-display text-[17px] font-semibold">Taskboard</div>
              <div class="flex flex-wrap items-center gap-x-2 text-[13px] text-muted">
                Version {version || '…'}
                <button class="link text-xs" onclick={() => (page = 'updates')}>
                  {updates.phase === 'available' && updates.info ? `Version ${updates.info.version} is available` : updates.phase === 'none' ? 'Up to date' : 'Check for updates'}
                </button>
              </div>
              <div class="mt-0.5 text-xs text-muted">Free for personal use; commercial use needs permission.</div>
            </div>
          </div>
          <div class="flex flex-wrap gap-1.5 border-t border-border px-4 py-3">
            <button class="btn !h-7 text-xs" onclick={() => void openLink(CHANGELOG_URL)}>What's new</button>
            <button class="btn !h-7 text-xs" onclick={() => void openLink(RELEASES_URL)}>All releases</button>
            <button class="btn !h-7 text-xs" onclick={() => void openLink(REPO_URL)}>Source code</button>
            <button class="btn !h-7 text-xs" onclick={() => void openLink(LICENSE_URL)}>License</button>
          </div>
        </section>

        <section class="card">
          <h4 class="card-title">Developer</h4>
          <div class="px-4 pb-1">
            <div class="text-[13px] font-semibold">{DEVELOPER.name}</div>
            <div class="text-xs text-muted">Designed and built Taskboard. For commercial licenses, support or custom work, get in touch.</div>
          </div>
          <ul class="links">
            <li>
              <Mail size={14} class="shrink-0 text-muted" />
              <span class="label">Email</span>
              <button class="link" onclick={() => void openLink(`mailto:${DEVELOPER.email}`)}>{DEVELOPER.email}</button>
            </li>
            <li>
              <svg viewBox="0 0 24 24" class="size-3.5 shrink-0 text-muted" aria-hidden="true"><path fill="currentColor" d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.1.79-.25.79-.56v-2c-3.2.7-3.87-1.37-3.87-1.37-.52-1.33-1.28-1.69-1.28-1.69-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.18 1.76 1.18 1.03 1.76 2.69 1.25 3.35.96.1-.75.4-1.25.73-1.54-2.55-.29-5.24-1.28-5.24-5.68 0-1.25.45-2.28 1.18-3.08-.12-.29-.51-1.46.11-3.04 0 0 .97-.31 3.16 1.18a10.9 10.9 0 0 1 5.76 0c2.19-1.49 3.16-1.18 3.16-1.18.62 1.58.23 2.75.11 3.04.74.8 1.18 1.83 1.18 3.08 0 4.41-2.69 5.38-5.25 5.67.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5Z"/></svg>
              <span class="label">GitHub</span>
              <button class="link" onclick={() => void openLink(DEVELOPER.github.url)}>{DEVELOPER.github.label}</button>
            </li>
            {#each DEVELOPER.sites as site (site.url)}
              <li>
                <Globe size={14} class="shrink-0 text-muted" />
                <span class="label">{site.main ? 'Website' : ''}</span>
                <button class="link" onclick={() => void openLink(site.url)}>{site.label}</button>
              </li>
            {/each}
          </ul>
        </section>

        {#if dbInfo}
          <section class="card">
            <h4 class="card-title">Technical details</h4>
            <div class="px-4 pb-3 text-xs text-muted">
              Database: SQLite {dbInfo.sqlite_version} · {dbInfo.journal_mode.toUpperCase()} · schema v{dbInfo.schema_version} · full-text search {dbInfo.fts5 ? 'on' : 'off'}.
              Built with Tauri, Svelte and Rust.
            </div>
          </section>
        {/if}
      {/if}
    </div>
  </div>
</Modal>

<style>
  .tab {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    height: 34px;
    padding-inline: 0.65rem;
    border-radius: 6px;
    color: var(--text-muted);
    text-align: left;
    flex-shrink: 0;
  }
  .tab:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tab[aria-selected='true'] {
    background: var(--accent-soft);
    color: var(--accent-soft-text);
    font-weight: 500;
  }
  .card {
    margin-bottom: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-1);
  }
  .card-title {
    padding: 12px 16px 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-subtle);
  }
  .set-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    min-height: 56px;
    padding: 10px 16px;
  }
  .set-row + .set-row {
    border-top: 1px solid var(--border);
  }
  .set-row.dim {
    opacity: 0.55;
  }
  .seg {
    height: 26px;
    padding-inline: 0.75rem;
    border-radius: 5px;
    color: var(--text-muted);
  }
  .seg[aria-checked='true'] {
    background: var(--surface-3);
    color: var(--text);
  }
  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 99px;
    background: var(--s);
  }
  .swatch[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface-1),
      0 0 0 4px var(--s);
  }
  .kbd {
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-2);
    font-family: inherit;
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .backups {
    margin: 0 16px 12px;
    max-height: 176px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .backups li {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding-inline: 10px;
    font-size: 12px;
  }
  .backups li + li {
    border-top: 1px solid var(--border);
  }
  .links {
    padding: 4px 16px 12px;
  }
  .links li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 30px;
    font-size: 13px;
  }
  .links .label {
    width: 64px;
    flex-shrink: 0;
    font-size: 12px;
    color: var(--text-muted);
  }
  .link {
    min-height: 24px;
    color: var(--accent-soft-text);
    text-align: left;
    text-underline-offset: 2px;
    overflow-wrap: anywhere;
  }
  .link:hover {
    text-decoration: underline;
  }
  .saved {
    animation: saved 1.6s var(--ease-out) forwards;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding-inline: 8px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    min-width: 0;
    flex: 1;
    background: transparent;
    font-size: 13px;
    color: var(--text);
    outline: none;
  }
  .result {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
    padding: 5px 8px;
    border-radius: 6px;
    text-align: left;
  }
  .result[aria-selected='true'] {
    background: var(--accent-soft);
  }
  /* A setting found by search is pointed out briefly. */
  :global(#settings-panel .flash) {
    animation: settings-flash 1.6s var(--ease-out);
    border-radius: var(--radius);
  }
  @keyframes -global-settings-flash {
    0%,
    35% {
      box-shadow: 0 0 0 2px var(--accent);
      background-color: var(--accent-soft);
    }
    100% {
      box-shadow: 0 0 0 2px transparent;
    }
  }
  @keyframes saved {
    0%,
    70% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
</style>
