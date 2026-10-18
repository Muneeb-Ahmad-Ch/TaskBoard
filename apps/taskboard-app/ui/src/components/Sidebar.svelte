<script lang="ts">
  import { untrack } from 'svelte'
  import { Bookmark, ChevronRight, Inbox, LayoutDashboard, Layers, MoreHorizontal, Pin, Plus, Search, Settings, Star, CalendarDays, User, Video } from '@lucide/svelte'
  import type { Client } from '../lib/bindings/Client'
  import type { Section } from '../lib/bindings/Section'
  import type { Direction } from '../lib/bindings/Direction'
  import type { Id } from '../lib/bindings/Id'
  import type { ProjectSummary } from '../lib/bindings/ProjectSummary'
  import type { SavedView } from '../lib/bindings/SavedView'
  import { cap, plural, swatch } from '../lib/format'
  import { ipc } from '../lib/ipc'
  import { act, activeViewChanged, app, applySavedView, groupsIn, inboxId, isPage, itemName, looseProjectsIn, projectsOf, saveCurrentView, sectionById, setScope, updateActiveView, type Scope } from '../lib/store.svelte'
  import { layout, NAV_LABELS, pinProject, unpinProject, type NavId } from '../lib/layout.svelte'
  import { beginDrag, dropClass, setDropResolver, type DragItem, type DropTarget } from '../lib/dnd.svelte'
  import { startOfDay, addDays, newMeeting } from '../lib/cal.svelte'
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
  /** "+" on a project: open it and put the cursor in quick add. */
  function addTaskTo(p: ProjectSummary) {
    setScope({ kind: 'project', id: p.id })
    if (app.view !== 'table') app.view = 'table'
    app.quickAddFocus++
  }

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
        layout.pinned.includes(p.id)
          ? { label: 'Unpin', run: () => unpinProject(p.id) }
          : { label: 'Pin to top', run: () => pinProject(p.id) },
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

  // ---------- pinned projects ----------
  const pinnedProjects = $derived(
    layout.pinned.map((id) => app.projects.find((p) => p.id === id)).filter((p): p is ProjectSummary => !!p && !p.is_inbox),
  )

  // ---------- drag and drop ----------
  // Rows carry `data-drop` keys ("section:<id>", "group:<id>", "project:<id>",
  // "pin:<id>", "pins", "inbox"); the resolver turns the row under the pointer
  // into a drop: before / after it, or inside it.
  let list: HTMLDivElement | undefined = $state()

  /** The id that comes after `id` in `ids` once `skip` is taken out (null = last). */
  function nextAfter(ids: Id[], id: Id, skip: Id): Id | null {
    const rest = ids.filter((x) => x !== skip)
    return rest[rest.indexOf(id) + 1] ?? null
  }

  const projectById = (id: Id) => app.projects.find((p) => p.id === id)

  /** Siblings and container of a project row, for dropping next to it. */
  function containerOf(p: ProjectSummary) {
    return p.client_id
      ? { ids: projectsOf(p.client_id).map((x) => x.id), to: { kind: 'group' as const, id: p.client_id } }
      : { ids: looseProjectsIn(p.section_id ?? '').map((x) => x.id), to: { kind: 'section' as const, id: p.section_id ?? '' } }
  }

  function moveTasks(ids: Id[], projectId: Id, name: string) {
    void act(async () => {
      if (ids.length === 1) await ipc.updateTask(ids[0], { project_id: projectId })
      else await ipc.bulkUpdateTasks(ids, { project_id: projectId })
      return true
    }, `Moved ${plural(ids.length, 'task')} to ${name}`)
  }

  function resolveDrop(item: DragItem, x: number, y: number): DropTarget | null {
    // Scroll the list while the pointer rests near its top or bottom edge.
    if (list) {
      const r = list.getBoundingClientRect()
      if (x >= r.left && x <= r.right) {
        if (y < r.top + 28) list.scrollTop -= 12
        else if (y > r.bottom - 28) list.scrollTop += 12
      }
    }
    const el = document.elementsFromPoint(x, y).find((e): e is HTMLElement => e instanceof HTMLElement && !!e.dataset.drop)
    if (!el) return null
    const key = el.dataset.drop ?? ''
    const [kind, id = ''] = key.split(':')
    const box = el.getBoundingClientRect()
    const half: 'before' | 'after' = y < box.top + box.height / 2 ? 'before' : 'after'
    const at = (position: DropTarget['position'], run: () => void): DropTarget => ({ key, position, run })

    if (item.kind === 'tasks') {
      if (kind === 'inbox') {
        const inbox = inboxId()
        return inbox ? at('inside', () => moveTasks(item.ids, inbox, 'Inbox')) : null
      }
      if (kind !== 'project' && kind !== 'pin') return null
      const p = projectById(id)
      return p ? at('inside', () => moveTasks(item.ids, p.id, p.name)) : null
    }

    if (item.kind === 'section') {
      if (kind !== 'section' || id === item.id) return null
      const before = half === 'before' ? id : nextAfter(app.sections.map((s) => s.id), id, item.id)
      return at(half, () => void act(() => ipc.placeSection(item.id, before), `Moved “${item.label}”`))
    }

    if (item.kind === 'group') {
      if (kind === 'section') return at('inside', () => void act(() => ipc.placeClient(item.id, id, null), `Moved “${item.label}”`))
      if (kind !== 'group' || id === item.id) return null
      const target = app.clients.find((c) => c.id === id)
      if (!target?.section_id) return null
      const sectionId = target.section_id
      const before = half === 'before' ? id : nextAfter(groupsIn(sectionId).map((c) => c.id), id, item.id)
      return at(half, () => void act(() => ipc.placeClient(item.id, sectionId, before), `Moved “${item.label}”`))
    }

    // Projects. Pinned rows only reorder or extend the pinned list.
    if (kind === 'pins') return at('inside', () => pinProject(item.id))
    if (kind === 'pin') {
      if (id === item.id) return null
      const before = half === 'before' ? id : nextAfter(layout.pinned, id, item.id)
      return at(half, () => pinProject(item.id, before))
    }
    if (item.pinned) return null
    const moveTo = (to: { kind: 'group' | 'section'; id: Id }, before: Id | null) => () =>
      void act(() => ipc.placeProject(item.id, to, before), `Moved “${item.label}”`)
    if (kind === 'group') return at('inside', moveTo({ kind: 'group', id }, null))
    if (kind === 'section') return at('inside', moveTo({ kind: 'section', id }, null))
    if (kind !== 'project' || id === item.id) return null
    const target = projectById(id)
    if (!target || target.is_inbox) return null
    const { ids, to } = containerOf(target)
    const before = half === 'before' ? id : nextAfter(ids, id, item.id)
    return at(half, moveTo(to, before))
  }

  $effect(() => {
    setDropResolver(resolveDrop)
    return () => setDropResolver(null)
  })

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

{#snippet projectRow(p: ProjectSummary, indent: boolean, pinned = false)}
  {#if editing?.kind === 'rename-project' && editing.id === p.id && !pinned}
    {@render editor('Project name', editing.value)}
  {:else}
    {@const pct = p.total_tasks ? Math.round((p.done_tasks / p.total_tasks) * 100) : 0}
    {@const dropKey = `${pinned ? 'pin' : 'project'}:${p.id}`}
    <div class="drop group relative flex flex-col {dropClass(dropKey)}" data-drop={dropKey}>
    <button
      class="nav-item"
      class:in-client={indent}
      class:active={isActive({ kind: 'project', id: p.id })}
      onclick={() => setScope({ kind: 'project', id: p.id })}
      onpointerdown={(e) => beginDrag(e, { kind: 'project', id: p.id, label: p.name, pinned })}
      oncontextmenu={(e) => projectMenu(e, p)}
      onkeydown={(e) => reorderKeys(e, (dir) => ipc.moveProject(p.id, dir))}
      ondblclick={() => (editing = { kind: 'rename-project', id: p.id, value: p.name })}
    >
      <span class="grid shrink-0 place-items-center" title={p.total_tasks ? `${p.done_tasks} of ${p.total_tasks} done` : 'No tasks yet'}>
        <svg class="progress-ring" viewBox="0 0 16 16" aria-hidden="true" style="--c: {swatch(p.color, 0.68, 0.16)}">
          <circle class="track" cx="8" cy="8" r="5.75" />
          {#if pct > 0}
            <circle class="arc" cx="8" cy="8" r="5.75" pathLength="100" stroke-dasharray="{pct} 100" transform="rotate(-90 8 8)" />
          {/if}
        </svg>
      </span>
      <span class="flex-1 truncate text-left">{p.name}</span>
      {#if pinned && p.client_name}<span class="max-w-[40%] truncate text-xs text-subtle">{p.client_name}</span>{/if}
      {#if p.open_tasks && layout.counts.projects}<span class="tabular text-xs text-subtle group-hover:invisible group-focus-within:invisible">{p.open_tasks}</span>{/if}
    </button>
    <button
      class="icon-btn absolute top-[3px] right-1 opacity-0 group-hover:opacity-100 focus:opacity-100"
      aria-label="Add a task to {p.name}"
      title="Add a task"
      onclick={() => addTaskTo(p)}
    ><Plus size={14} /></button>
    </div>
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
      <div class="drop group relative {id === 'inbox' ? dropClass('inbox') : ''}" data-drop={id === 'inbox' ? 'inbox' : undefined}>
      <button class="nav-item w-full" class:active={navActive(id)} onclick={() => navGo(id)} oncontextmenu={customizeMenu}>
        <Icon size={15} />
        <span class="flex-1 text-left">{NAV_LABELS[id]}</span>
        <!-- Meetings swaps its count for a + on hover. -->
        <span class="counts flex items-center gap-2 {id === 'meetings' ? 'group-hover:invisible group-focus-within:invisible' : ''}">
        {#if id === 'today'}
          {#if app.counts?.overdue && layout.counts.overdue}<span class="badge-danger tabular" title="Overdue">{app.counts.overdue}</span>{/if}
          {#if app.counts?.today && layout.counts.views}<span class="tabular text-xs text-subtle">{app.counts.today}</span>{/if}
        {:else if id === 'upcoming'}
          {#if app.counts?.upcoming && layout.counts.views}<span class="tabular text-xs text-subtle">{app.counts.upcoming}</span>{/if}
        {:else if id === 'inbox'}
          {#if app.counts?.inbox && layout.counts.views}<span class="tabular text-xs text-subtle">{app.counts.inbox}</span>{/if}
        {:else if id === 'meetings'}
          {#if meetingsLeft && layout.counts.views}<span class="tabular text-xs text-subtle" title="Meetings left today">{meetingsLeft}</span>{/if}
        {/if}
        </span>
      </button>
      {#if id === 'meetings'}
        <button
          class="icon-btn absolute top-[3px] right-1 opacity-0 group-hover:opacity-100 focus:opacity-100"
          aria-label="New meeting"
          title="New meeting"
          onclick={newMeeting}
        ><Plus size={14} /></button>
      {/if}
      </div>
    {/each}
  </nav>

  <div bind:this={list} class="-mx-1 mt-4 flex min-h-0 flex-1 flex-col gap-px overflow-y-auto px-1 pb-2">
    {#if layout.sections.pinned && pinnedProjects.length}
      <div class="drop section {dropClass('pins')}" data-drop="pins" role="presentation" oncontextmenu={customizeMenu}>
        <span class="flex items-center gap-1.5"><Pin size={11} /> Pinned</span>
      </div>
      {#each pinnedProjects as p (p.id)}
        {@render projectRow(p, false, true)}
      {/each}
      <div class="mb-3"></div>
    {/if}
    {#each visibleSections as sec, si (sec.id)}
      {@const item = itemName(sec)}
      {@const groups = groupsIn(sec.id)}
      {@const loose = looseProjectsIn(sec.id)}
      {#if editing?.kind === 'rename-section' && editing.id === sec.id}
        {@render editor('Section name', editing.value)}
      {:else}
        <div class="drop section-head group relative {dropClass(`section:${sec.id}`)}" class:mt-4={si > 0} data-drop="section:{sec.id}">
          <button
            class="section-name"
            class:active={isActive({ kind: 'section', id: sec.id })}
            title="Show all work in {sec.name}"
            onclick={() => setScope({ kind: 'section', id: sec.id })}
            onpointerdown={(e) => beginDrag(e, { kind: 'section', id: sec.id, label: sec.name })}
            oncontextmenu={(e) => sectionMenu(e, sec)}
            onkeydown={(e) => reorderKeys(e, (dir) => ipc.moveSection(sec.id, dir))}
            ondblclick={() => (editing = { kind: 'rename-section', id: sec.id, value: sec.name })}
          >
            {#if layout.sectionDots}<span class="size-2 shrink-0 rounded-full" style="background: {swatch(sec.color)}"></span>{/if}
            <span class="truncate">{sec.name}</span>
            {#if layout.counts.sections}<span class="shrink-0 font-normal text-subtle tabular">· {groups.length + loose.length}</span>{/if}
          </button>
          <button class="icon-btn" aria-label="Add to {sec.name}" title="Add a {item} or a project" aria-haspopup="menu" onclick={(e) => addMenu(e, sec)}><Plus size={14} /></button>
          <button
            class="icon-btn"
            aria-label="{collapsed[sec.id] ? 'Expand' : 'Collapse'} {sec.name}"
            aria-expanded={!collapsed[sec.id]}
            onclick={() => toggle(sec.id)}
          >
            <ChevronRight size={13} class="transition-transform {collapsed[sec.id] ? '' : 'rotate-90'}" />
          </button>
        </div>
      {/if}
      {#if !collapsed[sec.id]}
        {#if editing?.kind === 'new-client' && editing.sectionId === sec.id}{@render editor(`${cap(item)} name`)}{/if}
        {#each groups as client (client.id)}
          {#if editing?.kind === 'rename-client' && editing.id === client.id}
            {@render editor(`${cap(item)} name`, editing.value)}
          {:else}
            <div class="drop group relative {dropClass(`group:${client.id}`)}" data-drop="group:{client.id}">
              <button
                class="nav-item client-row w-full"
                class:active={isActive({ kind: 'client', id: client.id })}
                onclick={() => setScope({ kind: 'client', id: client.id })}
                onpointerdown={(e) => beginDrag(e, { kind: 'group', id: client.id, label: client.name })}
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
                class="icon-btn absolute top-[3px] right-[54px] opacity-0 group-hover:opacity-100 focus:opacity-100"
                aria-label="New project in {client.name}"
                title="New project"
                onclick={() => startProject(client)}
              ><Plus size={14} /></button>
              <button
                class="icon-btn absolute top-[3px] right-[29px] opacity-0 group-hover:opacity-100 focus:opacity-100"
                aria-label="{cap(item)} actions"
                onclick={(e) => clientMenu(e, client)}
              ><MoreHorizontal size={14} /></button>
              <button
                class="icon-btn absolute top-[3px] right-1"
                aria-label="{collapsed[client.id] ? 'Expand' : 'Collapse'} {client.name}"
                aria-expanded={!collapsed[client.id]}
                onclick={() => toggle(client.id)}
              >
                <ChevronRight size={13} class="transition-transform {collapsed[client.id] ? '' : 'rotate-90'}" />
              </button>
            </div>
          {/if}
          {#if !collapsed[client.id]}
            {#each projectsOf(client.id) as p (p.id)}
              {@render projectRow(p, true)}
            {/each}
            {#if editing?.kind === 'new-project' && editing.clientId === client.id}
              <div class="pl-[24px]">{@render editor('Project name')}</div>
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
  /* Groups keep room on the right for the actions button and fold arrow;
     their projects line up under the group's name. */
  .nav-item.client-row {
    padding-right: 82px;
  }
  .section-head {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 30px;
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
    font-size: 13px;
    font-weight: 650;
    color: var(--text);
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
    padding-left: 32px;
  }
  /* Project progress ring in the project's color (SVG, so it stays crisp);
     the faint track keeps the color visible before any task is done. */
  .progress-ring {
    width: 15px;
    height: 15px;
    fill: none;
    stroke-width: 2.5;
  }
  .progress-ring .track {
    stroke: color-mix(in oklch, var(--c) 30%, transparent);
  }
  .progress-ring .arc {
    stroke: var(--c);
    stroke-linecap: round;
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
  /* Where a dragged item lands: a line above / below a row, or the row lit up. */
  .drop {
    border-radius: 6px;
  }
  .drop-before {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .drop-after {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .drop-inside {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
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
