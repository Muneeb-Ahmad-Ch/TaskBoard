// Pointer-based drag and drop into the sidebar: sections, groups and projects
// are dragged within it, and tasks are dragged onto it from the table or the
// board. Pointer events rather than HTML5 drag and drop, like the board, so
// the WebView's file-drop handling never gets in the way.
//
// The sidebar registers a resolver that turns a pointer position into a drop
// target; whoever starts a drag only says what is being dragged.
import type { Id } from './bindings/Id'

export type DragItem =
  | { kind: 'section'; id: Id; label: string }
  | { kind: 'group'; id: Id; label: string }
  | { kind: 'project'; id: Id; label: string; pinned?: boolean }
  | { kind: 'tasks'; ids: Id[]; label: string }

/** Where the item would land; `key` matches a row's `data-drop` attribute. */
export interface DropTarget {
  key: string
  position: 'before' | 'after' | 'inside'
  run: () => void
}

type Resolver = (item: DragItem, x: number, y: number) => DropTarget | null

export const dnd = $state({
  item: null as DragItem | null,
  x: 0,
  y: 0,
  target: null as DropTarget | null,
  /** Set when another component draws its own drag preview (board cards). */
  external: false,
})

let resolver: Resolver | null = null
let pending: { item: DragItem; x: number; y: number } | null = null
let swallowClick = false

/** The sidebar tells us how to find drop targets (null when it unmounts). */
export function setDropResolver(r: Resolver | null) {
  resolver = r
}

/** Starts watching a press; it becomes a drag once the pointer moves 5px. */
export function beginDrag(e: PointerEvent, item: DragItem) {
  if (e.button !== 0) return
  pending = { item, x: e.clientX, y: e.clientY }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('keydown', onKey, true)
}

function onMove(e: PointerEvent) {
  if (pending && !dnd.item) {
    if (Math.hypot(e.clientX - pending.x, e.clientY - pending.y) < 5) return
    dnd.item = pending.item
    dnd.external = false
    document.body.classList.add('dragging')
  }
  if (!dnd.item) return
  dnd.x = e.clientX
  dnd.y = e.clientY
  dnd.target = resolver?.(dnd.item, dnd.x, dnd.y) ?? null
}

function onUp() {
  const target = dnd.target
  const dragged = !!dnd.item
  stop()
  if (!dragged) return
  // The click that follows the release lands on the dragged row; ignore it.
  swallowClick = true
  setTimeout(() => (swallowClick = false), 300)
  target?.run()
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' && dnd.item) {
    e.preventDefault()
    e.stopPropagation()
    stop()
  }
}

function stop() {
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
  window.removeEventListener('keydown', onKey, true)
  pending = null
  dnd.item = null
  dnd.target = null
  document.body.classList.remove('dragging')
}

if (typeof window !== 'undefined') {
  window.addEventListener(
    'click',
    (e) => {
      if (!swallowClick) return
      swallowClick = false
      e.preventDefault()
      e.stopPropagation()
    },
    true,
  )
}

/** For a drag another component runs itself (board cards): track the pointer. */
export function hoverExternal(item: DragItem, x: number, y: number): boolean {
  dnd.item = item
  dnd.external = true
  dnd.x = x
  dnd.y = y
  dnd.target = resolver?.(item, x, y) ?? null
  return !!dnd.target
}

/** Ends an external drag; drops it on the sidebar if it is over a target. */
export function endExternal(drop: boolean): boolean {
  const target = drop ? dnd.target : null
  dnd.item = null
  dnd.target = null
  dnd.external = false
  target?.run()
  return !!target
}

/** Class for a row whose `data-drop` key is `key`, showing where a drop lands. */
export function dropClass(key: string): string {
  const t = dnd.target
  return t && t.key === key ? `drop-${t.position}` : ''
}
