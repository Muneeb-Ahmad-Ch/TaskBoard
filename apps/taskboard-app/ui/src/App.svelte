<script lang="ts">
  import { AlertTriangle, CalendarRange, Eye, EyeOff, KanbanSquare, PanelLeft, Rows3, Search, X } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import Board from './components/Board.svelte'
  import Calendar from './components/Calendar.svelte'
  import ClientDialog from './components/ClientDialog.svelte'
  import CommandPalette from './components/CommandPalette.svelte'
  import ProjectDialog from './components/ProjectDialog.svelte'
  import SectionDialog from './components/SectionDialog.svelte'
  import ShortcutsDialog from './components/ShortcutsDialog.svelte'
  import DeleteSectionDialog from './components/DeleteSectionDialog.svelte'
  import QuickAdd from './components/QuickAdd.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import StatusBar from './components/StatusBar.svelte'
  import TaskPanel from './components/TaskPanel.svelte'
  import TaskTable from './components/TaskTable.svelte'
  import ScopeStats from './components/ScopeStats.svelte'
  import ScheduleStrip from './components/ScheduleStrip.svelte'
  import ListControls from './components/ListControls.svelte'
  import FilterChips from './components/FilterChips.svelte'
  import { errorMessage, inTauri, ipc } from './lib/ipc'
  import { toast } from './lib/toast.svelte'
  import Toasts from './components/Toasts.svelte'
  import DashboardView from './components/DashboardView.svelte'
  import EventDialog from './components/EventDialog.svelte'
  import FindSlotDialog from './components/FindSlotDialog.svelte'
  import MeetingsView from './components/MeetingsView.svelte'
  import RemindersView from './components/RemindersView.svelte'
  import ReminderDialog from './components/ReminderDialog.svelte'
  import WorkingHoursDialog from './components/WorkingHoursDialog.svelte'
  import { cal } from './lib/cal.svelte'
  import { layout } from './lib/layout.svelte'
  import { dnd } from './lib/dnd.svelte'
  import ReminderBanner from './components/ReminderBanner.svelte'
  import Welcome from './components/Welcome.svelte'
  import SettingsDialog from './components/SettingsDialog.svelte'
  import { anyDialogOpen } from './components/Modal.svelte'
  import { isTyping, swatch } from './lib/format'
  import { app, isPage, redo, savePlace, scopeTitle, sectionById, undo, type View } from './lib/store.svelte'

  /** A view, the task panel or a dialog failed to render: log it for troubleshooting. */
  function logFailure(where: string, error: unknown) {
    const message = error instanceof Error ? error.message : errorMessage(error)
    if (inTauri) void ipc.logUiError(`${where}: ${message}`, error instanceof Error ? error.stack : undefined).catch(() => {})
  }

  // A failed view or panel gets another chance when the list or task changes.
  let viewReset: (() => void) | null = null
  let panelReset: (() => void) | null = null
  $effect(() => {
    void JSON.stringify(app.scope)
    untrack(() => {
      viewReset?.()
      viewReset = null
    })
  })
  $effect(() => {
    void app.openTaskId
    untrack(() => {
      panelReset?.()
      panelReset = null
    })
  })

  /** A dialog failed: close the dialogs, say so, and let them open again. */
  function dialogFailed(error: unknown, reset: () => void) {
    logFailure('dialog', error)
    app.paletteOpen = false
    app.settingsOpen = false
    app.shortcutsOpen = false
    app.clientDialog = null
    app.projectDialog = null
    app.sectionDialog = null
    app.deleteSection = null
    app.reminderDialog = false
    cal.draft = null
    cal.workingHoursOpen = false
    cal.findSlotOpen = false
    toast('That window ran into a problem and was closed. Your data is safe.', { kind: 'error' })
    setTimeout(reset, 0)
  }

  const views: { id: View; label: string; key: string; icon: typeof Rows3; milestone: string }[] = [
    { id: 'table', label: 'Table', key: '1', icon: Rows3, milestone: 'M2' },
    { id: 'board', label: 'Board', key: '2', icon: KanbanSquare, milestone: 'M3' },
    { id: 'calendar', label: 'Calendar', key: '3', icon: CalendarRange, milestone: 'M4' },
  ]
  /** View tabs the user chose to show (Settings → Layout); keys 1–3 follow them. */
  const shownViews = $derived(views.filter((v) => layout.tabs[v.id as 'table' | 'board' | 'calendar']))
  const current = $derived(views.find((v) => v.id === app.view) ?? views[0])
  const pageTitle = $derived(app.view === 'dashboard' ? 'Dashboard' : app.view === 'meetings' ? 'Meetings' : app.view === 'reminders' ? 'Reminders' : null)
  // Hiding the tab you are on moves you to the first visible one.
  $effect(() => {
    if (!isPage(app.view) && !shownViews.some((v) => v.id === app.view) && shownViews[0]) app.view = shownViews[0].id
  })

  const subtitle = $derived.by(() => {
    const s = app.scope
    if (s.kind === 'project') {
      const p = app.projects.find((x) => x.id === s.id)
      if (!p) return null
      const text = [sectionById(p.section_id)?.name, p.client_name].filter(Boolean).join(' › ')
      return text ? { text, color: p.color } : null
    }
    if (s.kind === 'client') {
      const c = app.clients.find((x) => x.id === s.id)
      return c ? { text: sectionById(c.section_id)?.name ?? '', color: c.color } : null
    }
    if (s.kind === 'today' && app.counts) {
      // Overdue rows carry a red rail in the table; say how many of each.
      const { overdue, today } = app.counts
      const parts = [overdue ? `${overdue} overdue` : '', today ? `${today} due today` : ''].filter(Boolean)
      return parts.length ? { text: parts.join(' · '), color: overdue ? '10' : 'gray', warn: overdue > 0 } : null
    }
    return null
  })
  const canHideDone = $derived(
    app.view === 'table' && app.scope.kind !== 'today' && app.scope.kind !== 'upcoming',
  )

  let searchInput: HTMLInputElement | undefined = $state()

  // First run: no sections or groups, no projects with tasks, nothing in the Inbox.
  let welcomeDismissed = $state(
    (() => {
      try {
        return localStorage.getItem('taskboard.welcomeDismissed') === '1'
      } catch {
        return false
      }
    })(),
  )
  const isEmpty = $derived(
    app.counts !== null &&
      app.clients.length === 0 &&
      app.sections.length === 0 &&
      app.projects.every((p) => p.total_tasks === 0) &&
      (app.counts?.inbox ?? 0) === 0,
  )
  function dismissWelcome() {
    welcomeDismissed = true
    try {
      localStorage.setItem('taskboard.welcomeDismissed', '1')
    } catch {
      // per-device convenience only
    }
  }

  // Narrow windows: the sidebar becomes a drawer.
  let viewportWidth = $state(window.innerWidth)
  const drawer = $derived(viewportWidth < 1000)
  // Picking anything in the drawer, opening a dialog from it, or widening the
  // window closes it.
  $effect(() => {
    void app.scope
    void app.view
    void app.activeViewId
    void drawer
    void app.settingsOpen
    void app.paletteOpen
    void app.clientDialog
    void app.projectDialog
    void app.sectionDialog
    void app.deleteSection
    void cal.draft
    void app.shortcutsOpen
    void app.reminderDialog
    untrack(() => (app.sidebarOpen = false))
  })

  // Reopen on the same list and view next time.
  $effect(() => {
    void JSON.stringify(app.scope)
    void app.view
    savePlace()
  })

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && app.sidebarOpen && !anyDialogOpen()) {
      app.sidebarOpen = false
      return
    }
    const typing = isTyping(e)
    const ctrl = e.ctrlKey || e.metaKey
    if (ctrl && e.key.toLowerCase() === 'k') {
      e.preventDefault()
      app.paletteOpen = !app.paletteOpen
      return
    }
    if (ctrl && !typing && e.key.toLowerCase() === 'z') {
      e.preventDefault()
      void (e.shiftKey ? redo() : undo())
      return
    }
    if (ctrl && !typing && e.key.toLowerCase() === 'y') {
      e.preventDefault()
      void redo()
      return
    }
    // Inside a dialog, keys belong to the dialog (Settings has its own Ctrl+F).
    if (anyDialogOpen()) return
    if (ctrl && e.key.toLowerCase() === 'f') {
      e.preventDefault()
      searchInput?.focus()
      return
    }
    if (typing || ctrl || e.altKey || app.paletteOpen) return
    if (e.key === 'n') {
      e.preventDefault()
      app.quickAddFocus++
    } else if (e.key === '?') {
      e.preventDefault()
      app.shortcutsOpen = true
    } else if (e.key === '/') {
      e.preventDefault()
      searchInput?.focus()
    } else {
      const match = shownViews[Number(e.key) - 1]
      if (match) app.view = match.id
    }
  }
</script>

<svelte:window bind:innerWidth={viewportWidth} onkeydown={onKeydown} />

<div class="flex h-full">
  {#if !drawer}
    <Sidebar />
  {:else if app.sidebarOpen}
    <button class="fixed inset-0 z-40 bg-black/30" aria-label="Close navigation" tabindex="-1" onclick={() => (app.sidebarOpen = false)}></button>
    <!-- Any navigation click closes the drawer, even on the item already showing. -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="drawer fixed inset-y-0 left-0 z-50 shadow-2xl" onclick={(e) => (e.target as Element).closest('.nav-item') && (app.sidebarOpen = false)}>
      <Sidebar />
    </div>
  {/if}

  <main class="panel flex min-w-0 flex-1 flex-col bg-surface-1">
    {#if app.integrityError}
      <div class="flex items-center gap-3 border-b border-danger bg-[color-mix(in_oklch,var(--danger)_12%,transparent)] px-6 py-2 text-[13px]" role="alert">
        <span class="flex-1"><strong>Taskboard found a problem in its database file.</strong> Your latest backups are safe; restoring one is recommended.</span>
        <button class="btn !h-7 text-xs" onclick={() => { app.settingsSection = 'data'; app.settingsOpen = true }}>Open backups</button>
        <button class="btn !h-7 text-xs" onclick={() => (app.integrityError = null)}>Dismiss</button>
      </div>
    {/if}
    <header class="flex h-14 shrink-0 items-center gap-3 px-6 max-[1000px]:gap-2 max-[1000px]:px-4">
      {#if drawer}
        <button class="grid size-8 shrink-0 place-items-center rounded-md text-muted hover:bg-hover hover:text-text" aria-label="Open navigation" title="Navigation" aria-expanded={app.sidebarOpen} onclick={() => (app.sidebarOpen = true)}>
          <PanelLeft size={17} />
        </button>
      {/if}
      <div class="@container flex min-w-0 flex-1 items-baseline gap-2.5">
        <h1 class="font-display shrink-[0.25] truncate text-lg font-semibold tracking-tight">{pageTitle ?? app.views.find((v) => v.id === app.activeViewId)?.name ?? scopeTitle()}</h1>
        {#if subtitle}
          <span class="flex shrink-0 items-center gap-1.5 text-xs @max-[520px]:hidden {'warn' in subtitle && subtitle.warn ? 'font-medium text-danger' : 'text-muted'}">
            {#if layout.headerDot}<span class="size-2 rounded-full" style="background: {swatch(subtitle.color)}"></span>{/if}{subtitle.text}
          </span>
        {/if}
        {#if !isPage()}
          <!-- Progress, time left, meetings and overdue for the list on screen. -->
          <span class="min-w-0 shrink-[3] overflow-hidden @max-[260px]:hidden">
            {#key JSON.stringify(app.scope)}<ScopeStats scope={app.scope} />{/key}
          </span>
        {/if}
      </div>

      <label class="search flex h-7 w-56 shrink items-center max-[1180px]:w-44 max-[820px]:w-32 gap-1.5 rounded-md border border-border bg-surface-2 px-2 text-muted focus-within:border-accent">
        <Search size={13} />
        <input bind:this={searchInput} bind:value={app.search} class="min-w-0 flex-1 bg-transparent text-text outline-none placeholder:text-subtle" placeholder="Search tasks  /" aria-label="Search tasks" onkeydown={(e) => e.key === 'Escape' && ((app.search = ''), e.currentTarget.blur())} />
        {#if app.search}<button aria-label="Clear search" onclick={() => (app.search = '')}><X size={13} /></button>{/if}
      </label>

      {#if app.view === 'table' || app.view === 'board'}
        <ListControls />
      {/if}

      {#if canHideDone}
        <button
          class="flex h-7 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2.5 text-muted hover:text-text"
          aria-pressed={app.showDone}
          title={app.showDone ? 'Hide completed' : 'Show completed'}
          onclick={() => (app.showDone = !app.showDone)}
        >
          {#if app.showDone}<Eye size={14} />{:else}<EyeOff size={14} />{/if} <span class="max-[1180px]:sr-only">{app.showDone ? 'Hide completed' : 'Show completed'}</span>
        </button>
      {/if}

      <div class="flex rounded-md border border-border bg-surface-2 p-0.5" role="tablist" aria-label="View">
        {#each shownViews as v, i (v.id)}
          <button role="tab" aria-selected={app.view === v.id} title="{v.label} ({i + 1})" class="view-tab" onclick={() => (app.view = v.id)}>
            <v.icon size={14} />
            <span class="max-[1180px]:sr-only">{v.label}</span>
          </button>
        {/each}
      </div>
    </header>

    <div class="relative flex min-h-0 flex-1">
      <div class="flex min-w-0 flex-1 flex-col gap-3 px-6 pb-5 max-[1000px]:px-4" role="tabpanel" aria-label={pageTitle ?? current.label}>
        {#key app.view}
        <svelte:boundary onerror={(e, reset) => { logFailure('view', e); viewReset = reset }} failed={viewFailed}>
        {#if app.view === 'table'}
          <ScheduleStrip />
          {#if layout.quickAdd === 'top'}<QuickAdd placement="top" />{/if}
          <FilterChips />
          {#if isEmpty && !welcomeDismissed}
            <Welcome ondismiss={dismissWelcome} />
          {:else}
            <TaskTable />
          {/if}
          <!-- At the bottom (default) quick add sits under the list, like a chat box. -->
          {#if layout.quickAdd === 'bottom'}<div class="mt-auto"><QuickAdd placement="bottom" /></div>{/if}
        {:else if app.view === 'board'}
          <FilterChips />
          <Board />
        {:else if app.view === 'calendar'}
          <Calendar />
        {:else if app.view === 'dashboard'}
          <DashboardView />
        {:else if app.view === 'meetings'}
          <MeetingsView />
        {:else if app.view === 'reminders'}
          <RemindersView />
        {:else}
          <div class="grid flex-1 place-items-center rounded-[var(--radius)] border border-dashed border-border-strong">
            <div class="text-center">
              <current.icon size={28} class="mx-auto mb-2 text-subtle" />
              <p class="font-medium">{current.label} view</p>
              <p class="text-muted">Arrives in milestone {current.milestone}.</p>
            </div>
          </div>
        {/if}
        </svelte:boundary>
        {/key}
      </div>
      <svelte:boundary onerror={(e, reset) => { logFailure('task panel', e); panelReset = reset }} failed={panelFailed}>
        <TaskPanel />
      </svelte:boundary>
    </div>

    <StatusBar />
  </main>
</div>

<svelte:boundary onerror={dialogFailed}>
{#if app.paletteOpen}<CommandPalette />{/if}
{#if cal.draft}
  {#key cal.draft}
    <EventDialog draft={cal.draft} onclose={() => (cal.draft = null)} />
  {/key}
{/if}
{#if cal.workingHoursOpen}<WorkingHoursDialog onclose={() => (cal.workingHoursOpen = false)} />{/if}
{#if cal.findSlotOpen}<FindSlotDialog onclose={() => (cal.findSlotOpen = false)} />{/if}
{#if app.settingsOpen}<SettingsDialog onclose={() => (app.settingsOpen = false)} />{/if}
{#if app.shortcutsOpen}<ShortcutsDialog onclose={() => (app.shortcutsOpen = false)} />{/if}
{#if app.reminderDialog}<ReminderDialog onclose={() => (app.reminderDialog = false)} />{/if}
{#if dnd.item && !dnd.external}
  <!-- What is being dragged; the sidebar marks where it would land. -->
  <div class="drag-ghost" style="left: {dnd.x + 14}px; top: {dnd.y + 10}px" aria-hidden="true">
    {dnd.item.label}{#if !dnd.target}<span class="text-subtle"> · drop on a highlighted spot</span>{/if}
  </div>
{/if}
{#if app.clientDialog}
  {#key app.clientDialog}<ClientDialog id={app.clientDialog} onclose={() => (app.clientDialog = null)} />{/key}
{/if}
{#if app.projectDialog}
  {#key app.projectDialog}<ProjectDialog id={app.projectDialog} onclose={() => (app.projectDialog = null)} />{/key}
{/if}
{#if app.sectionDialog}
  {#key app.sectionDialog}<SectionDialog id={app.sectionDialog.id} onclose={() => (app.sectionDialog = null)} />{/key}
{/if}
{#if app.deleteSection}
  {#key app.deleteSection}<DeleteSectionDialog id={app.deleteSection} onclose={() => (app.deleteSection = null)} />{/key}
{/if}
</svelte:boundary>
<!-- Its own boundary: if it fails it disappears instead of retrying in a loop. -->
<svelte:boundary onerror={(e) => logFailure('reminder banner', e)}>
  <ReminderBanner />
  {#snippet failed()}{/snippet}
</svelte:boundary>
<Toasts />

{#snippet viewFailed(_error: unknown, reset: () => void)}
  <div class="grid flex-1 place-items-center rounded-[var(--radius)] border border-dashed border-border-strong p-6 text-center" role="alert">
    <div class="max-w-[360px]">
      <AlertTriangle size={28} class="mx-auto mb-2 text-danger" />
      <p class="font-medium">This view ran into a problem</p>
      <p class="text-muted">Your data is safe, and the problem was logged. Try again, or switch to another view.</p>
      <button class="btn mt-3" onclick={reset}>Try again</button>
    </div>
  </div>
{/snippet}

{#snippet panelFailed(_error: unknown, reset: () => void)}
  <aside class="flex w-[320px] shrink-0 flex-col items-center justify-center gap-2 border-l border-border bg-surface-1 p-6 text-center" role="alert">
    <AlertTriangle size={24} class="text-danger" />
    <p class="font-medium">This task couldn't be shown</p>
    <p class="text-xs text-muted">The problem was logged.</p>
    <div class="flex gap-1.5">
      <button class="btn !h-7 text-xs" onclick={reset}>Try again</button>
      <button class="btn !h-7 text-xs" onclick={() => { app.openTaskId = null; reset() }}>Close</button>
    </div>
  </aside>
{/snippet}

<style>
  .drag-ghost {
    position: fixed;
    z-index: 70;
    max-width: 280px;
    padding: 4px 10px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.22);
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
  }
  .panel {
    border-top-left-radius: 10px;
    box-shadow: var(--shadow-panel);
  }
  .view-tab {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    height: 24px;
    padding-inline: 0.6rem;
    border-radius: 5px;
    color: var(--text-muted);
    transition:
      background-color 120ms var(--ease-out),
      color 120ms var(--ease-out);
  }
  .view-tab:hover {
    color: var(--text);
  }
  .view-tab[aria-selected='true'] {
    background: var(--surface-3);
    color: var(--text);
    box-shadow: 0 1px 2px oklch(0 0 0 / 0.08);
  }
</style>
