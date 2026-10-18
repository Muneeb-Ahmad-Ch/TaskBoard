<script lang="ts">
  // A draggable edge that resizes a side panel (ARIA window splitter):
  // drag it, or focus it and use ← / →; Home or a double-click resets.
  let {
    value,
    min,
    max,
    initial,
    edge,
    label,
    onchange,
  }: {
    value: number
    min: number
    max: number
    /** The default width, for Home / double-click. */
    initial: number
    /** Which edge of its panel it sits on: right of the sidebar, left of the task panel. */
    edge: 'right' | 'left'
    label: string
    onchange: (px: number) => void
  } = $props()

  /** Dragging right grows a left-side panel and shrinks a right-side one. */
  const sign = $derived(edge === 'right' ? 1 : -1)
  const clamp = (v: number) => Math.round(Math.max(min, Math.min(max, v)))
  let dragging = $state(false)

  function down(e: PointerEvent) {
    if (e.button !== 0) return
    e.preventDefault()
    const el = e.currentTarget as HTMLElement
    el.setPointerCapture(e.pointerId)
    const startX = e.clientX
    const start = value
    dragging = true
    document.body.classList.add('col-resizing')
    const move = (ev: PointerEvent) => onchange(clamp(start + (ev.clientX - startX) * sign))
    const up = () => {
      dragging = false
      document.body.classList.remove('col-resizing')
      el.removeEventListener('pointermove', move)
      el.removeEventListener('pointerup', up)
      el.removeEventListener('pointercancel', up)
    }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', up)
    el.addEventListener('pointercancel', up)
  }

  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 48 : 16
    if (e.key === 'ArrowRight') onchange(clamp(value + step * sign))
    else if (e.key === 'ArrowLeft') onchange(clamp(value - step * sign))
    else if (e.key === 'Home') onchange(initial)
    else return
    e.preventDefault()
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="splitter {edge}"
  class:dragging
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={value}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  title="Drag to resize · double-click to reset"
  onpointerdown={down}
  onkeydown={key}
  ondblclick={() => onchange(initial)}
></div>

<style>
  .splitter {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 35;
    width: 9px;
    cursor: col-resize;
    touch-action: none;
    outline: none;
  }
  .splitter.right {
    right: -5px;
  }
  .splitter.left {
    left: -5px;
  }
  /* A thin line appears on hover / focus / drag, on the panel's edge. */
  .splitter::after {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 4px;
    width: 1px;
    background: transparent;
    transition: background-color 120ms;
  }
  .splitter:hover::after,
  .splitter:focus-visible::after,
  .splitter.dragging::after {
    left: 3px;
    width: 3px;
    background: var(--accent);
  }
</style>
