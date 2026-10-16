<script lang="ts">
  import { Bookmark, ChevronRight, Inbox, LayoutDashboard, Layers, MoreHorizontal, Plus, Search, Settings, Star, CalendarDays, User } from '@lucide/svelte'
  import type { Client } from '../lib/bindings/Client'
  import type { Id } from '../lib/bindings/Id'
  import type { ProjectSummary } from '../lib/bindings/ProjectSummary'
  import type { SavedView } from '../lib/bindings/SavedView'
  import { swatch } from '../lib/format'
  import { ipc } from '../lib/ipc'
  import { act, activeViewChanged, app, applySavedView, projectsOf, saveCurrentView, setScope, updateActiveView, type Scope } from '../lib/store.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'

  const COLLAPSE_KEY = 'taskboard.collapsedClients'
  function loadCollapsed(): Record<string, boolean> {
    try {
      return JSON.parse(localStorage.getItem(COLLAPSE_KEY) ?? '{}')
    } catch {
      return {}
    }
  }
  let collapsed = $state<Record<string, boolean>>(loadCollapsed())
  function toggle(id: Id) {
    collapsed[id] = !collapsed[id]
    try {
      localStorage.setItem(COLLAPSE_KEY, JSON.stringify(collapsed))
    } catch {
      // per-device convenience only
    }
  }

  /** Inline editor: adding a client/project, or renaming one. */
  type Editing =
    | { kind: 'new-client' }
    | { kind: 'new-project'; clientId: Id | null }
    | { kind: 'rename-client'; id: Id; value: string }
    | { kind: 'rename-project'; id: Id; value: string }
    | { kind: 'new-view' }
    | { kind: 'rename-view'; id: Id; value: string }
  let editing = $state<Editing | null>(null)
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)

  const personal = $derived(projectsOf(null))

  function isActive(scope: Scope): boolean {
    if (app.view === 'dashboard' || app.activeViewId) return false
    const s = app.scope
    if (s.kind !== scope.kind) return false
    return !('id' in scope) || ('id' in s && s.id === scope.id)
  }

  async function commit(value: string) {
    const name = value.trim()
    const e = editing
    editing = null
    if (!e || !name) return
    if (e.kind === 'new-client') {
      const c = await act(() => ipc.createClient({ name }))
      if (c) setScope({ kind: 'client', id: c.id })
    } else if (e.kind === 'new-project') {
      const p = await act(() => ipc.createProject({ name, client_id: e.clientId ?? undefined }))
      if (p) {
        if (e.clientId) collapsed[e.clientId] = false
        setScope({ kind: 'project', id: p.id })
      }
    } else if (e.kind === 'rename-client' && name !== e.value) {
      await act(() => ipc.updateClient(e.id, { name }))
    } else if (e.kind === 'rename-project' && name !== e.value) {
      await act(() => ipc.updateProject(e.id, { name }))
    } else if (e.kind === 'new-view') {
      await saveCurrentView(name)
    } else if (e.kind === 'rename-view' && name !== e.value) {
      await act(() => ipc.updateSavedView(e.id, { name }))
    }
  }

  function editorKeys(e: KeyboardEvent) {
    if (e.key === 'Enter') void commit((e.target as HTMLInputElement).value)
    if (e.key === 'Escape') editing = null
  }

  function clientMenu(e: MouseEvent, c: Client) {
    e.preventDefault()
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'New project', run: () => (editing = { kind: 'new-project', clientId: c.id }) },
        { label: 'Settings…', hint: 'time zone, hours', run: () => (app.clientDialog = c.id) },
        { label: 'Rename', run: () => (editing = { kind: 'rename-client', id: c.id, value: c.name }) },
        'separator',
        { label: 'Archive client', run: () => void act(() => ipc.setClientArchived(c.id, true), `Archived “${c.name}”`) },
        { label: 'Delete client', danger: true, run: () => void act(() => ipc.deleteClient(c.id), `Deleted “${c.name}”`) },
      ],
    }
  }

  function projectMenu(e: MouseEvent, p: ProjectSummary) {
    e.preventDefault()
    const moveTargets: MenuEntry[] = [
      ...app.clients
        .filter((c) => c.id !== p.client_id)
        .map((c) => ({
          label: `Move to ${c.name}`,
          color: c.color,
          run: () => void act(() => ipc.updateProject(p.id, { client_id: c.id }), `Moved “${p.name}” to ${c.name}`),
        })),
      ...(p.client_id
        ? [{ label: 'Move to Personal', run: () => void act(() => ipc.updateProject(p.id, { client_id: null }), `Moved “${p.name}” to Personal`) }]
        : []),
    ]
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'Settings…', run: () => (app.projectDialog = p.id) },
        { label: 'Rename', run: () => (editing = { kind: 'rename-project', id: p.id, value: p.name }) },
        ...(moveTargets.length ? (['separator', ...moveTargets] as MenuEntry[]) : []),
        'separator',
        { label: 'Archive project', run: () => void act(() => ipc.setProjectArchived(p.id, true), `Archived “${p.name}”`) },
        { label: 'Delete project and its tasks', danger: true, run: () => void act(() => ipc.deleteProject(p.id), `Deleted “${p.name}”`) },
      ],
    }
  }

  function viewMenu(e: MouseEvent, v: SavedView) {
    e.preventDefault()
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        {
          label: 'Update with current filters',
          run: () => {
            // Saves what is on screen into this view, which then becomes the open one.
            app.activeViewId = v.id
            void updateActiveView()
          },
        },
        { label: 'Rename', run: () => (editing = { kind: 'rename-view', id: v.id, value: v.name }) },
        'separator',
        { label: 'Delete view', danger: true, run: () => void act(() => ipc.deleteSavedView(v.id), `Deleted view “${v.name}”`) },
      ],
    }
  }

  function autofocus(node: HTMLInputElement) {
    node.focus()
    node.select()
  }
</script>

{#snippet editor(placeholder: string, value = '')}
  <input
    class="mx-1 my-0.5 h-7 w-[calc(100%-0.5rem)] rounded-md border border-accent bg-surface-1 px-2 text-[13px] outline-none"
    {placeholder}
    {value}
    use:autofocus
    onkeydown={editorKeys}
    onblur={(e) => void commit(e.currentTarget.value)}
  />
{/snippet}

{#snippet projectRow(p: ProjectSummary, indent: boolean)}
  {#if editing?.kind === 'rename-project' && editing.id === p.id}
    {@render editor('Project name', editing.value)}
  {:else}
    <button
      class="nav-item {indent ? 'pl-7' : ''}"
      class:active={isActive({ kind: 'project', id: p.id })}
      onclick={() => setScope({ kind: 'project', id: p.id })}
      oncontextmenu={(e) => projectMenu(e, p)}
      ondblclick={() => (editing = { kind: 'rename-project', id: p.id, value: p.name })}
    >
      <span class="size-2 shrink-0 rounded-full" style="background: {swatch(p.color)}"></span>
      <span class="flex-1 truncate text-left">{p.name}</span>
      {#if p.open_tasks}<span class="tabular text-xs text-subtle">{p.open_tasks}</span>{/if}
    </button>
  {/if}
{/snippet}

<aside class="flex h-full w-[264px] shrink-0 flex-col bg-[var(--sidebar-bg)] px-2.5 pt-3 pb-2" aria-label="Navigation">
  <div class="mb-3 flex items-center gap-2 px-2">
    <div class="grid size-7 place-items-center rounded-lg bg-accent text-on-accent shadow-sm">
      <svg viewBox="0 0 16 16" class="size-4" aria-hidden="true"><path fill="currentColor" d="M3 3h4v4H3zm6 0h4v4H9zM3 9h4v4H3zm6.5 1.8 1.2 1.2 2.3-2.6.9.8-3.2 3.6-2.1-2.1z" /></svg>
    </div>
    <span class="font-display text-[15px] font-semibold tracking-tight">Taskboard</span>
  </div>

  <button
    class="mb-3 flex h-8 items-center gap-2 rounded-md border border-border bg-surface-1 px-2.5 text-muted transition-colors hover:border-border-strong"
    onclick={() => (app.paletteOpen = true)}
  >
    <Search size={14} />
    <span class="min-w-0 flex-1 truncate text-left">Search or run a command</span>
    <kbd class="shrink-0 rounded border border-border px-1 font-sans text-[11px] whitespace-nowrap text-subtle">Ctrl K</kbd>
  </button>

  <nav class="flex flex-col gap-px" aria-label="Views">
    <button class="nav-item" class:active={isActive({ kind: 'today' })} onclick={() => setScope({ kind: 'today' })}>
      <Star size={15} />
      <span class="flex-1 text-left">Today</span>
      {#if app.counts?.overdue}<span class="badge-danger tabular" title="Overdue">{app.counts.overdue}</span>{/if}
      {#if app.counts?.today}<span class="tabular text-xs text-subtle">{app.counts.today}</span>{/if}
    </button>
    <button class="nav-item" class:active={isActive({ kind: 'upcoming' })} onclick={() => setScope({ kind: 'upcoming' })}>
      <CalendarDays size={15} />
      <span class="flex-1 text-left">Upcoming</span>
      {#if app.counts?.upcoming}<span class="tabular text-xs text-subtle">{app.counts.upcoming}</span>{/if}
    </button>
    <button class="nav-item" class:active={isActive({ kind: 'inbox' })} onclick={() => setScope({ kind: 'inbox' })}>
      <Inbox size={15} />
      <span class="flex-1 text-left">Inbox</span>
      {#if app.counts?.inbox}<span class="tabular text-xs text-subtle">{app.counts.inbox}</span>{/if}
    </button>
    <button class="nav-item" class:active={isActive({ kind: 'all' })} onclick={() => setScope({ kind: 'all' })}>
      <Layers size={15} />
      <span class="flex-1 text-left">All work</span>
    </button>
    <button class="nav-item" class:active={app.view === 'dashboard'} onclick={() => (app.view = 'dashboard')}>
      <LayoutDashboard size={15} />
      <span class="flex-1 text-left">Dashboard</span>
    </button>
  </nav>

  <div class="-mx-1 mt-4 flex min-h-0 flex-1 flex-col gap-px overflow-y-auto px-1 pb-2">
    <div class="section">
      <span>Clients</span>
      <button class="icon-btn" aria-label="Add client" title="Add client" onclick={() => (editing = { kind: 'new-client' })}><Plus size={14} /></button>
    </div>
    {#if editing?.kind === 'new-client'}{@render editor('Client name')}{/if}
    {#if !app.clients.length && editing?.kind !== 'new-client'}
      <p class="px-2 py-1 text-xs text-subtle">Add a client to group its projects.</p>
    {/if}

    {#each app.clients as client (client.id)}
      {#if editing?.kind === 'rename-client' && editing.id === client.id}
        {@render editor('Client name', editing.value)}
      {:else}
        <div class="group relative">
          <button
            class="chevron absolute top-[3px] left-0.5 z-[1] grid size-6 place-items-center rounded text-subtle hover:bg-hover"
            aria-label="{collapsed[client.id] ? 'Expand' : 'Collapse'} {client.name}"
            aria-expanded={!collapsed[client.id]}
            onclick={() => toggle(client.id)}
          >
            <ChevronRight size={13} class="transition-transform {collapsed[client.id] ? '' : 'rotate-90'}" />
          </button>
          <button
            class="nav-item w-full pl-6"
            class:active={isActive({ kind: 'client', id: client.id })}
            onclick={() => setScope({ kind: 'client', id: client.id })}
            oncontextmenu={(e) => clientMenu(e, client)}
            ondblclick={() => (editing = { kind: 'rename-client', id: client.id, value: client.name })}
          >

            <span class="grid size-[16px] shrink-0 place-items-center rounded text-[9.5px] font-bold text-white" style="background: {swatch(client.color, 0.6)}">
              {client.name.charAt(0).toUpperCase()}
            </span>
            <span class="flex-1 truncate text-left font-medium">{client.name}</span>
          </button>
          <button
            class="icon-btn absolute top-1 right-1 opacity-0 group-hover:opacity-100 focus:opacity-100"
            aria-label="Client actions"
            onclick={(e) => clientMenu(e, client)}
          ><MoreHorizontal size={14} /></button>
        </div>
      {/if}
      {#if !collapsed[client.id]}
        {#each projectsOf(client.id) as p (p.id)}
          {@render projectRow(p, true)}
        {/each}
        {#if editing?.kind === 'new-project' && editing.clientId === client.id}
          <div class="pl-6">{@render editor('Project name')}</div>
        {/if}
      {/if}
    {/each}

    <div class="section mt-4">
      <span>Personal</span>
      <button class="icon-btn" aria-label="Add personal project" title="Add personal project" onclick={() => (editing = { kind: 'new-project', clientId: null })}><Plus size={14} /></button>
    </div>
    {#each personal as p (p.id)}
      {@render projectRow(p, false)}
    {/each}
    {#if editing?.kind === 'new-project' && editing.clientId === null}{@render editor('Project name')}{/if}

    <div class="section mt-4">
      <span>Saved views</span>
      <button class="icon-btn" aria-label="Save current view" title="Save current filters as a view" onclick={() => (editing = { kind: 'new-view' })}><Plus size={14} /></button>
    </div>
    {#each app.views as v (v.id)}
      {#if editing?.kind === 'rename-view' && editing.id === v.id}
        {@render editor('View name', editing.value)}
      {:else}
        <button
          class="nav-item"
          class:active={app.activeViewId === v.id && app.view !== 'dashboard'}
          onclick={() => applySavedView(v)}
          oncontextmenu={(e) => viewMenu(e, v)}
          ondblclick={() => (editing = { kind: 'rename-view', id: v.id, value: v.name })}
        >
          <Bookmark size={14} class="shrink-0 text-subtle" />
          <span class="flex-1 truncate text-left">{v.name}</span>
          {#if app.activeViewId === v.id && activeViewChanged()}
            <span class="text-xs text-subtle" title="Filters changed since the view was opened">edited</span>
          {/if}
        </button>
      {/if}
    {/each}
    {#if editing?.kind === 'new-view'}{@render editor('Name this view')}{/if}
    {#if !app.views.length && editing?.kind !== 'new-view'}
      <p class="px-2 py-1 text-xs text-subtle">Set filters, then + to save them.</p>
    {/if}
  </div>

  <div class="flex items-center gap-2 border-t border-border px-2 pt-2 text-muted">
    <User size={14} />
    <span class="flex-1 text-xs">Local workspace · no account</span>
    <button class="icon-btn" aria-label="Settings" title="Settings" onclick={() => (app.settingsOpen = true)}><Settings size={14} /></button>
  </div>
</aside>

{#if menu}
  <Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    height: 30px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    color: var(--text);
    transition: background-color 120ms var(--ease-out);
  }
  .nav-item:hover {
    background: var(--hover);
  }
  .nav-item.active {
    background: var(--accent-soft);
    color: var(--accent-soft-text);
  }
  .section {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0.5rem 0.25rem;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-subtle);
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    color: var(--text-subtle);
  }
  .icon-btn:hover {
    background: var(--hover);
    color: var(--text);
  }
  .badge-danger {
    border-radius: 999px;
    padding: 0 6px;
    font-size: 11px;
    line-height: 17px;
    font-weight: 600;
    color: var(--danger);
    background: color-mix(in oklch, var(--danger) 14%, transparent);
  }
</style>
