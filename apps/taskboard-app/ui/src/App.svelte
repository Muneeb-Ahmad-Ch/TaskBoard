<script lang="ts">
  import { CalendarRange, Eye, EyeOff, KanbanSquare, Rows3, Search, X } from '@lucide/svelte'
  import CommandPalette from './components/CommandPalette.svelte'
  import QuickAdd from './components/QuickAdd.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import StatusBar from './components/StatusBar.svelte'
  import TaskPanel from './components/TaskPanel.svelte'
  import TaskTable from './components/TaskTable.svelte'
  import ThemeControls from './components/ThemeControls.svelte'
  import Toasts from './components/Toasts.svelte'
  import { swatch } from './lib/format'
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
  const canHideDone = $derived(app.scope.kind !== 'today' && app.scope.kind !== 'upcoming')

  let searchInput: HTMLInputElement | undefined = $state()

  function onKeydown(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement).closest('input, textarea, [contenteditable]')
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
    <header class="flex h-14 shrink-0 items-center gap-3 px-6">
      <div class="flex min-w-0 flex-1 items-baseline gap-2.5">
        <h1 class="font-display truncate text-lg font-semibold tracking-tight">{scopeTitle()}</h1>
        {#if subtitle}
          <span class="flex shrink-0 items-center gap-1.5 text-xs text-muted">
            <span class="size-2 rounded-full" style="background: {swatch(subtitle.color)}"></span>{subtitle.text}
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
          <TaskTable />
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
