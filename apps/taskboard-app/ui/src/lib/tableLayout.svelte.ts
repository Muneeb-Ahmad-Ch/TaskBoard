// Table columns: order, visibility and widths. Shared by the table and
// Settings → Layout; a per-device UI preference stored in localStorage.
import type { TaskSort } from './bindings/TaskSort'

export type ColId = 'title' | 'project' | 'status' | 'priority' | 'due' | 'progress' | 'estimate' | 'tags' | 'added'

export interface ColumnDef {
  id: ColId
  label: string
  /** Default width in px; the Task column (0) fills the remaining space. */
  width: number
  sort: TaskSort
  /** The first click sorts newest / highest first. */
  firstDescending?: boolean
}

export const COLUMN_DEFS: ColumnDef[] = [
  { id: 'title', label: 'Task', width: 0, sort: 'title' },
  { id: 'project', label: 'Group / Project', width: 230, sort: 'project' },
  { id: 'status', label: 'Status', width: 130, sort: 'status' },
  { id: 'priority', label: 'Priority', width: 104, sort: 'priority' },
  { id: 'due', label: 'Due', width: 136, sort: 'due' },
  { id: 'progress', label: 'Progress', width: 118, sort: 'progress', firstDescending: true },
  { id: 'estimate', label: 'Estimate', width: 96, sort: 'estimate' },
  { id: 'tags', label: 'Tags', width: 150, sort: 'tags' },
  { id: 'added', label: 'Added', width: 112, sort: 'created', firstDescending: true },
]

/** Columns that can move; Task always stays first (it stretches to fill). */
const MOVABLE = COLUMN_DEFS.filter((c) => c.id !== 'title').map((c) => c.id)
const KEY = 'taskboard.table.layout'

type Stored = { widths: Partial<Record<ColId, number>>; hidden: ColId[]; order: ColId[] }

function load(): Stored {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Partial<Stored> | null
    const known = (saved?.order ?? []).filter((id) => MOVABLE.includes(id))
    const order = [...known, ...MOVABLE.filter((id) => !known.includes(id))]
    return { widths: saved?.widths ?? {}, hidden: (saved?.hidden ?? []).filter((id) => MOVABLE.includes(id)), order }
  } catch {
    return { widths: {}, hidden: [], order: [...MOVABLE] }
  }
}

export const tableLayout = $state<Stored>(load())

export function saveTableLayout() {
  try {
    localStorage.setItem(KEY, JSON.stringify(tableLayout))
  } catch {
    // per-device convenience only
  }
}

/** Task first, then the others in the user's order (hidden ones included). */
export function orderedColumns(): ColumnDef[] {
  const rest = tableLayout.order.flatMap((id) => COLUMN_DEFS.filter((c) => c.id === id))
  return [COLUMN_DEFS[0], ...rest]
}

export function moveColumn(id: ColId, dir: -1 | 1) {
  const i = tableLayout.order.indexOf(id)
  const j = i + dir
  if (i < 0 || j < 0 || j >= tableLayout.order.length) return
  const [item] = tableLayout.order.splice(i, 1)
  tableLayout.order.splice(j, 0, item)
  saveTableLayout()
}

export function setColumnVisible(id: ColId, visible: boolean) {
  if (id === 'title') return
  tableLayout.hidden = visible ? tableLayout.hidden.filter((h) => h !== id) : [...new Set([...tableLayout.hidden, id])]
  saveTableLayout()
}

export function resetTableLayout() {
  tableLayout.widths = {}
  tableLayout.hidden = []
  tableLayout.order = [...MOVABLE]
  saveTableLayout()
}
