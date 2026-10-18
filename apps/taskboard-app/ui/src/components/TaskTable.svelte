<script lang="ts">
  import { beginDrag } from '../lib/dnd.svelte'
  import { Archive, ArrowDown, ArrowUp, CalendarDays, CheckCircle2, Circle, CircleDot, CircleDashed, Columns3, Hourglass, Inbox, ListTodo, Repeat, Search, Star, Trash2, X } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import type { Id } from '../lib/bindings/Id'
  import type { Priority } from '../lib/bindings/Priority'
  import type { Status } from '../lib/bindings/Status'
  import type { TaskPatch } from '../lib/bindings/TaskPatch'
  import type { TaskQuery } from '../lib/bindings/TaskQuery'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import type { TaskSort } from '../lib/bindings/TaskSort'
  import type { TaskSummary } from '../lib/bindings/TaskSummary'
  import { dueLabel, isOverdue, isTyping, isoDate, minutes, parseMinutes, plural, swatch, todayIso } from '../lib/format'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { act, app, archiveCompleted, scopeQuery, sectionById } from '../lib/store.svelte'
  import { layout as viewLayout } from '../lib/layout.svelte'
  import { toast } from '../lib/toast.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'
  import { COLUMN_DEFS, orderedColumns, saveTableLayout, setColumnVisible, tableLayout, type ColId, type ColumnDef } from '../lib/tableLayout.svelte'
  import Progress from './Progress.svelte'

  const ROW = 36
  const PAGE = 200
  const OVERSCAN = 12

  // ---------- columns ----------
  // Order, visibility and widths are shared with Settings → Layout.
  type Col = ColumnDef
  const layout = tableLayout
  const saveLayout = saveTableLayout
  /** Auto-hidden first when the table is too narrow (user-hidden ones stay hidden). */
  const TITLE_MIN = 180
  const SQUEEZE_ORDER: ColId[] = ['tags', 'estimate', 'progress', 'priority', 'status']
  let width = $state(1200)
  /** While a column edge is being dragged, the columns on screen stay as they
   *  were (none appears or vanishes under the pointer); they re-fit on release. */
  let frozen = $state<ColId[] | null>(null)
  const cols = $derived.by(() => {
    const withWidth = (c: ColumnDef) => ({ ...c, width: layout.widths[c.id] ?? c.width })
    if (frozen) {
      const ids = frozen
      return orderedColumns().filter((c) => ids.includes(c.id)).map(withWidth)
    }
    let list = orderedColumns().filter((c) => c.id === 'title' || !layout.hidden.includes(c.id)).map(withWidth)
    // Decide with default widths: which columns fit depends on the window, never
    // on how the user sized them, so resizing a column can't make another one
    // appear or vanish (a wider column scrolls the table; room freed goes to Task).
    const fit = (c: Col) => COLUMN_DEFS.find((d) => d.id === c.id)?.width ?? c.width
    const need = () => 36 + TITLE_MIN + list.reduce((sum, c) => sum + (c.id === 'title' ? 0 : fit(c)), 0)
    for (const id of SQUEEZE_ORDER) {
      if (need() <= width) break
      list = list.filter((c) => c.id !== id)
    }
    return list
  })
  /** Task fills the leftover space but never shrinks below this; a resize that
   *  runs out of room on the right raises it, so the table scrolls instead. */
  const titleMin = $derived(Math.max(TITLE_MIN, layout.widths.title ?? 0))
  const minWidth = $derived(36 + titleMin + cols.reduce((sum, c) => sum + (c.id === 'title' ? 0 : c.width), 0))
  const gridTemplate = $derived(
    `36px ${cols.map((c) => (c.id === 'title' ? `minmax(${titleMin}px, 1fr)` : `${c.width}px`)).join(' ')}`,
  )

  const COL_MIN = 70
  const COL_MAX = 600

  /**
   * What a column's right-edge handle changes, so the edge follows the mouse.
   * The Task column fills the remaining space, so:
   * - Task's own handle takes width from (or gives it to) the columns to its right;
   * - left of Task, a column just grows and Task gives way;
   * - right of Task, the column trades width with the columns to its right
   *   (nearest first), so Task and everything to the left stay put.
   */
  type Resize = { target: Col; sign: 1 | -1; after?: Col[] }
  function resizeTarget(col: Col): Resize | null {
    // By id: `cols` is rebuilt on every width change, so a handle's own
    // column object goes stale between key presses.
    const i = cols.findIndex((c) => c.id === col.id)
    if (i < 0) return null
    col = cols[i]
    const next = cols[i + 1]
    if (col.id === 'title') return next ? { target: col, sign: 1, after: cols.slice(i + 1) } : null
    if (i < cols.findIndex((c) => c.id === 'title')) return { target: col, sign: 1 }
    return next ? { target: col, sign: 1, after: cols.slice(i + 1) } : null
  }

  function setWidth(id: ColId, w: number) {
    layout.widths[id] = Math.round(Math.max(COL_MIN, Math.min(COL_MAX, w)))
  }

  /**
   * Moves a handle by `dx` pixels from the starting widths (target, the columns
   * after it, and Task's pixel width). Growing takes room from the columns to
   * the right first; once they are at their minimum the column keeps growing
   * and Task is held at its width, so the table scrolls sideways rather than
   * the edge stopping under the mouse.
   */
  function applyResize(r: Resize, startA: number, startAfter: number[], taskPx: number, dx: number) {
    const fill = r.target.id === 'title'
    // Task never goes below its minimum; other columns stay within their limits.
    let d = fill ? Math.max(TITLE_MIN - taskPx, dx) : Math.max(COL_MIN - startA, Math.min(COL_MAX - startA, dx * r.sign))
    if (r.after) {
      if (d > 0) {
        // Growing takes room from the columns to the right, nearest first.
        const room = startAfter.reduce((sum, w) => sum + Math.max(0, w - COL_MIN), 0)
        let left = Math.min(d, room)
        r.after.forEach((c, k) => {
          const give = Math.min(left, Math.max(0, startAfter[k] - COL_MIN))
          left -= give
          setWidth(c.id, startAfter[k] - give)
        })
      } else {
        // Shrinking: the next column takes the space.
        d = Math.max(d, -(COL_MAX - startAfter[0]))
        r.after.forEach((c, k) => setWidth(c.id, k === 0 ? startAfter[0] - d : startAfter[k]))
      }
      // Task's own edge moves by the whole drag (the table scrolls if it must);
      // right of Task, Task and everything to its left stay put.
      layout.widths.title = fill ? Math.max(TITLE_MIN, taskPx + d) : Math.max(TITLE_MIN, taskPx)
    }
    if (!fill) setWidth(r.target.id, startA + d)
  }

  /** Task's pixel width right now, for freezing it during a resize. */
  function taskWidth(): number {
    return viewport?.querySelector<HTMLElement>('[role="columnheader"]:nth-child(2)')?.getBoundingClientRect().width ?? TITLE_MIN
  }

  /** After a resize: Task's floor only matters while the table overflows. */
  function settleTitleMin() {
    const floor = layout.widths.title
    if (floor && 36 + floor + cols.reduce((sum, c) => sum + (c.id === 'title' ? 0 : c.width), 0) <= width) delete layout.widths.title
  }

  function startResize(e: PointerEvent, col: Col) {
    if (e.button !== 0) return
    const r = resizeTarget(col)
    if (!r) return
    e.preventDefault()
    e.stopPropagation()
    const handle = e.currentTarget as HTMLElement
    handle.setPointerCapture(e.pointerId)
    document.body.classList.add('col-resizing')
    const startX = e.clientX
    const startA = r.target.width
    const startAfter = (r.after ?? []).map((c) => c.width)
    const taskPx = taskWidth()
    frozen = cols.map((c) => c.id)
    const move = (ev: PointerEvent) => applyResize(r, startA, startAfter, taskPx, ev.clientX - startX)
    const up = () => {
      handle.removeEventListener('pointermove', move)
      handle.removeEventListener('pointerup', up)
      handle.removeEventListener('pointercancel', up)
      document.body.classList.remove('col-resizing')
      frozen = null
      settleTitleMin()
      saveLayout()
    }
    handle.addEventListener('pointermove', move)
    handle.addEventListener('pointerup', up)
    handle.addEventListener('pointercancel', up)
  }

  /** Keyboard: ← / → resize by 16px; Home resets. */
  function resizeKey(e: KeyboardEvent, col: Col) {
    const r = resizeTarget(col)
    if (!r) return
    const step = e.key === 'ArrowLeft' ? -16 : e.key === 'ArrowRight' ? 16 : 0
    // Read the saved widths: repeated keys can arrive before the next render.
    const a = layout.widths[r.target.id] ?? r.target.width
    const after = (r.after ?? []).map((c) => layout.widths[c.id] ?? c.width)
    if (step) applyResize(r, a, after, taskWidth(), step)
    else if (e.key === 'Home') delete layout.widths[r.target.id === 'title' && r.after ? r.after[0].id : r.target.id]
    else return
    e.preventDefault()
    e.stopPropagation()
    settleTitleMin()
    saveLayout()
  }

  /** Double-click a handle: back to the default width. */
  function resetWidth(col: Col) {
    const r = resizeTarget(col)
    if (!r) return
    delete layout.widths[r.target.id]
    if (r.after?.[0]) delete layout.widths[r.after[0].id]
    delete layout.widths.title
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

  const queryKey = $derived(JSON.stringify(scopeQuery()))
  /** The sticky header sits inside the scroller, above row 0. */
  const HEADER = 36
  const first = $derived(Math.max(0, Math.floor((scrollTop - HEADER) / ROW) - OVERSCAN))
  const last = $derived(Math.min(total, Math.ceil((scrollTop + height) / ROW) + OVERSCAN))

  function rowAt(i: number): TaskRow | undefined {
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

  function visiblePages(): number[] {
    const from = Math.floor(first / PAGE)
    const to = Math.floor(Math.max(first, last - 1) / PAGE)
    return Array.from({ length: to - from + 1 }, (_, k) => from + k)
  }

  // New query → start over at the top. Depends on `queryKey` only.
  $effect(() => {
    void queryKey
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
      void fetchPage(0, generation)
    })
  })

  // Data changed elsewhere → refresh what's on screen without clearing it.
  // Depends on `dataVersion` only; untracked so loading pages can't re-trigger it.
  $effect(() => {
    if (!app.dataVersion) return
    untrack(() => {
      generation++
      const keep = new Set(visiblePages())
      for (const p of [...pages.keys()]) if (!keep.has(p)) pages.delete(p)
      inflight.clear()
      for (const p of keep) void fetchPage(p, generation)
    })
  })

  // Scrolling into unloaded pages.
  $effect(() => {
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
    if (!row) return
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
      const i = Math.max(0, Math.min(total - 1, (cursor < 0 ? -1 : cursor) + d))
      select(i, e.shiftKey ? e : undefined)
      scrollIntoView(i)
      if (app.openTaskId) app.openTaskId = rowAt(i)?.id ?? null
    }
    const current = rowAt(cursor)
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
          for (const rows of pages.values()) for (const r of rows) selected.add(r.id)
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
  const statusCache = new SvelteMap<Id, Status[]>()
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
    let statuses = statusCache.get(row.project_id)
    if (!statuses) {
      statuses = await ipc.listStatuses(row.project_id)
      statusCache.set(row.project_id, statuses)
    }
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
    if (!col.sort) return
    if (app.sort === col.sort) app.descending = !app.descending
    else {
      app.sort = col.sort
      app.descending = false
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
    aria-rowcount={total}
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
        <div class="relative flex h-full items-center px-2 {col.align === 'right' ? 'justify-end' : ''}" role="columnheader" aria-sort={app.sort === col.sort ? (app.descending ? 'descending' : 'ascending') : 'none'}>
          <button class="flex items-center gap-1 font-medium {col.sort ? 'hover:text-text' : 'cursor-default'}" onclick={() => sortBy(col)}>
            {col.label}
            {#if col.sort && app.sort === col.sort}
              {#if app.descending}<ArrowDown size={12} />{:else}<ArrowUp size={12} />{/if}
            {/if}
          </button>
          {#if resizeTarget(col)}
            <!-- Focusable splitter (ARIA window-splitter pattern): ←/→ resize. -->
            <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
            <span
              class="resizer"
              class:last={ci === cols.length - 1}
              role="separator"
              tabindex="0"
              aria-orientation="vertical"
              aria-label="Resize {col.label} column (arrow keys; double-click to reset)"
              aria-valuemin={COL_MIN}
              aria-valuemax={COL_MAX}
              aria-valuenow={col.id === 'title' ? (cols[ci + 1]?.width ?? 0) : col.width}
              title="Drag to resize · double-click to reset"
              onpointerdown={(e) => startResize(e, col)}
              ondblclick={() => resetWidth(col)}
              onkeydown={(e) => resizeKey(e, col)}
            ></span>
          {/if}
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
    <div style="height: {total * ROW}px; min-width: {minWidth}px" class="relative">
      {#each { length: last - first } as _, k (first + k)}
        {@const i = first + k}
        {@const row = rowAt(i)}
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
              <div class="cell min-w-0 px-2 {col.align === 'right' ? 'text-right' : ''}" role="gridcell">
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
                    {#if row.client_name}<span class="truncate text-muted">{row.client_name}</span><span class="text-subtle">/</span>{/if}
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
                      class="h-7 w-full rounded border border-accent bg-surface-1 px-1.5 text-right outline-none"
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
                    <button class="cell-btn tabular w-full justify-end text-muted" onclick={(e) => { e.stopPropagation(); estimating = row.id }}>
                      {#if row.estimate_minutes}{minutes(row.estimate_minutes)}{:else}<span class="ghost">—</span>{/if}
                    </button>
                  {/if}
                {:else if col.id === 'tags'}
                  <span class="flex gap-1 overflow-hidden">
                    {#each row.tags as tag (tag)}<span class="tag">#{tag}</span>{/each}
                  </span>
                {/if}
              </div>
            {/each}
          {:else}
            <div></div>
            <div class="mx-2 h-3 w-1/2 rounded bg-surface-3"></div>
          {/if}
        </div>
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
