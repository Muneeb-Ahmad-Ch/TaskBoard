<script lang="ts">
  import { CalendarRange, Filter, KanbanSquare, Plus, Rows3 } from '@lucide/svelte'
  import PreviewTable from './components/PreviewTable.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import StatusBar from './components/StatusBar.svelte'
  import ThemeControls from './components/ThemeControls.svelte'

  type View = 'table' | 'board' | 'calendar'

  const views: { id: View; label: string; key: string; icon: typeof Rows3; milestone: string }[] = [
    { id: 'table', label: 'Table', key: '1', icon: Rows3, milestone: 'M2' },
    { id: 'board', label: 'Board', key: '2', icon: KanbanSquare, milestone: 'M3' },
    { id: 'calendar', label: 'Calendar', key: '3', icon: CalendarRange, milestone: 'M4' },
  ]

  let view = $state<View>('table')
  const current = $derived(views.find((v) => v.id === view) ?? views[0])

  function onKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement
    if (event.ctrlKey || event.altKey || event.metaKey || target.closest('input, textarea, [contenteditable]')) return
    const match = views.find((v) => v.key === event.key)
    if (match) view = match.id
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full">
  <Sidebar />

  <main class="panel flex min-w-0 flex-1 flex-col bg-surface-1">
    <header class="flex h-14 shrink-0 items-center gap-3 px-6">
      <div class="min-w-0 flex-1">
        <h1 class="font-display truncate text-lg font-semibold tracking-tight">All work</h1>
      </div>

      <div class="flex rounded-md border border-border bg-surface-2 p-0.5" role="tablist" aria-label="View">
        {#each views as v (v.id)}
          <button
            role="tab"
            aria-selected={view === v.id}
            title="{v.label} ({v.key})"
            class="view-tab"
            onclick={() => (view = v.id)}
          >
            <v.icon size={14} />
            {v.label}
          </button>
        {/each}
      </div>

      <button class="flex h-7 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2.5 text-muted hover:text-text">
        <Filter size={13} /> Filter
      </button>
      <ThemeControls />
      <button class="flex h-7 items-center gap-1.5 rounded-md bg-accent px-3 font-medium text-on-accent shadow-sm transition-colors hover:bg-accent-hover">
        <Plus size={14} /> New task
      </button>
    </header>

    <div class="min-h-0 flex-1 overflow-auto px-6 pb-6" role="tabpanel" aria-label={current.label}>
      {#if view === 'table'}
        <PreviewTable />
      {:else}
        <div class="grid h-full place-items-center rounded-[var(--radius)] border border-dashed border-border-strong">
          <div class="text-center">
            <current.icon size={28} class="mx-auto mb-2 text-subtle" />
            <p class="font-medium">{current.label} view</p>
            <p class="text-muted">Arrives in milestone {current.milestone}.</p>
          </div>
        </div>
      {/if}
    </div>

    <StatusBar />
  </main>
</div>

<style>
  .panel {
    border-top-left-radius: 10px;
    box-shadow: var(--shadow-panel);
    margin-top: 0;
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
