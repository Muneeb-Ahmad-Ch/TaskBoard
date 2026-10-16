<script lang="ts">
  import { CalendarRange, Eye, EyeOff, KanbanSquare, Rows3, Search, X } from '@lucide/svelte'
  import Board from './components/Board.svelte'
  import Calendar from './components/Calendar.svelte'
  import ClientDialog from './components/ClientDialog.svelte'
  import CommandPalette from './components/CommandPalette.svelte'
  import ProjectDialog from './components/ProjectDialog.svelte'
  import QuickAdd from './components/QuickAdd.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import StatusBar from './components/StatusBar.svelte'
  import TaskPanel from './components/TaskPanel.svelte'
  import TaskTable from './components/TaskTable.svelte'
  import ThemeControls from './components/ThemeControls.svelte'
  import Progress from './components/Progress.svelte'
  import Toasts from './components/Toasts.svelte'
  import DashboardView from './components/DashboardView.svelte'
  import ReminderBanner from './components/ReminderBanner.svelte'
  import Welcome from './components/Welcome.svelte'
  import SettingsDialog from './components/SettingsDialog.svelte'
  import { isTyping, swatch } from './lib/format'
  import { app, redo, scopeTitle, undo, type View } from './lib/store.svelte'

  const views: { id: View; label: string; key: string; icon: typeof Rows3; milestone: string }[] = [
    { id: 'table', label: 'Table', key: '1', icon: Rows3, milestone: 'M2' },
    { id: 'board', label: 'Board', key: '2', icon: KanbanSquare, milestone: 'M3' },
    { id: 'calendar', label: 'Calendar', key: '3', icon: CalendarRange, milestone: 'M4' },
  ]
  const current = $derived(views.find((v) => v.id === app.view) ?? views[0])

  const subtitle = $derived.by(() => {
    const s = app.scope
    if (s.kind === 'project') {
      const p = app.projects.find((x) => x.id === s.id)
      return p ? { text: p.client_name ?? 'Personal', color: p.color } : null
    }
    if (s.kind === 'client') return { text: 'Client', color: app.clients.find((c) => c.id === s.id)?.color ?? 'gray' }
    return null
  })
  /** Done / total top-level tasks (cancelled excluded) for a project or client scope. */
  const scopeProgress = $derived.by(() => {
    const s = app.scope
    const list =
      s.kind === 'project'
        ? app.projects.filter((p) => p.id === s.id)
        : s.kind === 'client'
          ? app.projects.filter((p) => p.client_id === s.id)
          : []
    const total = list.reduce((n, p) => n + p.total_tasks, 0)
    const done = list.reduce((n, p) => n + p.done_tasks, 0)
    return total ? { done, total, pct: (done / total) * 100 } : null
  })
  const canHideDone = $derived(
    app.view === 'table' && app.scope.kind !== 'today' && app.scope.kind !== 'upcoming',
  )

  let searchInput: HTMLInputElement | undefined = $state()

  // First run: no clients, no projects with tasks, nothing in the Inbox.
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

  function onKeydown(e: KeyboardEvent) {
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
    if (ctrl && e.key.toLowerCase() === 'f') {
      e.preventDefault()
      searchInput?.focus()
      return
    }
    if (typing || ctrl || e.altKey || app.paletteOpen) return
    if (e.key === 'n') {
      e.preventDefault()
      app.quickAddFocus++
    } else if (e.key === '/') {
      e.preventDefault()
      searchInput?.focus()
    } else {
      const match = views.find((v) => v.key === e.key)
      if (match) app.view = match.id
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full">
  <Sidebar />

  <main class="panel flex min-w-0 flex-1 flex-col bg-surface-1">
    {#if app.integrityError}
      <div class="flex items-center gap-3 border-b border-danger bg-[color-mix(in_oklch,var(--danger)_12%,transparent)] px-6 py-2 text-[13px]" role="alert">
        <span class="flex-1"><strong>Taskboard found a problem in its database file.</strong> Your latest backups are safe; restoring one is recommended.</span>
        <button class="btn !h-7 text-xs" onclick={() => (app.settingsOpen = true)}>Open backups</button>
        <button class="btn !h-7 text-xs" onclick={() => (app.integrityError = null)}>Dismiss</button>
      </div>
    {/if}
    <header class="flex h-14 shrink-0 items-center gap-3 px-6">
      <div class="flex min-w-0 flex-1 items-baseline gap-2.5">
        <h1 class="font-display truncate text-lg font-semibold tracking-tight">{app.view === 'dashboard' ? 'Dashboard' : scopeTitle()}</h1>
        {#if subtitle}
          <span class="flex shrink-0 items-center gap-1.5 text-xs text-muted">
            <span class="size-2 rounded-full" style="background: {swatch(subtitle.color)}"></span>{subtitle.text}
          </span>
        {/if}
        {#if scopeProgress}
          <span class="flex shrink-0 items-center gap-2 self-center text-xs text-muted" title="{scopeProgress.done} of {scopeProgress.total} tasks done">
            <Progress value={scopeProgress.pct} width={72} label />
            <span class="tabular text-subtle">{scopeProgress.done}/{scopeProgress.total}</span>
          </span>
        {/if}
      </div>

      <label class="search flex h-7 w-56 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2 text-muted focus-within:border-accent">
        <Search size={13} />
        <input bind:this={searchInput} bind:value={app.search} class="min-w-0 flex-1 bg-transparent text-text outline-none placeholder:text-subtle" placeholder="Filter tasks  /" aria-label="Filter tasks" onkeydown={(e) => e.key === 'Escape' && ((app.search = ''), e.currentTarget.blur())} />
        {#if app.search}<button aria-label="Clear filter" onclick={() => (app.search = '')}><X size={13} /></button>{/if}
      </label>

      {#if canHideDone}
        <button
          class="flex h-7 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2.5 text-muted hover:text-text"
          aria-pressed={app.showDone}
          title={app.showDone ? 'Hide completed' : 'Show completed'}
          onclick={() => (app.showDone = !app.showDone)}
        >
          {#if app.showDone}<Eye size={14} />{:else}<EyeOff size={14} />{/if} Done
        </button>
      {/if}

      <div class="flex rounded-md border border-border bg-surface-2 p-0.5" role="tablist" aria-label="View">
        {#each views as v (v.id)}
          <button role="tab" aria-selected={app.view === v.id} title="{v.label} ({v.key})" class="view-tab" onclick={() => (app.view = v.id)}>
            <v.icon size={14} />
            {v.label}
          </button>
        {/each}
      </div>
      <ThemeControls />
    </header>

    <div class="flex min-h-0 flex-1">
      <div class="flex min-w-0 flex-1 flex-col gap-3 px-6 pb-5" role="tabpanel" aria-label={current.label}>
        {#if app.view === 'table'}
          <QuickAdd />
          {#if isEmpty && !welcomeDismissed}
            <Welcome ondismiss={dismissWelcome} />
          {:else}
            <TaskTable />
          {/if}
        {:else if app.view === 'board'}
          <Board />
        {:else if app.view === 'calendar'}
          <Calendar />
        {:else if app.view === 'dashboard'}
          <DashboardView />
        {:else}
          <div class="grid flex-1 place-items-center rounded-[var(--radius)] border border-dashed border-border-strong">
            <div class="text-center">
              <current.icon size={28} class="mx-auto mb-2 text-subtle" />
              <p class="font-medium">{current.label} view</p>
              <p class="text-muted">Arrives in milestone {current.milestone}.</p>
            </div>
          </div>
        {/if}
      </div>
      <TaskPanel />
    </div>

    <StatusBar />
  </main>
</div>

{#if app.paletteOpen}<CommandPalette />{/if}
{#if app.settingsOpen}<SettingsDialog onclose={() => (app.settingsOpen = false)} />{/if}
<ReminderBanner />
{#if app.clientDialog}
  {#key app.clientDialog}<ClientDialog id={app.clientDialog} onclose={() => (app.clientDialog = null)} />{/key}
{/if}
{#if app.projectDialog}
  {#key app.projectDialog}<ProjectDialog id={app.projectDialog} onclose={() => (app.projectDialog = null)} />{/key}
{/if}
<Toasts />

<style>
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
