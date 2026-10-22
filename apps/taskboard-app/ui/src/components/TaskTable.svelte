<script lang="ts">
  import { beginDrag } from '../lib/dnd.svelte'
  import { Archive, ArrowDown, ArrowUp, CalendarDays, CheckCircle2, ChevronDown, ChevronRight, Circle, CircleDot, CircleDashed, Columns3, Hourglass, Inbox, ListFilter, ListTodo, Repeat, Search, Star, Trash2, X } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import type { Id } from '../lib/bindings/Id'
  import type { Priority } from '../lib/bindings/Priority'
  import type { Status } from '../lib/bindings/Status'
  import type { TaskPatch } from '../lib/bindings/TaskPatch'
  import type { TaskQuery } from '../lib/bindings/TaskQuery'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import type { TaskSummary } from '../lib/bindings/TaskSummary'
  import { buildColumns } from '../lib/board'
  import { filterCount } from '../lib/filters'
  import { addedLabel, addedTitle, dueLabel, isOverdue, isTyping, isoDate, minutes, parseMinutes, plural, swatch, todayIso } from '../lib/format'
  import { flattenGroups, groupRows, type GroupItem } from '../lib/grouping'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { act, app, archiveCompleted, clearFilter, listQuery, scopeProjectIds, sectionById } from '../lib/store.svelte'
  import { layout as viewLayout } from '../lib/layout.svelte'
  import { toast } from '../lib/toast.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'
  import { COLUMN_DEFS, orderedColumns, saveTableLayout, setColumnVisible, tableLayout, type ColId, type ColumnDef } from '../lib/tableLayout.svelte'
  import Progress from './Progress.svelte'

  const ROW = 36
  const PAGE = 200
  const OVERSCAN = 12

  // ---------- columns ----------
  // Order, visibility and widths are shared with Settings → Views & table.
  // Resizing works like a spreadsheet: dragging a column's right edge changes
  // that column only, the columns after it move along, and the table scrolls
  // sideways when it gets wider than the window. Task fills the free space
  // until it has a width of its own (the first resize gives it one, so nothing
  // else moves); double-clicking an edge fits the column to its content.
  type Col = ColumnDef
  const layout = tableLayout
  const saveLayout = saveTableLayout
  const TITLE_MIN = 180
  const COL_MIN = 60
  const COL_MAX = 900
  /** Auto-hidden first when the table is too narrow (user-hidden ones stay hidden). */
  const SQUEEZE_ORDER: ColId[] = ['added', 'tags', 'estimate', 'progress', 'priority', 'status']
  let width = $state(1200)
  /** In a project's (or the Inbox's) own list every row has the same project. */
  const projectColumnRepeats = $derived(app.display.hide_repeated_context && (app.scope.kind === 'project' || app.scope.kind === 'inbox'))
  /** In a group's own list every row has the same group. */
  const groupRepeats = $derived(app.display.hide_repeated_context && app.scope.kind === 'client')
  const cols = $derived.by(() => {
    let list = orderedColumns()
      .filter((c) => c.id === 'title' || (!layout.hidden.includes(c.id) && !(c.id === 'project' && projectColumnRepeats)))
      .map((c) => ({ ...c, width: layout.widths[c.id] ?? c.width }))
    // Which columns fit depends on the window and default widths only, never on
    // how they were sized, so resizing never makes another column appear or vanish.
    const fit = (c: Col) => COLUMN_DEFS.find((d) => d.id === c.id)?.width ?? c.width
    const need = () => 36 + TITLE_MIN + list.reduce((sum, c) => sum + (c.id === 'title' ? 0 : fit(c)), 0)
    for (const id of SQUEEZE_ORDER) {
      if (need() <= width) break
      list = list.filter((c) => c.id !== id)
    }
    return list
  })
  /** Task has a width of its own (else it fills the free space). */
  const taskSized = $derived(layout.widths.title !== undefined)
  /** While an edge is dragged the table doesn't get narrower (spare room goes to
   *  the empty end), so a scrolled view can't shift under the pointer. */
  let dragFloor = $state(0)
  const minWidth = $derived(Math.max(dragFloor, 36 + cols.reduce((sum, c) => sum + (c.id === 'title' && !taskSized ? TITLE_MIN : c.width), 0)))
  const gridTemplate = $derived(
    `36px ${cols.map((c) => (c.id === 'title' && !taskSized ? `minmax(${TITLE_MIN}px, 1fr)` : `${c.width}px`)).join(' ')}${taskSized ? ' minmax(0, 1fr)' : ''}`,
  )
  const headerLabel = (c: Col) => (c.id === 'project' && groupRepeats ? 'Project' : c.label)

  const clampWidth = (id: ColId, w: number) =>
    Math.round(id === 'title' ? Math.max(TITLE_MIN, Math.min(2000, w)) : Math.max(COL_MIN, Math.min(COL_MAX, w)))

  /** A column's header cell (after the checkbox column). */
  function headerCell(id: ColId): HTMLElement | undefined {
    const i = cols.findIndex((c) => c.id === id)
    return i < 0 ? undefined : viewport?.querySelectorAll<HTMLElement>('[role="columnheader"]')[i + 1]
  }

  /** The width a column has on screen right now. */
  function shownWidth(id: ColId): number {
    return headerCell(id)?.getBoundingClientRect().width ?? layout.widths[id] ?? COLUMN_DEFS.find((d) => d.id === id)?.width ?? 120
  }

  /** Before a resize, Task keeps the width it has, so only the dragged column changes. */
  function pinTask() {
    if (layout.widths.title === undefined) layout.widths.title = clampWidth('title', shownWidth('title'))
  }

  function startResize(e: PointerEvent, col: Col) {
    if (e.button !== 0) return
    e.preventDefault()
    e.stopPropagation()
    const handle = e.currentTarget as HTMLElement
    handle.setPointerCapture(e.pointerId)
    document.body.classList.add('col-resizing')
    dragFloor = viewport?.scrollWidth ?? 0
    pinTask()
    const id = col.id
    const startX = e.clientX
    const start = layout.widths[id] ?? shownWidth(id)
    const move = (ev: PointerEvent) => {
      layout.widths[id] = clampWidth(id, start + ev.clientX - startX)
    }
    const up = () => {
      handle.removeEventListener('pointermove', move)
      handle.removeEventListener('pointerup', up)
      handle.removeEventListener('pointercancel', up)
      document.body.classList.remove('col-resizing')
      dragFloor = 0
      saveLayout()
    }
    handle.addEventListener('pointermove', move)
    handle.addEventListener('pointerup', up)
    handle.addEventListener('pointercancel', up)
  }

  /** Keyboard: ← / → resize by 16px (Shift: 48px); Enter fits; Home resets. */
  function resizeKey(e: KeyboardEvent, col: Col) {
    const step = (e.key === 'ArrowLeft' ? -16 : e.key === 'ArrowRight' ? 16 : 0) * (e.shiftKey ? 3 : 1)
    if (step) {
      pinTask()
      layout.widths[col.id] = clampWidth(col.id, (layout.widths[col.id] ?? shownWidth(col.id)) + step)
    } else if (e.key === 'Home') delete layout.widths[col.id]
    else if (e.key === 'Enter') return autoFit(col)
    else return
    e.preventDefault()
    e.stopPropagation()
    saveLayout()
  }

  /** The width `el` would take with no limit (text untruncated). */
  function naturalWidth(el: HTMLElement): number {
    const { width: w, maxWidth: mw, flex } = el.style
    el.style.width = 'max-content'
    el.style.maxWidth = 'none'
    el.style.flex = 'none'
    const px = el.getBoundingClientRect().width
    el.style.width = w
    el.style.maxWidth = mw
    el.style.flex = flex
    return px
  }

  /** Double-click an edge: fit the column to its widest content on screen. */
  function autoFit(col: Col) {
    const i = cols.findIndex((c) => c.id === col.id)
    if (i < 0 || !viewport) return
    const label = headerCell(col.id)?.querySelector<HTMLElement>('button')
    // Cell padding (16) and room for the edge handle (8).
    let widest = label ? naturalWidth(label) + 24 : 0
    for (const row of viewport.querySelectorAll<HTMLElement>('[role="row"][aria-rowindex]')) {
      const content = row.querySelectorAll<HTMLElement>('[role="gridcell"]')[i + 1]?.firstElementChild as HTMLElement | null
      if (content) widest = Math.max(widest, naturalWidth(content) + 18)
    }
    if (!widest) return
    pinTask()
    layout.widths[col.id] = clampWidth(col.id, widest)
    saveLayout()
  }

  // ---------- data: windowed pages ----------
  let total = $state(0)
  let loaded = $state(false)
  let summary = $state<TaskSummary | null>(null)
  const pages = new SvelteMap<number, TaskRow[]>()
  const inflight = new Set<number>()
  let generation = 0
  let viewport: HTMLDivElement | undefined = $state()
  let scrollTop = $state(0)
  let height = $state(600)

  const queryKey = $derived(JSON.stringify(listQuery()))

  // ---------- grouping ----------
  // Grouped, the whole list is loaded (up to GROUP_CAP rows) and drawn as
  // group headers + rows; each group keeps the list's sort order.
  const GROUP_CAP = 5000
  const grouped = $derived(app.groupBy !== 'none')
  const collapsedGroups = new SvelteSet<string>()
  $effect(() => {
    void app.groupBy
    untrack(() => collapsedGroups.clear())
  })
  const items = $derived.by((): GroupItem[] => {
    if (!grouped) return []
    const rows = [...pages.keys()].sort((a, b) => a - b).flatMap((k) => pages.get(k) ?? [])
    const ctx = {
      columns: buildColumns(app.statuses, scopeProjectIds()).map((c) => c.key),
      projects: app.projects.map((p) => p.id),
      clients: app.clients.map((c) => c.id),
      now: new Date(),
    }
    return flattenGroups(groupRows(rows, app.groupBy, ctx), collapsedGroups)
  })
  /** Lines in the list: rows, plus group headers when grouped. */
  const count = $derived(grouped ? items.length : total)

  function groupAt(i: number) {
    const item = grouped ? items[i] : undefined
    return item?.kind === 'group' ? item : undefined
  }

  function toggleGroup(key: string) {
    if (collapsedGroups.has(key)) collapsedGroups.delete(key)
    else collapsedGroups.add(key)
  }
  /** The sticky header sits inside the scroller, above row 0. */
  const HEADER = 36
  const first = $derived(Math.max(0, Math.floor((scrollTop - HEADER) / ROW) - OVERSCAN))
  const last = $derived(Math.min(count, Math.ceil((scrollTop + height) / ROW) + OVERSCAN))

  function rowAt(i: number): TaskRow | undefined {
    if (grouped) {
      const item = items[i]
      return item?.kind === 'row' ? item.row : undefined
    }
    return pages.get(Math.floor(i / PAGE))?.[i % PAGE]
  }

  async function fetchPage(page: number, gen: number) {
    if (inflight.has(page) || !inTauri) return
    inflight.add(page)
    try {
      const q = { ...JSON.parse(queryKey), offset: page * PAGE, limit: PAGE }
      const result = await ipc.listTasks(q)
      if (gen !== generation) return
      total = result.total
      pages.set(page, result.rows)
      loaded = true
    } catch (err) {
      if (gen === generation) toast(errorMessage(err), { kind: 'error' })
    } finally {
      inflight.delete(page)
    }
  }

  /** Grouped: every row (up to GROUP_CAP), in pages of PAGE so edits find them. */
  async function fetchAll(gen: number) {
    if (!inTauri) return
    try {
      const q = JSON.parse(queryKey)
      const next = new Map<number, TaskRow[]>()
      let offset = 0
      let all = 0
      do {
        const result = await ipc.listTasks({ ...q, offset, limit: 1000 })
        if (gen !== generation) return
        all = result.total
        for (let k = 0; k < result.rows.length; k += PAGE) next.set((offset + k) / PAGE, result.rows.slice(k, k + PAGE))
        offset += result.rows.length
        if (!result.rows.length) break
      } while (offset < Math.min(all, GROUP_CAP))
      pages.clear()
      for (const [k, rows] of next) pages.set(k, rows)
      total = all
      loaded = true
    } catch (err) {
      if (gen === generation) toast(errorMessage(err), { kind: 'error' })
    }
  }

  function visiblePages(): number[] {
    const from = Math.floor(first / PAGE)
    const to = Math.floor(Math.max(first, last - 1) / PAGE)
    return Array.from({ length: to - from + 1 }, (_, k) => from + k)
  }

  // New query or grouping → start over at the top.
  $effect(() => {
    void queryKey
    void grouped
    untrack(() => {
      generation++
      pages.clear()
      inflight.clear()
      total = 0
      loaded = false
      selected.clear()
      anchor = -1
      cursor = -1
      if (viewport) viewport.scrollTop = 0
      scrollTop = 0
      if (grouped) void fetchAll(generation)
      else void fetchPage(0, generation)
    })
  })

  // Data changed elsewhere → refresh what's on screen without clearing it.
  // Depends on `dataVersion` only; untracked so loading pages can't re-trigger it.
  $effect(() => {
    if (!app.dataVersion) return
    untrack(() => {
      generation++
      if (grouped) return void fetchAll(generation)
      const keep = new Set(visiblePages())
      for (const p of [...pages.keys()]) if (!keep.has(p)) pages.delete(p)
      inflight.clear()
      for (const p of keep) void fetchPage(p, generation)
    })
  })

  // Scrolling into unloaded pages.
  $effect(() => {
    if (grouped) return
    for (const p of visiblePages()) if (!pages.has(p)) void fetchPage(p, generation)
  })

  /** Completed top-level tasks in view: what "Archive completed" would take. */
  let completedHere = $state(0)
  /** Completed tasks an empty list is hiding, for its empty state. */
  let hiddenDone = $state(0)

  // Footer summary (debounced; it scans the whole query).
  $effect(() => {
    const key = queryKey
    void app.dataVersion
    if (!inTauri) return
    const t = setTimeout(async () => {
      const q: TaskQuery = JSON.parse(key)
      const showsDone = !q.categories?.length || q.categories.includes('done')
      try {
        const [s, c] = await Promise.all([
          ipc.summarizeTasks(q),
          showsDone ? ipc.summarizeTasks({ ...q, categories: ['done', 'cancelled'], top_level: true }) : null,
        ])
        summary = s
        completedHere = c?.total ?? 0
        hiddenDone = !s.total && !showsDone ? (await ipc.summarizeTasks(hiddenDoneQuery(q))).total : 0
      } catch {
        summary = null
        completedHere = 0
        hiddenDone = 0
      }
    }, 120)
    return () => clearTimeout(t)
  })

  /** Today: what was finished today. Other lists: their completed tasks. */
  function hiddenDoneQuery(q: TaskQuery): TaskQuery {
    if (app.scope.kind !== 'today') return { ...q, categories: ['done'] }
    const now = new Date()
    return { due_to: q.due_to, categories: ['done'], completed_from: new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() }
  }

  $effect(() => {
    if (!viewport) return
    const ro = new ResizeObserver(() => {
      height = viewport?.clientHeight ?? 600
      width = viewport?.clientWidth ?? 1200
    })
    ro.observe(viewport)
    return () => ro.disconnect()
  })

  // ---------- selection & keyboard ----------
  const selected = new SvelteSet<Id>()
  let anchor = -1
  let cursor = $state(-1)
  let renaming = $state<Id | null>(null)

  function select(i: number, e?: MouseEvent | KeyboardEvent) {
    const row = rowAt(i)
    if (!row) {
      // A group header takes the cursor (Enter / ←→ open and close it), not the selection.
      if (groupAt(i)) cursor = i
      return
    }
    if (e?.shiftKey && anchor >= 0) {
      selected.clear()
      for (let k = Math.min(anchor, i); k <= Math.max(anchor, i); k++) {
        const r = rowAt(k)
        if (r) selected.add(r.id)
      }
    } else if (e && (e.ctrlKey || e.metaKey)) {
      if (selected.has(row.id)) selected.delete(row.id)
      else selected.add(row.id)
      anchor = i
    } else {
      selected.clear()
      selected.add(row.id)
      anchor = i
    }
    cursor = i
  }

  function scrollIntoView(i: number) {
    if (!viewport) return
    const top = HEADER + i * ROW
    if (top - HEADER < viewport.scrollTop) viewport.scrollTop = top - HEADER
    else if (top + ROW > viewport.scrollTop + viewport.clientHeight)
      viewport.scrollTop = top + ROW - viewport.clientHeight
  }

  function selectedIds(): Id[] {
    return [...selected]
  }

  function onkeydown(e: KeyboardEvent) {
    if (renaming || isTyping(e)) return
    const move = (d: number) => {
      e.preventDefault()
      const i = Math.max(0, Math.min(count - 1, (cursor < 0 ? -1 : cursor) + d))
      select(i, e.shiftKey ? e : undefined)
      scrollIntoView(i)
      if (app.openTaskId) app.openTaskId = rowAt(i)?.id ?? null
    }
    const current = rowAt(cursor)
    const head = groupAt(cursor)
    const opens = head?.collapsed ? e.key === 'ArrowRight' : e.key === 'ArrowLeft'
    if (head && (e.key === 'Enter' || e.key === ' ' || opens)) {
      e.preventDefault()
      toggleGroup(head.group.key)
      return
    }
    switch (e.key) {
      case 'ArrowDown':
      case 'j':
        return move(1)
      case 'ArrowUp':
      case 'k':
        return move(-1)
      case 'Enter':
        if (current) app.openTaskId = current.id
        return
      case 'F2':
      case 'e':
        if (current) {
          e.preventDefault()
          renaming = current.id
        }
        return
      case ' ':
        if (selected.size) {
          e.preventDefault()
          const ids = selectedIds()
          const allDone = ids.every((id) => rowById(id)?.completed_at)
          void act(() => ipc.bulkSetDone(ids, !allDone))
        }
        return
      case 'Delete':
        if (selected.size) {
          e.preventDefault()
          void removeSelected()
        }
        return
      case 'Escape':
        selected.clear()
        return
      case 'a':
        if (e.ctrlKey) {
          e.preventDefault()
          if (grouped) for (const item of items) item.kind === 'row' && selected.add(item.row.id)
          else for (const rows of pages.values()) for (const r of rows) selected.add(r.id)
        } else if (!e.altKey && !e.metaKey && selected.size) {
          e.preventDefault()
          void archiveSelected()
        }
    }
  }

  function rowById(id: Id): TaskRow | undefined {
    for (const rows of pages.values()) {
      const r = rows.find((x) => x.id === id)
      if (r) return r
    }
    return undefined
  }

  // ---------- edits ----------
  /** Replaces a loaded row with an updated copy (pages hold plain arrays). */
  function patchRow(id: Id, changes: Partial<TaskRow>) {
    for (const [page, rows] of pages) {
      const i = rows.findIndex((r) => r.id === id)
      if (i >= 0) {
        const next = rows.slice()
        next[i] = { ...rows[i], ...changes }
        pages.set(page, next)
        return
      }
    }
  }

  /** Optimistic done toggle: flip locally, roll back on failure. */
  async function toggleDone(row: TaskRow) {
    const was = { completed_at: row.completed_at, category: row.category }
    const done = !row.completed_at
    patchRow(row.id, { completed_at: done ? Date.now() : null, category: done ? 'done' : 'todo' })
    try {
      await ipc.setTaskDone(row.id, done)
    } catch (err) {
      patchRow(row.id, was)
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  async function patch(ids: Id[], p: TaskPatch, message?: string) {
    if (ids.length === 1) await act(() => ipc.updateTask(ids[0], p), message)
    else await act(() => ipc.bulkUpdateTasks(ids, p), message ?? `Updated ${plural(ids.length, 'task')}`)
  }

  async function removeSelected(ids = selectedIds()) {
    selected.clear()
    await act(
      () => (ids.length === 1 ? ipc.deleteTask(ids[0]).then(() => 1) : ipc.bulkDelete(ids)),
      ids.length === 1 ? 'Task deleted' : `Deleted ${plural(ids.length, 'task')}`,
    )
  }

  /** Archive: out of every list, kept in Settings → Data → Archived. */
  async function archiveSelected(ids = selectedIds()) {
    if (!ids.length) return
    selected.clear()
    if (app.openTaskId && ids.includes(app.openTaskId)) app.openTaskId = null
    await act(
      () => (ids.length === 1 ? ipc.setTaskArchived(ids[0], true).then(() => 1) : ipc.bulkSetArchived(ids, true)),
      ids.length === 1 ? 'Task archived' : `Archived ${plural(ids.length, 'task')}`,
    )
  }

  async function rename(row: TaskRow, value: string) {
    renaming = null
    const title = value.trim()
    if (title && title !== row.title) {
      const old = row.title
      patchRow(row.id, { title })
      const ok = await act(() => ipc.updateTask(row.id, { title }))
      if (!ok) patchRow(row.id, { title: old })
    }
  }

  /** Targets for cell menus: the whole selection if the row is part of it. */
  function targets(row: TaskRow): Id[] {
    return selected.has(row.id) && selected.size > 1 ? selectedIds() : [row.id]
  }

  // ---------- cell menus ----------
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)
  let datePicker: HTMLInputElement | undefined = $state()
  let pickFor: Id[] = []
  let estimating = $state<Id | null>(null)

  function openMenu(e: MouseEvent, items: MenuEntry[]) {
    e.stopPropagation()
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    menu = { x: r.left, y: r.bottom + 2, items }
  }

  async function statusMenu(e: MouseEvent, row: TaskRow) {
    e.stopPropagation()
    const anchorEl = e.currentTarget as HTMLElement
    // The app's live list (refreshed after every change, e.g. a column added
    // from Ctrl+K or by an assistant); ask the database only if it isn't there.
    let statuses: Status[] = app.statuses.filter((s) => s.project_id === row.project_id)
    if (!statuses.length) statuses = await ipc.listStatuses(row.project_id)
    const r = anchorEl.getBoundingClientRect()
    menu = {
      x: r.left,
      y: r.bottom + 2,
      items: statuses.map((s) => ({
        label: s.name,
        color: s.color,
        checked: s.id === row.status_id,
        run: () => void patch([row.id], { status_id: s.id }),
      })),
    }
  }

  const PRIORITIES: { value: Priority; label: string }[] = [
    { value: 'urgent', label: 'Urgent' },
    { value: 'high', label: 'High' },
    { value: 'med', label: 'Medium' },
    { value: 'low', label: 'Low' },
    { value: 'none', label: 'No priority' },
  ]
  const priorityLabel: Record<Priority, string> = { urgent: 'Urgent', high: 'High', med: 'Medium', low: 'Low', none: '' }

  function priorityMenu(e: MouseEvent, row: TaskRow) {
    openMenu(
      e,
      PRIORITIES.map((p) => ({
        label: p.label,
        checked: p.value === row.priority,
        run: () => void patch(targets(row), { priority: p.value }),
      })),
    )
  }

  function shift(days: number): string {
    const d = new Date()
    d.setDate(d.getDate() + days)
    return isoDate(d)
  }

  function dueMenu(e: MouseEvent, row: TaskRow) {
    const ids = targets(row)
    const nextMonday = (() => {
      const d = new Date()
      d.setDate(d.getDate() + ((8 - d.getDay()) % 7 || 7))
      return isoDate(d)
    })()
    openMenu(e, [
      { label: 'Today', hint: dueLabel(todayIso()), run: () => void patch(ids, { due_date: todayIso() }) },
      { label: 'Tomorrow', run: () => void patch(ids, { due_date: shift(1) }) },
      { label: 'Next week', hint: dueLabel(nextMonday), run: () => void patch(ids, { due_date: nextMonday }) },
      {
        label: 'Pick a date…',
        run: () => {
          pickFor = ids
          if (datePicker) {
            datePicker.value = row.due_date ?? todayIso()
            datePicker.showPicker()
          }
        },
      },
      'separator',
      { label: 'No date', disabled: !row.due_date && ids.length === 1, run: () => void patch(ids, { due_date: null }) },
    ])
  }

  /** Manual progress for tasks without subtasks; 100% completes the task. */
  function progressMenu(e: MouseEvent, row: TaskRow) {
    if (row.subtasks_total) {
      e.stopPropagation()
      toast(`Progress comes from ${plural(row.subtasks_total, 'subtask')}`)
      return
    }
    const ids = targets(row)
    openMenu(e, [
      ...[0, 25, 50, 75].map((p) => ({
        label: `${p}%`,
        checked: !row.completed_at && row.progress === p,
        run: () => void patch(ids, { progress: p }),
      })),
      {
        label: '100% — mark done',
        checked: !!row.completed_at,
        run: () => void act(() => ipc.bulkSetDone(ids, true)),
      },
    ])
  }

  /** project id → "Section › Group / Project", for the project cell's tooltip. */
  const projectPath = $derived(
    new Map(
      app.projects.map((p) => [
        p.id,
        [sectionById(p.section_id)?.name, p.client_name ? `${p.client_name} / ${p.name}` : p.name].filter(Boolean).join(' › '),
      ]),
    ),
  )

  function projectMenu(e: MouseEvent, row: TaskRow) {
    const ids = targets(row)
    openMenu(
      e,
      app.projects.map((p) => ({
        label: p.client_name ? `${p.client_name} / ${p.name}` : p.name,
        color: p.color,
        checked: p.id === row.project_id,
        run: () => void patch(ids, { project_id: p.id }, `Moved to ${p.name}`),
      })),
    )
  }

  /**
   * Right-click a row (or press the menu key / Shift+F10 on the table): actions
   * for that row, or for the whole selection when the row is part of it.
   */
  function onGridMenu(e: MouseEvent) {
    const target = e.target as HTMLElement
    // The header has its own menu; text fields keep the native one.
    if (e.defaultPrevented || target.closest('input, textarea')) return
    const rowEl = target.closest<HTMLElement>('[aria-rowindex]')
    const i = rowEl ? Number(rowEl.getAttribute('aria-rowindex')) - 1 : cursor
    const row = rowAt(i)
    if (!row) return
    e.preventDefault()
    if (!selected.has(row.id)) select(i)
    // From the keyboard there is no pointer: open at the row.
    const at = rowEl ? { x: e.clientX, y: e.clientY } : rowPoint(i)
    const ids = targets(row)
    const many = ids.length > 1
    const allDone = ids.every((id) => rowById(id)?.completed_at)
    const items: MenuEntry[] = []
    if (!many)
      items.push(
        { label: 'Open', hint: 'Enter', run: () => (app.openTaskId = row.id) },
        { label: 'Rename', hint: 'F2', run: () => (renaming = row.id) },
      )
    items.push({
      label: `${allDone ? 'Mark not done' : 'Mark done'}${many ? ` (${ids.length})` : ''}`,
      hint: 'Space',
      run: () => void act(() => ipc.bulkSetDone(ids, !allDone)),
    })
    if (!many) items.push({ label: 'Duplicate', run: () => void act(() => ipc.duplicateTask(row.id), 'Task duplicated') })
    items.push(
      'separator',
      { label: many ? `Archive ${ids.length} tasks` : 'Archive', hint: 'A', run: () => void archiveSelected(ids) },
      { label: many ? `Delete ${ids.length} tasks` : 'Delete', hint: 'Del', danger: true, run: () => void removeSelected(ids) },
    )
    menu = { ...at, items }
  }

  function rowPoint(i: number): { x: number; y: number } {
    const r = viewport?.querySelector(`[aria-rowindex="${i + 1}"]`)?.getBoundingClientRect()
    return r ? { x: r.left + 48, y: r.bottom + 2 } : { x: 120, y: 120 }
  }

  function columnsMenu(e: MouseEvent) {
    e.preventDefault()
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        ...orderedColumns()
          .filter((c) => c.id !== 'title')
          .map((c) => ({
            label: c.label,
            checked: !layout.hidden.includes(c.id),
            run: () => setColumnVisible(c.id, layout.hidden.includes(c.id)),
          })),
        'separator',
        { label: 'Reorder columns…', run: () => { app.settingsSection = 'views'; app.settingsOpen = true } },
        {
          label: 'Reset widths',
          run: () => {
            layout.widths = {}
            saveLayout()
          },
        },
      ],
    }
  }

  function sortBy(col: Col) {
    if (app.sort === col.sort) app.descending = !app.descending
    else {
      app.sort = col.sort
      app.descending = !!col.firstDescending
    }
  }

  function autofocus(node: HTMLInputElement) {
    node.focus()
    node.select()
  }

  const bulkRow = $derived(selected.size > 1 ? rowById(selectedIds()[0]) : undefined)

  /** What an empty list says; "above" or "below" follows where quick add sits. */
  const empty = $derived.by(() => {
    const add = `Add one ${viewLayout.quickAdd === 'top' ? 'above' : 'below'}, or press N.`
    const showDone = { label: 'Show completed', run: () => (app.showDone = true) }
    if (filterCount(app.filter))
      return { icon: ListFilter, title: 'No tasks match the filters', body: 'Change the filters, or clear them to see the whole list.', action: { label: 'Clear filters', run: clearFilter } }
    if (app.search.trim())
      return app.showDone || app.scope.kind === 'today' || app.scope.kind === 'upcoming'
        ? { icon: Search, title: 'No matching tasks', body: 'Try a different search.', action: null }
        : { icon: Search, title: 'No matching open tasks', body: 'Try a different search, or include completed tasks.', action: { ...showDone, label: 'Include completed' } }
    if (hiddenDone) {
      const n = `${plural(hiddenDone, 'completed task')} ${hiddenDone === 1 ? 'is' : 'are'} hidden.`
      return app.scope.kind === 'today'
        ? { icon: CheckCircle2, title: 'All done for today', body: `You finished ${plural(hiddenDone, 'task')} today.`, action: null }
        : { icon: CheckCircle2, title: 'All done here', body: n, action: app.scope.kind === 'upcoming' ? null : showDone }
    }
    switch (app.scope.kind) {
      case 'inbox':
        return { icon: Inbox, title: 'Your Inbox is empty', body: `Tasks without a project land here. ${add}`, action: null }
      case 'today':
        return { icon: Star, title: 'Nothing due today', body: `Tasks due today, and overdue ones, show up here. ${add}`, action: null }
      case 'upcoming':
        return { icon: CalendarDays, title: 'Nothing due in the next 7 days', body: 'Tasks with a due date in the coming week show up here.', action: null }
      default:
        return { icon: ListTodo, title: 'No tasks yet', body: add, action: null }
    }
  })
</script>

<input
  bind:this={datePicker}
  type="date"
  class="pointer-events-none absolute size-0 opacity-0"
  tabindex="-1"
  aria-hidden="true"
  onchange={(e) => {
    const v = e.currentTarget.value
    if (v && pickFor.length) void patch(pickFor, { due_date: v })
  }}
/>

<div class="table-shell flex min-h-0 flex-1 flex-col overflow-hidden rounded-[var(--radius)] border border-border bg-surface-2">
  <!-- body -->
  <div
    bind:this={viewport}
    class="relative min-h-0 flex-1 overflow-auto outline-none"
    role="grid"
    aria-rowcount={count}
    aria-multiselectable="true"
    tabindex="0"
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
    {onkeydown}
    oncontextmenu={onGridMenu}
  >
    <!-- header (sticky, scrolls sideways with the rows) -->
    <div class="sticky top-0 z-10 grid h-9 items-center border-b border-border bg-surface-2 text-xs text-muted select-none" style="grid-template-columns: {gridTemplate}; min-width: {minWidth}px" role="row" tabindex="-1" oncontextmenu={columnsMenu}>
      <div role="columnheader" class="h-full"><button class="grid h-full w-full place-items-center text-subtle hover:text-text" title="Columns" aria-label="Choose columns" onclick={columnsMenu}><Columns3 size={14} /></button></div>
      {#each cols as col, ci (col.id)}
        <div class="relative flex h-full min-w-0 items-center px-2" role="columnheader" aria-sort={app.sort === col.sort ? (app.descending ? 'descending' : 'ascending') : 'none'}>
          <button class="flex min-w-0 items-center gap-1 font-medium hover:text-text" title="Sort by {headerLabel(col).toLowerCase()}" onclick={() => sortBy(col)}>
            <span class="truncate">{headerLabel(col)}</span>
            {#if app.sort === col.sort}
              {#if app.descending}<ArrowDown size={12} />{:else}<ArrowUp size={12} />{/if}
            {/if}
          </button>
          <!-- Focusable splitter (ARIA window-splitter pattern): ←/→ resize. -->
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <span
            class="resizer"
            class:last={ci === cols.length - 1}
            role="separator"
            tabindex="0"
            aria-orientation="vertical"
            aria-label="Resize {headerLabel(col)} column (arrow keys; Enter or double-click fits it; Home resets)"
            aria-valuemin={col.id === 'title' ? TITLE_MIN : COL_MIN}
            aria-valuemax={col.id === 'title' ? 2000 : COL_MAX}
            aria-valuenow={Math.round(col.id === 'title' && !taskSized ? TITLE_MIN : col.width)}
            title="Drag to resize · double-click to fit"
            onpointerdown={(e) => startResize(e, col)}
            ondblclick={() => autoFit(col)}
            onkeydown={(e) => resizeKey(e, col)}
          ></span>
        </div>
      {/each}
    </div>

    {#if loaded && total === 0}
      <div class="absolute inset-0 top-9 grid place-items-center px-6 text-center">
        <div class="max-w-[360px]">
          <empty.icon size={28} class="mx-auto mb-2 text-subtle" />
          <p class="font-medium">{empty.title}</p>
          <p class="text-muted">{empty.body}</p>
          {#if empty.action}
            <button class="btn mt-3" onclick={empty.action.run}>{empty.action.label}</button>
          {/if}
        </div>
      </div>
    {/if}
    <div style="height: {count * ROW}px; min-width: {minWidth}px" class="relative">
      {#each { length: last - first } as _, k (first + k)}
        {@const i = first + k}
        {@const row = rowAt(i)}
        {@const head = groupAt(i)}
        {#if head}
          <div
            class="row group-row absolute inset-x-0 flex items-center border-b border-border"
            class:cursor={i === cursor}
            style="top: {i * ROW}px; height: {ROW}px"
            role="row"
            tabindex="-1"
            aria-rowindex={i + 1}
            aria-expanded={!head.collapsed}
            onclick={() => {
              cursor = i
              toggleGroup(head.group.key)
            }}
            onkeydown={() => {}}
          >
            <div role="gridcell" class="sticky left-0 flex h-full min-w-0 items-center gap-2 px-3">
              {#if head.collapsed}<ChevronRight size={14} class="shrink-0 text-subtle" />{:else}<ChevronDown size={14} class="shrink-0 text-subtle" />{/if}
              {#if head.group.color}<span class="size-2 shrink-0 rounded-full" style="background: {head.group.color.startsWith('var') ? head.group.color : swatch(head.group.color, 0.62, 0.13)}"></span>{/if}
              <span class="truncate font-medium">{head.group.label}</span>
              <span class="tabular text-xs text-subtle">{head.group.rows.length}</span>
            </div>
          </div>
        {:else}
        <div
          class="row absolute inset-x-0 grid items-center border-b border-border"
          class:selected={row && selected.has(row.id)}
          class:cursor={i === cursor}
          class:done={row?.completed_at}
          class:overdue={app.scope.kind === 'today' && row && isOverdue(row.due_date, !!row.completed_at)}
          style="top: {i * ROW}px; height: {ROW}px; grid-template-columns: {gridTemplate}"
          role="row"
          tabindex="-1"
          aria-rowindex={i + 1}
          aria-selected={row ? selected.has(row.id) : false}
          onclick={(e) => {
            select(i, e)
            if (row && !e.ctrlKey && !e.shiftKey) app.openTaskId = row.id
          }}
          onpointerdown={(e) => {
            // Drag a task (or the selection it belongs to) onto a sidebar project.
            if (!row || (e.target as HTMLElement).closest('input, textarea, select, .resizer')) return
            const ids = targets(row)
            beginDrag(e, { kind: 'tasks', ids, label: ids.length > 1 ? plural(ids.length, 'task') : row.title })
          }}
          onkeydown={() => {}}
        >
          {#if row}
            <div role="gridcell" class="h-full">
            <button
              class="grid h-full w-full place-items-center {row.category === 'done' ? 'text-success' : row.category === 'active' ? 'text-accent' : 'text-subtle'} hover:text-accent"
              aria-label={row.completed_at ? 'Mark not done' : 'Mark done'}
              onclick={(e) => {
                e.stopPropagation()
                void toggleDone(row)
              }}
            >
              {#if row.category === 'done' || row.category === 'cancelled'}<CheckCircle2 size={17} />
              {:else if row.status_name.toLowerCase() === 'waiting'}<Hourglass size={16} aria-label="Waiting" />
              {:else if row.category === 'active'}<CircleDot size={17} />
              {:else if row.status_name === 'Backlog'}<CircleDashed size={17} />
              {:else}<Circle size={17} />{/if}
            </button>
            </div>
            {#each cols as col (col.id)}
              <div class="cell min-w-0 px-2" role="gridcell">
                {#if col.id === 'title'}
                  {#if renaming === row.id}
                    <input
                      class="h-7 w-full rounded border border-accent bg-surface-1 px-1.5 outline-none"
                      value={row.title}
                      use:autofocus
                      onclick={(e) => e.stopPropagation()}
                      onkeydown={(e) => {
                        if (e.key === 'Enter') void rename(row, e.currentTarget.value)
                        if (e.key === 'Escape') renaming = null
                      }}
                      onblur={(e) => void rename(row, e.currentTarget.value)}
                    />
                  {:else}
                    <span class="title flex items-center gap-2">
                      <!-- Mouse shortcut; keyboard users rename with F2 / E. -->
                      <!-- svelte-ignore a11y_no_static_element_interactions -->
                      <span class="truncate" ondblclick={() => (renaming = row.id)}>{row.title}</span>
                      {#if row.recurring}<Repeat size={12} class="shrink-0 text-subtle" aria-label="Repeats" />{/if}
                      {#if row.subtasks_total}<span class="tabular shrink-0 text-xs text-muted">{row.subtasks_done}/{row.subtasks_total}</span>{/if}
                      {#if row.parent_title}<span class="min-w-0 shrink truncate text-xs text-muted" title="Subtask of “{row.parent_title}”">↳ {row.parent_title}</span>{/if}
                    </span>
                  {/if}
                {:else if col.id === 'project'}
                  <button class="cell-btn" title={projectPath.get(row.project_id)} onclick={(e) => projectMenu(e, row)}>
                    <span class="size-2 shrink-0 rounded-full" style="background: {swatch(row.project_color)}"></span>
                    {#if row.client_name && !groupRepeats}<span class="truncate text-muted">{row.client_name}</span><span class="text-subtle">/</span>{/if}
                    <span class="truncate">{row.project_name}</span>
                  </button>
                {:else if col.id === 'status'}
                  <button class="cell-btn text-muted" onclick={(e) => void statusMenu(e, row)}>
                    <span class="size-1.5 shrink-0 rounded-full" style="background: {swatch(row.status_color, 0.62, 0.13)}"></span>
                    <span class="truncate">{row.status_name}</span>
                  </button>
                {:else if col.id === 'priority'}
                  <button class="cell-btn text-muted" onclick={(e) => priorityMenu(e, row)}>
                    {#if row.priority !== 'none'}
                      <span class="size-1.5 shrink-0 rounded-full" style="background: var(--priority-{row.priority})"></span>
                      {priorityLabel[row.priority]}
                    {:else}<span class="ghost">—</span>{/if}
                  </button>
                {:else if col.id === 'due'}
                  <button class="cell-btn tabular {isOverdue(row.due_date, !!row.completed_at) ? 'font-medium text-danger' : 'text-muted'}" onclick={(e) => dueMenu(e, row)}>
                    {#if row.due_date}{dueLabel(row.due_date)}{#if row.due_time}<span class="text-subtle">&nbsp;{row.due_time}</span>{/if}{:else}<span class="ghost">—</span>{/if}
                  </button>
                {:else if col.id === 'progress'}
                  <button class="cell-btn w-full" title={row.subtasks_total ? `From ${row.subtasks_done}/${row.subtasks_total} subtasks` : 'Set progress'} onclick={(e) => progressMenu(e, row)}>
                    <!-- Only when it means something: subtasks, or progress someone set. -->
                    {#if row.subtasks_total || row.progress}<Progress value={row.progress} width={52} label />{:else}<span class="ghost">—</span>{/if}
                  </button>
                {:else if col.id === 'estimate'}
                  {#if estimating === row.id}
                    <input
                      class="h-7 w-full rounded border border-accent bg-surface-1 px-1.5 outline-none"
                      value={row.estimate_minutes ? minutes(row.estimate_minutes) : ''}
                      placeholder="1h30m"
                      use:autofocus
                      onclick={(e) => e.stopPropagation()}
                      onkeydown={(e) => {
                        if (e.key === 'Escape') estimating = null
                        if (e.key !== 'Enter') return
                        const m = parseMinutes(e.currentTarget.value)
                        estimating = null
                        if (Number.isNaN(m)) toast('Use a duration like 45m, 2h or 1h30m', { kind: 'error' })
                        else void patch(targets(row), { estimate_minutes: m })
                      }}
                      onblur={() => (estimating = null)}
                    />
                  {:else}
                    <button class="cell-btn tabular text-muted" onclick={(e) => { e.stopPropagation(); estimating = row.id }}>
                      {#if row.estimate_minutes}{minutes(row.estimate_minutes)}{:else}<span class="ghost">—</span>{/if}
                    </button>
                  {/if}
                {:else if col.id === 'tags'}
                  <span class="flex gap-1 overflow-hidden">
                    {#each row.tags as tag (tag)}<span class="tag">#{tag}</span>{/each}
                  </span>
                {:else if col.id === 'added'}
                  <span class="tabular block truncate text-muted" title="Added {addedTitle(row.created_at)}">{addedLabel(row.created_at)}</span>
                {/if}
              </div>
            {/each}
          {:else}
            <div></div>
            <div class="mx-2 h-3 w-1/2 rounded bg-surface-3"></div>
          {/if}
        </div>
        {/if}
      {/each}
    </div>
  </div>

  <!-- footer -->
  <div class="flex h-9 shrink-0 items-center gap-3 border-t border-border px-3 text-xs text-muted">
    {#if selected.size > 1 && bulkRow}
      <span class="font-medium text-text">{selected.size} selected</span>
      <button class="bulk" onclick={() => void act(() => ipc.bulkSetDone(selectedIds(), true), `Completed ${plural(selected.size, 'task')}`)}>Complete</button>
      <button class="bulk" onclick={(e) => dueMenu(e, bulkRow)}>Due date</button>
      <button class="bulk" onclick={(e) => priorityMenu(e, bulkRow)}>Priority</button>
      <button class="bulk" onclick={(e) => projectMenu(e, bulkRow)}>Move to</button>
      <button class="bulk" title="Hide them from every list; restore from Settings → Data (A)" onclick={() => void archiveSelected()}><Archive size={13} /> Archive</button>
      <button class="bulk text-danger" onclick={() => void removeSelected()}><Trash2 size={13} /> Delete</button>
      <button class="bulk ml-auto" onclick={() => selected.clear()}><X size={13} /> Clear</button>
    {:else if summary}
      <span class="tabular">{plural(summary.total, 'task')}</span>
      {#if grouped && total > GROUP_CAP}
        <span class="text-subtle" title="Narrow the list with a filter or search to group the rest">· grouping the first {GROUP_CAP.toLocaleString()}</span>
      {/if}
      {#if summary.total && app.showDone}
        <span aria-hidden="true">·</span>
        <span class="flex items-center gap-2">
          <span class="progress"><span style="width: {(summary.done / summary.total) * 100}%"></span></span>
          <span class="tabular">{Math.round((summary.done / summary.total) * 100)}% done</span>
        </span>
      {/if}
      {#if summary.estimate_minutes}
        <span aria-hidden="true">·</span>
        <span class="tabular">{minutes(summary.estimate_minutes)} estimated</span>
      {/if}
      <span class="ml-auto flex min-w-0 items-center gap-3">
        <span class="hidden truncate text-subtle {completedHere ? '2xl:inline' : 'xl:inline'}">↑↓ move · Enter open · F2 rename · Space done · A archive · Del delete</span>
        {#if completedHere}
          <button class="bulk shrink-0" title="Hide this list's completed tasks; restore them from Settings → Data" onclick={() => void archiveCompleted(JSON.parse(queryKey))}>
            <Archive size={13} /> Archive {completedHere} completed
          </button>
        {/if}
      </span>
    {/if}
  </div>
</div>

{#if menu}
  <Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .row {
    transition: background-color 80ms var(--ease-out);
  }
  .row:hover {
    background: var(--hover);
  }
  .row.selected {
    background: var(--accent-soft);
  }
  .group-row {
    background: var(--surface-1);
    cursor: pointer;
  }
  /* Dimmed details (a due time, the group / project slash) stay readable on
     the selection color. */
  .row.selected .text-subtle {
    color: var(--text-muted);
  }
  .row.cursor {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .row.done .title {
    color: var(--text-subtle);
    text-decoration: line-through;
    text-decoration-color: var(--border-strong);
  }
  .cell-btn {
    display: flex;
    max-width: 100%;
    align-items: center;
    gap: 0.4rem;
    height: 26px;
    padding-inline: 0.35rem;
    margin-inline: -0.35rem;
    border-radius: 5px;
  }
  .cell-btn:hover {
    background: var(--surface-3);
  }
  /* Empty cells stay blank; the dash only appears on the row being pointed at
     or selected, to show the cell can be set. */
  .ghost {
    color: transparent;
  }
  .row:hover .ghost,
  .row.selected .ghost,
  .row.cursor .ghost {
    color: var(--text-subtle);
  }
  /* Today: overdue rows carry a thin red rail, so the two groups read apart. */
  .row.overdue {
    box-shadow: inset 3px 0 0 color-mix(in oklch, var(--danger) 75%, transparent);
  }
  .tag {
    flex-shrink: 0;
    border-radius: 4px;
    padding: 0 5px;
    font-size: 11.5px;
    color: var(--text-muted);
    background: var(--surface-3);
  }
  .resizer {
    position: absolute;
    top: 4px;
    bottom: 4px;
    right: -4px;
    z-index: 1;
    width: 9px;
    cursor: col-resize;
    touch-action: none;
    outline: none;
  }
  /* The last column's handle stays inside the table (no sideways scroll). */
  .resizer.last {
    right: 0;
  }
  /* The visible line sits on the column edge; the 9px handle straddles it. */
  .resizer::after {
    content: '';
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 4px;
    width: 1px;
    background: var(--border);
  }
  .resizer:hover::after,
  .resizer:focus-visible::after,
  :global(body.col-resizing) .resizer:active::after {
    left: 3px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  :global(body.col-resizing) {
    cursor: col-resize;
    user-select: none;
  }
  .bulk {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    height: 24px;
    padding-inline: 0.5rem;
    border-radius: 5px;
    border: 1px solid var(--border);
    color: var(--text);
  }
  .bulk:hover {
    background: var(--hover);
  }
  .progress {
    display: inline-block;
    width: 64px;
    height: 5px;
    border-radius: 99px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--success);
  }
</style>
