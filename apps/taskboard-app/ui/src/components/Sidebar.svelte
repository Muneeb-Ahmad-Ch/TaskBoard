<script lang="ts">
  import { untrack } from 'svelte'
  import { Bookmark, ChevronRight, Inbox, LayoutDashboard, Layers, MoreHorizontal, Plus, Search, Settings, Star, CalendarDays, User, Video } from '@lucide/svelte'
  import type { Client } from '../lib/bindings/Client'
  import type { Section } from '../lib/bindings/Section'
  import type { Direction } from '../lib/bindings/Direction'
  import type { Id } from '../lib/bindings/Id'
  import type { ProjectSummary } from '../lib/bindings/ProjectSummary'
  import type { SavedView } from '../lib/bindings/SavedView'
  import { swatch } from '../lib/format'
  import { ipc } from '../lib/ipc'
  import { act, activeViewChanged, app, applySavedView, groupsIn, isPage, itemName, looseProjectsIn, projectsOf, saveCurrentView, sectionById, setScope, updateActiveView, type Scope } from '../lib/store.svelte'
  import { layout, NAV_LABELS, type NavId } from '../lib/layout.svelte'
  import { startOfDay, addDays } from '../lib/cal.svelte'
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
  function setCollapsed(id: Id, value: boolean) {
    if (!!collapsed[id] === value) return
    collapsed[id] = value
    try {
      localStorage.setItem(COLLAPSE_KEY, JSON.stringify(collapsed))
    } catch {
      // per-device convenience only
    }
  }
  const toggle = (id: Id) => setCollapsed(id, !collapsed[id])

  // Opening a project (from search, a dialog, a new project…) expands its
  // client, so the selected row is never hidden inside a collapsed group.
  $effect(() => {
    const s = app.scope
    if (s.kind !== 'project') return
    const project = app.projects.find((p) => p.id === s.id)
    // untrack: collapsing by hand must not re-run this and undo it.
    untrack(() => {
      if (project?.client_id) setCollapsed(project.client_id, false)
      if (project?.section_id) setCollapsed(project.section_id, false)
    })
  })

  /** Inline editor: adding a client/project, or renaming one. */
  type Editing =
    | { kind: 'new-client'; sectionId: Id }
    | { kind: 'new-project'; clientId: Id | null; sectionId: Id }
    | { kind: 'rename-section'; id: Id; value: string }
    | { kind: 'rename-client'; id: Id; value: string }
    | { kind: 'rename-project'; id: Id; value: string }
    | { kind: 'new-view' }
    | { kind: 'rename-view'; id: Id; value: string }
  let editing = $state<Editing | null>(null)
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)

  /** Sections shown in the sidebar (hidden ones live in Settings → Layout). */
  const visibleSections = $derived(app.sections.filter((s) => !layout.hiddenSections.includes(s.id)))
  const cap = (w: string) => w.charAt(0).toUpperCase() + w.slice(1)

  function isActive(scope: Scope): boolean {
    if (isPage(app.view) || app.activeViewId) return false
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
      const c = await act(() => ipc.createClient({ name, section_id: e.sectionId }))
      if (c) setScope({ kind: 'client', id: c.id })
    } else if (e.kind === 'new-project') {
      const p = await act(() => ipc.createProject({ name, client_id: e.clientId ?? undefined, section_id: e.sectionId }))
      if (p) setScope({ kind: 'project', id: p.id })
    } else if (e.kind === 'rename-section' && name !== e.value) {
      await act(() => ipc.updateSection(e.id, { name }))
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

  /** Opens the new-project editor under a group, expanding it so it is visible. */
  function startProject(c: Client) {
    setCollapsed(c.id, false)
    if (c.section_id) setCollapsed(c.section_id, false)
    editing = { kind: 'new-project', clientId: c.id, sectionId: c.section_id ?? '' }
  }

  /** "+" on a section: add a group or a project directly in it. */
  function addMenu(e: MouseEvent, sec: Section) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    setCollapsed(sec.id, false)
    menu = {
      x: r.left,
      y: r.bottom + 4,
      items: [
        { label: `${cap(itemName(sec))}…`, run: () => (editing = { kind: 'new-client', sectionId: sec.id }) },
        { label: 'Project…', run: () => (editing = { kind: 'new-project', clientId: null, sectionId: sec.id }) },
      ],
    }
  }

  function sectionMenu(e: MouseEvent, sec: Section) {
    e.preventDefault()
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'Edit section…', hint: 'name, color', run: () => (app.sectionDialog = { id: sec.id }) },
        { label: 'Rename', run: () => (editing = { kind: 'rename-section', id: sec.id, value: sec.name }) },
        'separator',
        ...orderItems(app.sections, sec.id, (dir) => ipc.moveSection(sec.id, dir)),
        'separator',
        { label: 'Customize sidebar…', run: customize },
        { label: 'Delete section…', danger: true, run: () => (app.deleteSection = sec.id) },
      ],
    }
  }

  function editorKeys(e: KeyboardEvent) {
    if (e.key === 'Enter') void commit((e.target as HTMLInputElement).value)
    if (e.key === 'Escape') editing = null
  }

  function clientMenu(e: MouseEvent, c: Client) {
    e.preventDefault()
    const sec = sectionById(c.section_id)
    const item = itemName(sec)
    const moveTo: MenuEntry[] = app.sections
      .filter((s) => s.id !== c.section_id)
      .map((s) => ({
        label: `Move to ${s.name}`,
        color: s.color,
        run: () => void act(() => ipc.updateClient(c.id, { section_id: s.id }), `Moved “${c.name}” to ${s.name}`),
      }))
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'New project', run: () => startProject(c) },
        { label: 'Details…', hint: 'time zone, hours', run: () => (app.clientDialog = c.id) },
        { label: 'Rename', run: () => (editing = { kind: 'rename-client', id: c.id, value: c.name }) },
        'separator',
        ...orderItems(c.section_id ? groupsIn(c.section_id) : app.clients, c.id, (dir) => ipc.moveClient(c.id, dir)),
        ...(moveTo.length ? (['separator', ...moveTo] as MenuEntry[]) : []),
        'separator',
        { label: `Archive ${item}`, run: () => void act(() => ipc.setClientArchived(c.id, true), `Archived “${c.name}”`) },
        { label: `Delete ${item}`, danger: true, run: () => void act(() => ipc.deleteClient(c.id), `Deleted “${c.name}”`) },
      ],
    }
  }

  function projectMenu(e: MouseEvent, p: ProjectSummary) {
    e.preventDefault()
    const sectionName = (id: Id | null) => sectionById(id)?.name ?? ''
    const moveTargets: MenuEntry[] = [
      ...app.clients
        .filter((c) => c.id !== p.client_id)
        .map((c) => ({
          label: `Move to ${sectionName(c.section_id)} › ${c.name}`,
          color: c.color,
          run: () => void act(() => ipc.updateProject(p.id, { client_id: c.id }), `Moved “${p.name}” to ${c.name}`),
        })),
      ...(p.client_id
        ? [{ label: `Take out of ${p.client_name ?? 'its group'}`, run: () => void act(() => ipc.updateProject(p.id, { client_id: null }), `“${p.name}” is no longer in a group`) }]
        : app.sections
            .filter((s) => s.id !== p.section_id)
            .map((s) => ({
              label: `Move to ${s.name}`,
              color: s.color,
              run: () => void act(() => ipc.updateProject(p.id, { section_id: s.id }), `Moved “${p.name}” to ${s.name}`),
            }))),
    ]
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'Settings…', run: () => (app.projectDialog = p.id) },
        { label: 'Rename', run: () => (editing = { kind: 'rename-project', id: p.id, value: p.name }) },
        'separator',
        ...orderItems(p.client_id ? projectsOf(p.client_id) : p.section_id ? looseProjectsIn(p.section_id) : [], p.id, (dir) => ipc.moveProject(p.id, dir)),
        ...(moveTargets.length ? (['separator', ...moveTargets] as MenuEntry[]) : []),
        'separator',
        { label: 'Archive project', run: () => void act(() => ipc.setProjectArchived(p.id, true), `Archived “${p.name}”`) },
        { label: 'Delete project and its tasks', danger: true, run: () => void act(() => ipc.deleteProject(p.id), `Deleted “${p.name}”`) },
      ],
    }
  }

  /** "Move up" / "Move down" entries, disabled at the ends of the list. */
  function orderItems(list: { id: Id }[], id: Id, move: (dir: Direction) => Promise<boolean>): MenuEntry[] {
    const i = list.findIndex((x) => x.id === id)
    return [
      { label: 'Move up', hint: 'Alt ↑', disabled: i <= 0, run: () => void act(() => move('up')) },
      { label: 'Move down', hint: 'Alt ↓', disabled: i < 0 || i >= list.length - 1, run: () => void act(() => move('down')) },
    ]
  }

  /** Alt+↑ / Alt+↓ on a focused client or project row moves it, keeping focus on it. */
  function reorderKeys(e: KeyboardEvent, move: (dir: Direction) => Promise<boolean>) {
    if (!e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return
    e.preventDefault()
    const row = e.currentTarget as HTMLElement
    void act(() => move(e.key === 'ArrowUp' ? 'up' : 'down')).then(() => requestAnimationFrame(() => row.isConnected && row.focus()))
  }

  // ---------- top navigation (order and visibility from layout) ----------
  const NAV_ICONS = { today: Star, upcoming: CalendarDays, inbox: Inbox, all: Layers, meetings: Video, dashboard: LayoutDashboard }
  const shownNav = $derived(layout.nav.filter((n) => n.visible).map((n) => n.id))

  function navActive(id: NavId): boolean {
    if (id === 'meetings' || id === 'dashboard') return app.view === id
    return isActive({ kind: id })
  }

  function navGo(id: NavId) {
    if (id === 'meetings' || id === 'dashboard') app.view = id
    else setScope({ kind: id })
  }

  /** Meetings still ahead today, for the Meetings badge. */
  let meetingsLeft = $state(0)
  $effect(() => {
    void app.dataVersion
    if (!shownNav.includes('meetings')) return
    const from = startOfDay(new Date())
    let stale = false
    ipc
      .occurrences(from.getTime(), addDays(from, 1).getTime())
      .then((list) => {
        if (!stale) meetingsLeft = list.filter((o) => o.kind === 'meeting' && o.status !== 'cancelled' && o.end_at > Date.now()).length
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  function customize() {
    app.settingsSection = 'layout'
    app.settingsOpen = true
  }

  /** Right-click on a sidebar item or section: show, hide and order live in Settings. */
  function customizeMenu(e: MouseEvent) {
    e.preventDefault()
    menu = { x: e.clientX, y: e.clientY, items: [{ label: 'Customize sidebar…', run: customize }] }
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
      class="nav-item"
      class:in-client={indent}
      class:active={isActive({ kind: 'project', id: p.id })}
      onclick={() => setScope({ kind: 'project', id: p.id })}
      oncontextmenu={(e) => projectMenu(e, p)}
      onkeydown={(e) => reorderKeys(e, (dir) => ipc.moveProject(p.id, dir))}
      ondblclick={() => (editing = { kind: 'rename-project', id: p.id, value: p.name })}
    >
      <span class="size-2 shrink-0 rounded-full" style="background: {swatch(p.color)}"></span>
      <span class="flex-1 truncate text-left">{p.name}</span>
      {#if p.open_tasks}<span class="tabular text-xs text-subtle">{p.open_tasks}</span>{/if}
    </button>
  {/if}
{/snippet}

<aside class="flex h-full w-[264px] shrink-0 flex-col bg-[var(--sidebar-bg)] px-2.5 pt-3 pb-2" aria-label="Navigation">
  <button
    class="mb-3 flex h-8 items-center gap-2 rounded-md border border-border bg-surface-1 px-2.5 text-muted transition-colors hover:border-border-strong"
    onclick={() => (app.paletteOpen = true)}
  >
    <Search size={14} />
    <span class="min-w-0 flex-1 truncate text-left">Search or run a command</span>
    <kbd class="shrink-0 rounded border border-border px-1 font-sans text-[11px] whitespace-nowrap text-subtle">Ctrl K</kbd>
  </button>

  <nav class="flex flex-col gap-px" aria-label="Views">
    {#each shownNav as id (id)}
      {@const Icon = NAV_ICONS[id]}
      <button class="nav-item w-full" class:active={navActive(id)} onclick={() => navGo(id)} oncontextmenu={customizeMenu}>
        <Icon size={15} />
        <span class="flex-1 text-left">{NAV_LABELS[id]}</span>
        <span class="counts flex items-center gap-2">
        {#if id === 'today'}
          {#if app.counts?.overdue}<span class="badge-danger tabular" title="Overdue">{app.counts.overdue}</span>{/if}
          {#if app.counts?.today}<span class="tabular text-xs text-subtle">{app.counts.today}</span>{/if}
        {:else if id === 'upcoming'}
          {#if app.counts?.upcoming}<span class="tabular text-xs text-subtle">{app.counts.upcoming}</span>{/if}
        {:else if id === 'inbox'}
          {#if app.counts?.inbox}<span class="tabular text-xs text-subtle">{app.counts.inbox}</span>{/if}
        {:else if id === 'meetings'}
          {#if meetingsLeft}<span class="tabular text-xs text-subtle" title="Meetings left today">{meetingsLeft}</span>{/if}
        {/if}
        </span>
      </button>
    {/each}
  </nav>

  <div class="-mx-1 mt-4 flex min-h-0 flex-1 flex-col gap-px overflow-y-auto px-1 pb-2">
    {#each visibleSections as sec, si (sec.id)}
      {@const item = itemName(sec)}
      {@const groups = groupsIn(sec.id)}
      {@const loose = looseProjectsIn(sec.id)}
      {#if editing?.kind === 'rename-section' && editing.id === sec.id}
        {@render editor('Section name', editing.value)}
      {:else}
        <div class="section-head group relative" class:mt-4={si > 0}>
          <button
            class="absolute top-[3px] left-0 z-[1] grid size-6 place-items-center rounded text-subtle hover:bg-hover hover:text-text"
            aria-label="{collapsed[sec.id] ? 'Expand' : 'Collapse'} {sec.name}"
            aria-expanded={!collapsed[sec.id]}
            onclick={() => toggle(sec.id)}
          >
            <ChevronRight size={12} class="transition-transform {collapsed[sec.id] ? '' : 'rotate-90'}" />
          </button>
          <button
            class="section-name"
            class:active={isActive({ kind: 'section', id: sec.id })}
            title="Show all work in {sec.name}"
            onclick={() => setScope({ kind: 'section', id: sec.id })}
            oncontextmenu={(e) => sectionMenu(e, sec)}
            onkeydown={(e) => reorderKeys(e, (dir) => ipc.moveSection(sec.id, dir))}
            ondblclick={() => (editing = { kind: 'rename-section', id: sec.id, value: sec.name })}
          >
            <span class="size-1.5 shrink-0 rounded-full" style="background: {swatch(sec.color)}"></span>
            <span class="truncate">{sec.name}</span>
          </button>
          <button class="icon-btn" aria-label="Add to {sec.name}" title="Add a {item} or a project" aria-haspopup="menu" onclick={(e) => addMenu(e, sec)}><Plus size={14} /></button>
        </div>
      {/if}
      {#if !collapsed[sec.id]}
        {#if editing?.kind === 'new-client' && editing.sectionId === sec.id}{@render editor(`${cap(item)} name`)}{/if}
        {#each groups as client (client.id)}
          {#if editing?.kind === 'rename-client' && editing.id === client.id}
            {@render editor(`${cap(item)} name`, editing.value)}
          {:else}
            <div class="group relative">
              <button
                class="absolute top-[3px] left-0 z-[1] grid size-6 place-items-center rounded text-subtle hover:bg-hover hover:text-text"
                aria-label="{collapsed[client.id] ? 'Expand' : 'Collapse'} {client.name}"
                aria-expanded={!collapsed[client.id]}
                onclick={() => toggle(client.id)}
              >
                <ChevronRight size={13} class="transition-transform {collapsed[client.id] ? '' : 'rotate-90'}" />
              </button>
              <button
                class="nav-item client-row w-full"
                class:active={isActive({ kind: 'client', id: client.id })}
                onclick={() => setScope({ kind: 'client', id: client.id })}
                oncontextmenu={(e) => clientMenu(e, client)}
                onkeydown={(e) => reorderKeys(e, (dir) => ipc.moveClient(client.id, dir))}
                ondblclick={() => (editing = { kind: 'rename-client', id: client.id, value: client.name })}
              >
                <span class="grid size-[16px] shrink-0 place-items-center rounded text-[9.5px] font-bold text-white" style="background: {swatch(client.color, 0.6)}">
                  {client.name.charAt(0).toUpperCase()}
                </span>
                <span class="flex-1 truncate text-left font-medium">{client.name}</span>
              </button>
              <button
                class="icon-btn absolute top-1 right-1 opacity-0 group-hover:opacity-100 focus:opacity-100"
                aria-label="{cap(item)} actions"
                onclick={(e) => clientMenu(e, client)}
              ><MoreHorizontal size={14} /></button>
            </div>
          {/if}
          {#if !collapsed[client.id]}
            {#each projectsOf(client.id) as p (p.id)}
              {@render projectRow(p, true)}
            {/each}
            {#if editing?.kind === 'new-project' && editing.clientId === client.id}
              <div class="pl-[22px]">{@render editor('Project name')}</div>
            {/if}
          {/if}
        {/each}
        {#each loose as p (p.id)}
          {@render projectRow(p, false)}
        {/each}
        {#if editing?.kind === 'new-project' && editing.clientId === null && editing.sectionId === sec.id}{@render editor('Project name')}{/if}
        {#if !groups.length && !loose.length && editing?.kind !== 'new-client' && editing?.kind !== 'new-project'}
          <p class="px-2 py-1 text-xs text-subtle">Add a {item} or a project.</p>
        {/if}
      {/if}
    {/each}
    <button class="new-section" onclick={() => (app.sectionDialog = { id: null })}><Plus size={13} /> New section</button>

    {#if layout.sections.savedViews}
    <div class="section mt-4" role="presentation" oncontextmenu={customizeMenu}>
      <span>Saved views</span>
      <button class="icon-btn" aria-label="Save current view" title="Save current filters as a view" onclick={() => (editing = { kind: 'new-view' })}><Plus size={14} /></button>
    </div>
    {#each app.views as v (v.id)}
      {#if editing?.kind === 'rename-view' && editing.id === v.id}
        {@render editor('View name', editing.value)}
      {:else}
        <button
          class="nav-item"
          class:active={app.activeViewId === v.id && !isPage(app.view)}
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
    flex-shrink: 0;
    padding-inline: 0.5rem;
    border-radius: 6px;
    color: var(--text);
    transition: background-color 120ms var(--ease-out);
  }
  /* Clients leave a gutter for the collapse chevron; their projects indent
     under the client's badge. */
  .nav-item.client-row {
    padding-left: 26px;
  }
  .section-head {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 30px;
    padding-left: 22px;
    padding-right: 0.25rem;
  }
  .section-name {
    display: flex;
    flex: 1;
    min-width: 0;
    align-items: center;
    gap: 6px;
    min-height: 24px;
    padding-inline: 4px;
    border-radius: 5px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-subtle);
    text-align: left;
  }
  .section-name:hover {
    background: var(--hover);
    color: var(--text);
  }
  .section-name.active {
    background: var(--accent-soft);
    color: var(--accent-soft-text);
  }
  .new-section {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    margin-top: 10px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    font-size: 12.5px;
    color: var(--text-subtle);
    flex-shrink: 0;
  }
  .new-section:hover {
    background: var(--hover);
    color: var(--text);
  }
  .nav-item.in-client {
    padding-left: 30px;
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
