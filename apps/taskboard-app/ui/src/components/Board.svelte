<script lang="ts">
  import { endExternal, hoverExternal } from '../lib/dnd.svelte'
  import { untrack } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { ChevronDown, ChevronRight, ChevronsLeft, CheckCircle2, Circle, MoreHorizontal, Plus, Repeat } from '@lucide/svelte'
  import type { Category } from '../lib/bindings/Category'
  import type { Id } from '../lib/bindings/Id'
  import type { Priority } from '../lib/bindings/Priority'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import { buildColumns, buildLanes, laneKey, type Column, type SectionOf } from '../lib/board'
  import { dueLabel, isOverdue, minutes, PALETTE, swatch, todayIso } from '../lib/format'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { act, app, archiveCompleted, inboxId, listQuery, scopeProjectId, scopeProjectIds, sectionById, type Lanes } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'
  import Progress from './Progress.svelte'
  import { segmented } from '../lib/segmented'

  const CARD_LIMIT = 100
  const COLLAPSE_KEY = 'taskboard.board.collapsed'

  // ---------- columns & data ----------
  /** Every column of the projects in view (column actions work on these). */
  const allColumns = $derived(buildColumns(app.statuses, scopeProjectIds()))
  /** The columns shown: all, or those picked in Filter → Status. */
  const columns = $derived(
    app.filter.statuses.length ? allColumns.filter((c) => app.filter.statuses.some((s) => s.trim().toLowerCase() === c.key)) : allColumns,
  )
  const colKeys = $derived(columns.map((c) => c.key).join('|'))
  const singleProject = $derived(allColumns.length > 0 && allColumns[0].status !== null)
  /** Cards follow a sort instead of the order they were dragged into. */
  const sorted = $derived(app.boardSort !== 'manual')
  const baseKey = $derived(
    JSON.stringify({
      ...listQuery(),
      // Every column (done ones too), unless Overdue limits it to open work.
      categories: app.filter.due === 'overdue' ? ['todo', 'active'] : [],
      sort: app.boardSort,
      descending: app.boardDescending,
      top_level: !app.boardSubtasks,
    }),
  )

  const data = new SvelteMap<string, { rows: TaskRow[]; total: number }>()
  const limits = new SvelteMap<string, number>()
  let generation = 0
  let pendingReload = false

  async function load() {
    if (!inTauri) return
    const gen = ++generation
    const base = JSON.parse(baseKey)
    const cols = columns
    const results = await Promise.all(
      cols.map((c) =>
        ipc.listTasks({ ...base, status_name: c.name, limit: limits.get(c.key) ?? CARD_LIMIT }).catch((err) => {
          toast(errorMessage(err), { kind: 'error' })
          return null
        }),
      ),
    )
    if (gen !== generation) return
    cols.forEach((c, i) => {
      const r = results[i]
      if (r) data.set(c.key, { rows: r.rows, total: r.total })
    })
    for (const k of [...data.keys()]) if (!cols.some((c) => c.key === k)) data.delete(k)
  }

  $effect(() => {
    void baseKey
    void colKeys
    untrack(() => void load())
  })

  $effect(() => {
    if (!app.dataVersion) return
    untrack(() => {
      // Don't reshuffle cards under the pointer; reload after the drop.
      if (drag || kbd) pendingReload = true
      else void load()
    })
  })

  function loadMore(col: Column) {
    limits.set(col.key, (limits.get(col.key) ?? CARD_LIMIT) + CARD_LIMIT)
    void load()
  }

  // ---------- lanes & collapse ----------
  const allRows = $derived([...data.values()].flatMap((d) => d.rows))
  const projectSection = $derived(new Map(app.projects.map((p) => [p.id, sectionById(p.section_id)])))
  const sectionOf: SectionOf = (projectId) => projectSection.get(projectId)
  const lanes = $derived(buildLanes(allRows, app.lanes, sectionOf))
  let laneCollapsed = $state<Record<string, boolean>>({})

  function loadCollapsed(): Record<string, boolean> {
    try {
      return JSON.parse(localStorage.getItem(COLLAPSE_KEY) ?? '{}')
    } catch {
      return {}
    }
  }
  let collapsed = $state<Record<string, boolean>>(loadCollapsed())
  function toggleColumn(key: string) {
    collapsed[key] = !collapsed[key]
    try {
      localStorage.setItem(COLLAPSE_KEY, JSON.stringify(collapsed))
    } catch {
      // per-device convenience only
    }
  }

  function cellRows(colKey: string, lane: string): TaskRow[] {
    const rows = data.get(colKey)?.rows ?? []
    return app.lanes === 'none' ? rows : rows.filter((r) => laneKey(r, app.lanes, sectionOf) === lane)
  }

  function columnOf(id: Id): string | undefined {
    for (const [key, d] of data) if (d.rows.some((r) => r.id === id)) return key
    return undefined
  }

  // ---------- drag & drop (pointer) ----------
  interface Drag {
    row: TaskRow
    x: number
    y: number
    offX: number
    offY: number
    width: number
  }
  let drag = $state<Drag | null>(null)
  let over = $state<{ col: string; lane: string; index: number } | null>(null)
  let pending: { row: TaskRow; x: number; y: number; el: HTMLElement } | null = null
  let board: HTMLDivElement | undefined = $state()
  let raf = 0

  function onCardPointerDown(e: PointerEvent, row: TaskRow) {
    if (e.button !== 0 || kbd) return
    pending = { row, x: e.clientX, y: e.clientY, el: e.currentTarget as HTMLElement }
    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', onPointerUp)
  }

  function onPointerMove(e: PointerEvent) {
    if (pending && !drag) {
      if (Math.hypot(e.clientX - pending.x, e.clientY - pending.y) < 5) return
      const r = pending.el.getBoundingClientRect()
      drag = { row: pending.row, x: e.clientX, y: e.clientY, offX: e.clientX - r.left, offY: e.clientY - r.top, width: r.width }
      document.body.classList.add('dragging')
      raf = requestAnimationFrame(autoScroll)
    }
    if (drag) {
      drag.x = e.clientX
      drag.y = e.clientY
      hitTest()
      // Over the sidebar: a project there can take the card.
      if (!over) hoverExternal({ kind: 'tasks', ids: [drag.row.id], label: drag.row.title }, drag.x, drag.y)
      else endExternal(false)
    }
  }

  function endPointer() {
    window.removeEventListener('pointermove', onPointerMove)
    window.removeEventListener('pointerup', onPointerUp)
    cancelAnimationFrame(raf)
    document.body.classList.remove('dragging')
  }

  function onPointerUp() {
    endPointer()
    if (drag) {
      const row = drag.row
      const target = over
      drag = null
      over = null
      if (target) {
        endExternal(false)
        void commitMove(row, target)
      } else if (!endExternal(true)) flushReload()
    } else if (pending) {
      app.openTaskId = pending.row.id
    }
    pending = null
  }

  function cancelDrag() {
    endExternal(false)
    endPointer()
    drag = null
    over = null
    pending = null
    flushReload()
  }

  function flushReload() {
    if (pendingReload) {
      pendingReload = false
      void load()
    }
  }

  function hitTest() {
    if (!drag) return
    const cell = document
      .elementsFromPoint(drag.x, drag.y)
      .find((el): el is HTMLElement => el instanceof HTMLElement && !!el.dataset.cell)
    if (!cell) {
      over = null
      return
    }
    const cards = [...cell.querySelectorAll<HTMLElement>('[data-card]')].filter((c) => c.dataset.card !== drag?.row.id)
    let index = cards.length
    for (let i = 0; i < cards.length; i++) {
      const r = cards[i].getBoundingClientRect()
      if (drag.y < r.top + r.height / 2) {
        index = i
        break
      }
    }
    over = { col: cell.dataset.col ?? '', lane: cell.dataset.lane ?? 'all', index }
  }

  /** Scrolls the board while a card is held near its edges. */
  function autoScroll() {
    if (!drag || !board) return
    const r = board.getBoundingClientRect()
    const edge = 56
    const speed = (d: number) => Math.ceil(((edge - d) / edge) * 18)
    if (drag.x < r.left + edge) board.scrollLeft -= speed(drag.x - r.left)
    else if (drag.x > r.right - edge) board.scrollLeft += speed(r.right - drag.x)
    if (drag.y < r.top + edge) board.scrollTop -= speed(drag.y - r.top)
    else if (drag.y > r.bottom - edge) board.scrollTop += speed(r.bottom - drag.y)
    hitTest()
    raf = requestAnimationFrame(autoScroll)
  }

  // ---------- moving (shared by pointer and keyboard) ----------
  async function commitMove(row: TaskRow, target: { col: string; lane: string; index: number }) {
    const col = columns.find((c) => c.key === target.col)
    const fromKey = columnOf(row.id)
    if (!col || !fromKey) return flushReload()
    const statusId = col.statusIds.get(row.project_id)
    if (!statusId) {
      toast(`“${row.project_name}” has no “${col.name}” column`, { kind: 'error' })
      return flushReload()
    }
    const list = cellRows(target.col, target.lane).filter((r) => r.id !== row.id)
    const above = list[target.index - 1] ?? null
    const below = list[target.index] ?? null
    const priorityChange =
      app.lanes === 'priority' && target.lane !== row.priority ? (target.lane as Priority) : null

    // A sorted column decides the order itself: a card can change column, not place.
    if (sorted && statusId === row.status_id && !priorityChange) {
      toast('Cards are sorted, so they keep their place. To arrange them by hand, choose Sort → Board order.')
      return flushReload()
    }

    // Nothing to do if it lands where it already is.
    const source = cellRows(fromKey, laneKey(row, app.lanes, sectionOf))
    const at = source.findIndex((r) => r.id === row.id)
    if (
      statusId === row.status_id &&
      !priorityChange &&
      (source[at - 1]?.id ?? null) === (above?.id ?? null) &&
      (source[at + 1]?.id ?? null) === (below?.id ?? null)
    ) {
      return flushReload()
    }

    // Optimistic: move the card locally, then persist.
    const status = app.statuses.find((s) => s.id === statusId)
    const category: Category = status?.category ?? col.category
    const closed = category === 'done' || category === 'cancelled'
    const moved: TaskRow = {
      ...row,
      status_id: statusId,
      status_name: status?.name ?? col.name,
      status_color: status?.color ?? col.color,
      category,
      completed_at: closed ? (row.completed_at ?? Date.now()) : null,
      priority: priorityChange ?? row.priority,
    }
    const from = data.get(fromKey)
    if (from) data.set(fromKey, { rows: from.rows.filter((r) => r.id !== row.id), total: from.total - 1 })
    const to = data.get(target.col) ?? { rows: [], total: 0 }
    const rows = to.rows.slice()
    let pos = rows.length
    if (below) pos = Math.max(0, rows.findIndex((r) => r.id === below.id))
    else if (above) pos = rows.findIndex((r) => r.id === above.id) + 1
    rows.splice(pos, 0, moved)
    data.set(target.col, { rows, total: to.total + 1 })

    try {
      if (sorted && statusId === row.status_id) {
        // Sorted, same column: only the lane (priority) changed; the hand-made
        // order stays as it was, and it's one undo step.
        await ipc.updateTask(row.id, { priority: priorityChange ?? row.priority })
      } else {
        await ipc.moveTask(row.id, sorted ? { status_id: statusId } : { status_id: statusId, above: above?.id, below: below?.id })
        if (priorityChange) await ipc.updateTask(row.id, { priority: priorityChange })
      }
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
      pendingReload = true
    }
    flushReload()
  }

  // ---------- keyboard moving ----------
  let kbd = $state<{ row: TaskRow } | null>(null)
  let announcement = $state('')
  const openCols = $derived(columns.filter((c) => !collapsed[c.key]))

  let boardWidth = $state(0)
  $effect(() => {
    if (!board) return
    const el = board
    const ro = new ResizeObserver(() => (boardWidth = el.clientWidth))
    ro.observe(el)
    return () => ro.disconnect()
  })
  /** Column width: the free width split between open columns, 208–480 px,
   *  so a wide window is filled rather than left empty on the right. */
  const colWidth = $derived.by(() => {
    if (!boardWidth || !openCols.length) return 260
    const gap = 12
    const extras = (columns.length - openCols.length) * (44 + gap) + (singleProject ? 200 + gap : 0)
    const free = boardWidth - extras - gap * (openCols.length - 1) - 4
    return Math.max(208, Math.min(480, Math.floor(free / openCols.length)))
  })

  function onCardKeydown(e: KeyboardEvent, row: TaskRow, colKey: string, lane: string) {
    if (kbd) return
    if (e.key === 'Enter') {
      e.preventDefault()
      e.stopPropagation()
      app.openTaskId = row.id
    } else if (e.key === ' ') {
      e.preventDefault()
      // Stop here, or the window handler would treat this Space as the drop.
      e.stopPropagation()
      const index = cellRows(colKey, lane).findIndex((r) => r.id === row.id)
      kbd = { row }
      over = { col: colKey, lane, index }
      announce(`Picked up “${row.title}”. Arrow keys move it, Space drops, Escape cancels.`)
    }
  }

  /**
   * Keeps keyboard focus on a card after a drop: the card re-renders in its new
   * column (possibly more than once while data reloads). Stops early if the
   * user moves focus somewhere else.
   */
  function refocusCard(id: string) {
    let frames = 0
    const step = () => {
      const card = document.querySelector<HTMLElement>(`[data-card="${id}"]`)
      const active = document.activeElement
      const lost = !active || active === document.body
      if (card && lost) card.focus()
      else if (!lost && active !== card) return
      if (++frames < 40) requestAnimationFrame(step)
    }
    requestAnimationFrame(step)
  }

  function announce(text: string) {
    announcement = text
  }

  function describe(target: { col: string; lane: string; index: number }) {
    const col = columns.find((c) => c.key === target.col)
    const n = cellRows(target.col, target.lane).filter((r) => r.id !== kbd?.row.id).length
    return `${col?.name ?? ''}, position ${target.index + 1} of ${n + 1}`
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (drag && e.key === 'Escape') {
      e.preventDefault()
      cancelDrag()
      return
    }
    if (!kbd || !over) return
    const target = over
    const colIndex = openCols.findIndex((c) => c.key === target.col)
    const size = (col: string) => cellRows(col, target.lane).filter((r) => r.id !== kbd?.row.id).length
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault()
      const next = openCols[colIndex + (e.key === 'ArrowLeft' ? -1 : 1)]
      if (!next) return
      over = { col: next.key, lane: target.lane, index: Math.min(target.index, size(next.key)) }
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault()
      const index = Math.max(0, Math.min(size(target.col), target.index + (e.key === 'ArrowUp' ? -1 : 1)))
      over = { ...target, index }
    } else if (e.key === ' ' || e.key === 'Enter') {
      e.preventDefault()
      const row = kbd.row
      kbd = null
      over = null
      announce(`Dropped “${row.title}” in ${describe(target)}.`)
      void commitMove(row, target)
      refocusCard(row.id)
      return
    } else if (e.key === 'Escape') {
      e.preventDefault()
      announce(`Cancelled moving “${kbd.row.title}”.`)
      kbd = null
      over = null
      flushReload()
      return
    } else return
    if (over) announce(describe(over))
  }

  /** Card list for a cell with the drop-indicator slot index for each card. */
  function slots(rows: TaskRow[]): { row: TaskRow; slot: number }[] {
    const moving = drag?.row.id ?? null
    let k = 0
    return rows.map((row) => ({ row, slot: row.id === moving ? -1 : k++ }))
  }

  function indicatorAt(colKey: string, lane: string, slot: number): boolean {
    if (sorted) return false
    return !!over && over.col === colKey && over.lane === lane && over.index === slot
  }

  // ---------- inline add ----------
  let adding = $state<{ col: string; lane: string } | null>(null)

  async function addInColumn(col: Column, lane: string, text: string) {
    const line = text.trim()
    if (!line) return
    try {
      const p = await ipc.previewQuickAdd(line)
      if (p.project_missing) {
        toast(`No project matches “@${p.parsed.project_hint}”`, { kind: 'error' })
        return
      }
      const project =
        p.project?.id ?? scopeProjectId() ?? (app.lanes === 'project' && lane !== 'all' ? lane : null) ?? inboxId()
      if (!project) return
      const statusId = col.statusIds.get(project)
      if (!statusId) toast(`That project has no “${col.name}” column; added to its default column`)
      await ipc.createTask({
        title: p.parsed.title,
        project_id: project,
        status_id: statusId,
        priority: (app.lanes === 'priority' && lane !== 'all' ? (lane as Priority) : p.parsed.priority) ?? undefined,
        // Like Quick Add: no date in Today means today.
        due_date: p.parsed.due_date ?? (app.scope.kind === 'today' ? todayIso() : undefined),
        due_time: p.parsed.due_time ?? undefined,
        estimate_minutes: p.parsed.estimate_minutes ?? undefined,
        tags: p.parsed.tags,
        recurrence_rule: p.parsed.recurrence_rule ?? undefined,
      })
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  // ---------- column management (single project) ----------
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)
  let editing = $state<{ key: string; kind: 'rename' | 'wip' } | null>(null)
  let addingColumn = $state(false)

  const CATEGORIES: { value: Category; label: string }[] = [
    { value: 'todo', label: 'Not started' },
    { value: 'active', label: 'In progress' },
    { value: 'done', label: 'Done' },
    { value: 'cancelled', label: 'Cancelled' },
  ]

  const completedColumn = (col: Column) => col.category === 'done' || col.category === 'cancelled'

  function archiveItems(col: Column): MenuEntry[] {
    if (!completedColumn(col)) return []
    return [
      {
        label: 'Archive completed cards',
        disabled: !data.get(col.key)?.total,
        run: () => void archiveCompleted({ ...JSON.parse(baseKey), status_name: col.name }),
      },
    ]
  }

  function columnMenu(e: MouseEvent, col: Column) {
    const st = col.status
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const at = { x: r.left, y: r.bottom + 2 }
    if (!st) {
      const items = archiveItems(col)
      if (items.length) menu = { ...at, items }
      return
    }
    const archive = archiveItems(col)
    const index = allColumns.findIndex((c) => c.key === col.key)
    const others = allColumns.filter((c) => c.key !== col.key && c.status)
    menu = {
      ...at,
      items: [
        { label: 'Rename', run: () => (editing = { key: col.key, kind: 'rename' }) },
        {
          label: 'Color…',
          run: () =>
            (menu = {
              ...at,
              items: PALETTE.map((c) => ({
                label: c === 'gray' ? 'Gray' : `Hue ${c}`,
                color: c,
                checked: c === st.color,
                run: () => void act(() => ipc.updateStatus(st.id, { color: c })),
              })),
            }),
        },
        'separator',
        ...CATEGORIES.map((c) => ({
          label: c.label,
          checked: st.category === c.value,
          run: () => void act(() => ipc.updateStatus(st.id, { category: c.value })),
        })),
        'separator',
        { label: st.wip_limit ? `WIP limit: ${st.wip_limit}…` : 'Set WIP limit…', run: () => (editing = { key: col.key, kind: 'wip' }) },
        ...(st.wip_limit ? [{ label: 'Remove WIP limit', run: () => void act(() => ipc.updateStatus(st.id, { wip_limit: null })) }] : []),
        ...(archive.length ? ['separator' as const, ...archive] : []),
        'separator',
        {
          label: 'Move left',
          disabled: index === 0,
          run: () => void act(() => ipc.moveStatus(st.id, allColumns[index - 2]?.status?.id ?? null)),
        },
        {
          label: 'Move right',
          disabled: index === allColumns.length - 1,
          run: () => void act(() => ipc.moveStatus(st.id, allColumns[index + 1]?.status?.id ?? null)),
        },
        'separator',
        ...others.map((o) => ({
          label: `Delete, move tasks to “${o.name}”`,
          danger: true,
          run: () => {
            const target = o.status
            if (target) void act(() => ipc.deleteStatus(st.id, target.id), `Deleted column “${col.name}”`)
          },
        })),
      ],
    }
  }

  function cardMenu(e: MouseEvent, row: TaskRow) {
    if (kbd || drag) return
    e.preventDefault()
    e.stopPropagation()
    // From the keyboard there is no pointer: open under the card.
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const at = e.button === 2 ? { x: e.clientX, y: e.clientY } : { x: r.left + 16, y: r.bottom + 2 }
    menu = {
      ...at,
      items: [
        { label: 'Open', hint: 'Enter', run: () => (app.openTaskId = row.id) },
        { label: row.completed_at ? 'Mark not done' : 'Mark done', run: () => void act(() => ipc.setTaskDone(row.id, !row.completed_at)) },
        { label: 'Duplicate', run: () => void act(() => ipc.duplicateTask(row.id), 'Task duplicated') },
        'separator',
        {
          label: 'Archive',
          run: () => {
            if (app.openTaskId === row.id) app.openTaskId = null
            void act(() => ipc.setTaskArchived(row.id, true), 'Task archived')
          },
        },
        { label: 'Delete', danger: true, run: () => void act(() => ipc.deleteTask(row.id), 'Task deleted') },
      ],
    }
  }

  async function commitEdit(col: Column, value: string) {
    const e = editing
    editing = null
    const st = col.status
    if (!e || !st) return
    if (e.kind === 'rename') {
      const name = value.trim()
      if (name && name !== st.name) await act(() => ipc.updateStatus(st.id, { name }))
    } else {
      const n = Number(value.trim())
      if (!value.trim()) await act(() => ipc.updateStatus(st.id, { wip_limit: null }))
      else if (Number.isInteger(n) && n > 0) await act(() => ipc.updateStatus(st.id, { wip_limit: n }))
      else toast('WIP limit must be a whole number above 0', { kind: 'error' })
    }
  }

  async function addColumn(name: string) {
    // Escape (or Enter, then the blur that follows) already finished it.
    if (!addingColumn) return
    addingColumn = false
    const project = scopeProjectId()
    if (!name.trim() || !project) return
    const lastOpen = [...allColumns].reverse().find((c) => c.category === 'todo' || c.category === 'active')
    await act(() =>
      ipc.createStatus({ project_id: project, name: name.trim(), category: 'active', after: lastOpen?.status?.id }),
    )
  }

  function autofocus(node: HTMLInputElement) {
    node.focus()
    node.select()
  }

  const LANE_OPTIONS: { value: Lanes; label: string }[] = [
    { value: 'none', label: 'None' },
    { value: 'client', label: 'Group' },
    { value: 'project', label: 'Project' },
    { value: 'priority', label: 'Priority' },
  ]
  const priorityName: Record<Priority, string> = { urgent: 'Urgent', high: 'High', med: 'Medium', low: 'Low', none: '' }
</script>

<svelte:window onkeydown={onWindowKeydown} />

{#snippet card(row: TaskRow, colKey: string, lane: string)}
  <!-- A card is a focusable composite: it contains buttons (so it can't be a button)
       and must take keyboard focus for Space-to-move. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    data-card={row.id}
    class="card"
    class:ghosted={drag?.row.id === row.id}
    class:lifted={kbd?.row.id === row.id}
    class:done={!!row.completed_at}
    role="group"
    tabindex="0"
    aria-roledescription="Draggable task"
    aria-label={row.title}
    onpointerdown={(e) => onCardPointerDown(e, row)}
    onkeydown={(e) => onCardKeydown(e, row, colKey, lane)}
    oncontextmenu={(e) => cardMenu(e, row)}
  >
    <div class="flex items-start gap-2">
      <button
        class="-m-1 grid size-6 shrink-0 place-items-center rounded {row.completed_at ? 'text-success' : 'text-subtle hover:text-accent'}"
        aria-label={row.completed_at ? 'Mark not done' : 'Mark done'}
        onpointerdown={(e) => e.stopPropagation()}
        onclick={(e) => {
          e.stopPropagation()
          void act(() => ipc.setTaskDone(row.id, !row.completed_at))
        }}
      >
        {#if row.completed_at}<CheckCircle2 size={15} />{:else}<Circle size={15} />{/if}
      </button>
      <span class="title min-w-0 flex-1 leading-snug">{row.title}</span>
    </div>
    <div class="meta">
      {#if !singleProject}
        <span class="flex min-w-0 items-center gap-1 text-muted">
          <span class="size-1.5 shrink-0 rounded-full" style="background: {swatch(row.project_color)}"></span>
          <span class="truncate">{row.client_name && !(app.display.hide_repeated_context && app.scope.kind === 'client') ? `${row.client_name} / ` : ''}{row.project_name}</span>
        </span>
      {/if}
      {#if row.due_date}
        <span class="tabular {isOverdue(row.due_date, !!row.completed_at) ? 'font-medium text-danger' : 'text-muted'}">
          {dueLabel(row.due_date)}{row.due_time ? ` ${row.due_time}` : ''}
        </span>
      {/if}
      {#if row.priority !== 'none'}
        <span class="flex items-center gap-1 text-muted" title="{priorityName[row.priority]} priority">
          <span class="size-1.5 rounded-full" style="background: var(--priority-{row.priority})"></span>{priorityName[row.priority]}
        </span>
      {/if}
      {#if row.recurring}<Repeat size={11} class="text-subtle" aria-label="Repeats" />{/if}
      {#if row.subtasks_total}<span class="tabular text-subtle">{row.subtasks_done}/{row.subtasks_total}</span>{/if}
      {#if row.estimate_minutes}<span class="tabular text-subtle">{minutes(row.estimate_minutes)}</span>{/if}
      {#each row.tags.slice(0, 2) as tag (tag)}<span class="tag">#{tag}</span>{/each}
    </div>
    {#if !row.completed_at && (row.subtasks_total || row.progress > 0)}
      <div class="flex items-center gap-2" title={row.subtasks_total ? `From ${row.subtasks_done}/${row.subtasks_total} subtasks` : 'Progress'}>
        <span class="min-w-0 flex-1 overflow-hidden"><Progress value={row.progress} width={Math.max(60, colWidth - 64)} height={4} /></span>
        <span class="tabular text-[11px] text-subtle">{row.progress}%</span>
      </div>
    {/if}
  </div>
{/snippet}

{#snippet cell(col: Column, lane: string)}
  {@const rows = cellRows(col.key, lane)}
  <div class="cell" class:collapsed={collapsed[col.key]} class:drop={sorted && over?.col === col.key && over.lane === lane} data-cell="1" data-col={col.key} data-lane={lane}>
    {#if !collapsed[col.key]}
      {#each slots(rows) as { row, slot } (row.id)}
        {#if slot >= 0 && indicatorAt(col.key, lane, slot)}<div class="indicator"></div>{/if}
        {@render card(row, col.key, lane)}
      {/each}
      {#if indicatorAt(col.key, lane, slots(rows).filter((s) => s.slot >= 0).length)}<div class="indicator"></div>{/if}

      {#if adding && adding.col === col.key && adding.lane === lane}
        <input
          class="add-input"
          placeholder="Task name — Quick Add syntax works"
          use:autofocus
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              const v = e.currentTarget.value
              e.currentTarget.value = ''
              void addInColumn(col, lane, v)
            }
            if (e.key === 'Escape') {
              e.currentTarget.value = ''
              adding = null
            }
          }}
          onblur={(e) => {
            // Clicking away keeps what was typed.
            const v = e.currentTarget.value
            adding = null
            if (v.trim()) void addInColumn(col, lane, v)
          }}
        />
      {:else}
        <button class="add-btn" onclick={() => (adding = { col: col.key, lane })}><Plus size={14} /> Add task</button>
      {/if}
    {:else if lane === lanes[0]?.key}
      <button class="strip-label" title="Expand {col.name}" aria-label="Expand {col.name}" onclick={() => toggleColumn(col.key)}>
        <span class="vertical text-xs font-medium">{col.name}</span>
      </button>
    {/if}
  </div>
{/snippet}

<div class="flex min-h-0 flex-1 flex-col gap-2">
  <div class="flex h-8 shrink-0 items-center gap-3 text-xs text-muted">
    <span>Swimlanes</span>
    <div class="flex rounded-md border border-border bg-surface-2 p-0.5" role="radiogroup" aria-label="Swimlanes" use:segmented>
      {#each LANE_OPTIONS as opt (opt.value)}
        <button role="radio" aria-checked={app.lanes === opt.value} class="seg" onclick={() => (app.lanes = opt.value)}>{opt.label}</button>
      {/each}
    </div>
    <label class="flex items-center gap-1.5">
      <input type="checkbox" class="accent-[var(--accent)]" bind:checked={app.boardSubtasks} />
      Subtasks
    </label>
    {#if !singleProject && columns.length}
      <span class="min-w-0 truncate text-subtle" title="Columns combine same-named statuses across {scopeProjectIds().length} projects">Columns from {scopeProjectIds().length} projects</span>
    {/if}
    {#if sorted}
      <span class="ml-auto shrink-0 whitespace-nowrap text-subtle">Sorted · drag a card to another column to move it</span>
    {:else}
      <span class="ml-auto hidden shrink-0 whitespace-nowrap text-subtle 2xl:inline">Drag cards, or Space to pick up · arrows · Space to drop</span>
    {/if}
  </div>

  <div bind:this={board} class="board min-h-0 flex-1 overflow-auto rounded-[var(--radius)]" style="--col-w: {colWidth}px">
    <div class="inline-flex min-w-full flex-col">
      <!-- column headers (sticky) -->
      <div class="sticky top-0 z-10 flex gap-3 bg-surface-1 pb-2">
        {#each columns as col (col.key)}
          {@const d = data.get(col.key)}
          {@const wip = col.status?.wip_limit ?? null}
          {@const count = d?.total ?? 0}
          <div class="col-head" class:collapsed={collapsed[col.key]} title={collapsed[col.key] ? `${col.name} (${count})` : undefined}>
            {#if collapsed[col.key]}
              <button class="flex h-full w-full items-center justify-center gap-1" aria-label="Expand {col.name} ({count})" onclick={() => toggleColumn(col.key)}>
                <span class="size-2 shrink-0 rounded-full" style="background: {swatch(col.color, 0.62, 0.13)}"></span>
                <span class="tabular text-[11px] text-subtle">{count}</span>
              </button>
            {:else if editing?.key === col.key}
              <input
                class="h-7 w-full rounded border border-accent bg-surface-2 px-2 text-[13px] outline-none"
                value={editing.kind === 'rename' ? col.name : (wip ?? '')}
                placeholder={editing.kind === 'wip' ? 'Max cards, empty = none' : 'Column name'}
                use:autofocus
                onkeydown={(e) => {
                  if (e.key === 'Enter') void commitEdit(col, e.currentTarget.value)
                  if (e.key === 'Escape') editing = null
                }}
                onblur={(e) => void commitEdit(col, e.currentTarget.value)}
              />
            {:else}
              <span class="size-2 shrink-0 rounded-full" style="background: {swatch(col.color, 0.62, 0.13)}"></span>
              <span class="truncate font-medium" ondblclick={() => col.status && (editing = { key: col.key, kind: 'rename' })} role="heading" aria-level="2">{col.name}</span>
              <span class="tabular text-xs {wip && count > wip ? 'wip-over' : 'text-subtle'}" title={wip ? `WIP limit ${wip}` : undefined}>
                {count}{wip ? ` / ${wip}` : ''}
              </span>
              <span class="flex-1"></span>
              <button class="icon" aria-label="Collapse {col.name}" title="Collapse column" onclick={() => toggleColumn(col.key)}><ChevronsLeft size={13} /></button>
              {#if col.status || completedColumn(col)}
                <button class="icon" aria-label="{col.name} column actions" onclick={(e) => columnMenu(e, col)}><MoreHorizontal size={14} /></button>
              {/if}
            {/if}
          </div>
        {/each}
        {#if singleProject}
          <div class="w-[200px] shrink-0">
            {#if addingColumn}
              <input
                class="h-9 w-full rounded-md border border-accent bg-surface-2 px-2 outline-none"
                placeholder="Column name"
                use:autofocus
                onkeydown={(e) => {
                  if (e.key === 'Enter') void addColumn(e.currentTarget.value)
                  if (e.key === 'Escape') {
                    e.currentTarget.value = ''
                    addingColumn = false
                  }
                }}
                onblur={(e) => void addColumn(e.currentTarget.value)}
              />
            {:else}
              <button class="add-col" onclick={() => (addingColumn = true)}><Plus size={14} /> Add column</button>
            {/if}
          </div>
        {/if}
      </div>

      {#each lanes as lane (lane.key)}
        {#if app.lanes !== 'none'}
          {@const laneCount = columns.reduce((n, c) => n + cellRows(c.key, lane.key).length, 0)}
          <button class="lane-head" onclick={() => (laneCollapsed[lane.key] = !laneCollapsed[lane.key])} aria-expanded={!laneCollapsed[lane.key]}>
            {#if laneCollapsed[lane.key]}<ChevronRight size={14} />{:else}<ChevronDown size={14} />{/if}
            {#if lane.color}<span class="size-2 rounded-full" style="background: {lane.color.startsWith('var') ? lane.color : swatch(lane.color)}"></span>{/if}
            <span class="font-medium">{lane.label}</span>
            <span class="tabular text-xs text-subtle">{laneCount}</span>
          </button>
        {/if}
        {#if !laneCollapsed[lane.key]}
          <div class="flex gap-3 pb-3">
            {#each columns as col (col.key)}
              {@render cell(col, lane.key)}
            {/each}
            {#if singleProject}<div class="w-[200px] shrink-0"></div>{/if}
          </div>
        {/if}
      {/each}

      {#if app.lanes === 'none'}
        <div class="flex gap-3">
          {#each columns as col (col.key)}
            {@const d = data.get(col.key)}
            <div class="w-[var(--col-w)] shrink-0 {collapsed[col.key] ? '!w-11' : ''}">
              {#if d && d.total > d.rows.length && !collapsed[col.key]}
                <button class="add-btn" onclick={() => loadMore(col)}>Show {Math.min(CARD_LIMIT, d.total - d.rows.length)} more of {d.total - d.rows.length}</button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<!-- drag ghost -->
{#if drag}
  <div class="ghost-wrap" style="left: {drag.x - drag.offX}px; top: {drag.y - drag.offY}px; width: {drag.width}px">
    {@render card(drag.row, '', '')}
  </div>
{/if}

<div class="sr-only" aria-live="assertive">{announcement}</div>

{#if menu}
  <Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .board {
    scroll-padding: 8px;
  }
  .col-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: var(--col-w);
    flex-shrink: 0;
    height: 36px;
    padding-inline: 0.6rem 0.25rem;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .col-head.collapsed {
    width: 44px;
    padding: 0;
  }
  /* A collapsed column's name, down its strip (the head row stays one line). */
  .strip-label {
    display: flex;
    justify-content: center;
    width: 100%;
    padding: 10px 0;
    border-radius: 6px;
    color: var(--text-muted);
  }
  .strip-label:hover {
    background: var(--hover);
    color: var(--text);
  }
  .vertical {
    writing-mode: vertical-rl;
    white-space: nowrap;
  }
  .cell {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: var(--col-w);
    flex-shrink: 0;
    min-height: 64px;
    padding: 6px;
    border-radius: var(--radius);
    background: color-mix(in oklch, var(--surface-3) 55%, transparent);
  }
  /* Sorted columns: the whole column lights up as the drop target. */
  .cell.drop {
    box-shadow: inset 0 0 0 2px color-mix(in oklch, var(--accent) 60%, transparent);
  }
  .cell.collapsed {
    width: 44px;
    padding: 0;
  }
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    cursor: grab;
    touch-action: none;
    transition:
      box-shadow 120ms var(--ease-out),
      border-color 120ms var(--ease-out);
  }
  .card:hover {
    border-color: var(--border-strong);
    box-shadow: 0 1px 3px oklch(0 0 0 / 0.08);
  }
  .card.done .title {
    color: var(--text-subtle);
    text-decoration: line-through;
  }
  .card.ghosted {
    opacity: 0.35;
  }
  .card.lifted {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--focus-ring);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    font-size: 11.5px;
  }
  .meta:empty {
    display: none;
  }
  .tag {
    border-radius: 4px;
    padding: 0 5px;
    color: var(--text-muted);
    background: var(--surface-3);
  }
  .indicator {
    height: 3px;
    margin: -1px 2px;
    border-radius: 2px;
    background: var(--accent);
  }
  .ghost-wrap {
    position: fixed;
    z-index: 60;
    pointer-events: none;
    transform: rotate(2deg);
    filter: drop-shadow(0 8px 18px oklch(0 0 0 / 0.22));
  }
  .ghost-wrap :global(.card) {
    border-color: var(--accent);
  }
  .add-btn {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    height: 30px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    color: var(--text-subtle);
    font-size: 12.5px;
  }
  .add-btn:hover {
    background: var(--hover);
    color: var(--text);
  }
  .add-input {
    height: 34px;
    padding-inline: 0.6rem;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: var(--surface-2);
    outline: none;
  }
  .add-col {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    height: 36px;
    padding-inline: 0.75rem;
    border-radius: var(--radius);
    border: 1px dashed var(--border-strong);
    color: var(--text-muted);
  }
  .add-col:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .lane-head {
    position: sticky;
    left: 0;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    height: 32px;
    margin-bottom: 4px;
    padding-inline: 0.25rem;
    color: var(--text);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 5px;
    color: var(--text-subtle);
  }
  .icon:hover {
    background: var(--hover);
    color: var(--text);
  }
  .wip-over {
    font-weight: 600;
    color: var(--warn);
  }
  .seg {
    height: 22px;
    padding-inline: 0.55rem;
    border-radius: 4px;
  }
  .seg[aria-checked='true'] {
    background: var(--surface-3);
    color: var(--text);
  }
  :global(body.dragging),
  :global(body.dragging *) {
    cursor: grabbing !important;
    user-select: none;
  }
</style>
