<script lang="ts">
  // Settings → Views & table: view tabs, where the add-a-task box sits, the
  // table's columns (show, hide, order) and the task panel's width.
  import { ArrowDown, ArrowUp } from '@lucide/svelte'
  import { layout, PANEL, resetViewsLayout, setPanelWidth, setQuickAddPlacement, setTab, TAB_LABELS, type TabId } from '../lib/layout.svelte'
  import { moveColumn, orderedColumns, resetTableLayout, setColumnVisible, tableLayout } from '../lib/tableLayout.svelte'
  import { toast } from '../lib/toast.svelte'
  import Switch from './Switch.svelte'

  let { onchange = () => {} }: { onchange?: () => void } = $props()

  const TABS = Object.keys(TAB_LABELS) as TabId[]

  /** A refused change (the last view) puts the box back. */
  function toggleTab(id: TabId, box: HTMLInputElement) {
    if (setTab(id, box.checked)) onchange()
    else {
      box.checked = layout.tabs[id]
      toast('Keep at least one view')
    }
  }
</script>

<section class="card">
  <h4 class="card-title">Views</h4>
  <div class="item">
    <span class="flex min-w-0 flex-1 flex-col">
      <span class="text-[13px]">View tabs</span>
      <span class="text-xs text-muted">Shown in the header; keep at least one</span>
    </span>
    <div class="flex flex-wrap justify-end gap-x-4 gap-y-1" role="group" aria-label="View tabs">
      {#each TABS as id (id)}
        <label class="flex items-center gap-1.5 text-[13px]">
          <input type="checkbox" class="accent-[var(--accent)]" checked={layout.tabs[id]} onchange={(e) => toggleTab(id, e.currentTarget)} />
          {TAB_LABELS[id]}
        </label>
      {/each}
    </div>
  </div>
  <div class="item">
    <span class="flex min-w-0 flex-1 flex-col">
      <span class="text-[13px]">Add-a-task box</span>
      <span class="text-xs text-muted">In the Table view; press N to jump to it from anywhere</span>
    </span>
    <div class="seg-group" role="radiogroup" aria-label="Add-a-task box position">
      {#each [['top', 'Top'], ['bottom', 'Bottom']] as [value, label] (value)}
        <button role="radio" aria-checked={layout.quickAdd === value} class="seg" onclick={() => { setQuickAddPlacement(value as 'top' | 'bottom'); onchange() }}>{label}</button>
      {/each}
    </div>
  </div>
</section>

<section class="card">
  <h4 class="card-title">Table columns <span class="font-normal tracking-normal normal-case">· point at a row to reorder it</span></h4>
  <ul aria-label="Table columns">
    {#each orderedColumns().filter((c) => c.id !== 'title') as col, i (col.id)}
      {@const shown = !tableLayout.hidden.includes(col.id)}
      <li class="item">
        <span class="flex-1 text-[13px] {shown ? '' : 'text-subtle'}">{col.label}</span>
        <button class="order" aria-label="Move {col.label} column left" title="Move left" disabled={i === 0} onclick={() => { moveColumn(col.id, -1); onchange() }}><ArrowUp size={14} /></button>
        <button class="order" aria-label="Move {col.label} column right" title="Move right" disabled={i === tableLayout.order.length - 1} onclick={() => { moveColumn(col.id, 1); onchange() }}><ArrowDown size={14} /></button>
        <Switch label="Show {col.label} column" checked={shown} onchange={(v) => { setColumnVisible(col.id, v); onchange() }} />
      </li>
    {/each}
  </ul>
  <p class="px-4 pb-3 text-xs text-muted">Task always comes first. In small windows some columns hide to fit. Drag a column's edge in the table to resize it.</p>
</section>

<section class="card">
  <h4 class="card-title">Task panel</h4>
  <div class="item">
    <input
      type="range"
      class="min-w-0 flex-1 accent-[var(--accent)]"
      min={PANEL.min}
      max={PANEL.max}
      step="8"
      value={layout.panelWidth}
      aria-label="Task panel width"
      oninput={(e) => setPanelWidth(Number(e.currentTarget.value))}
      onchange={onchange}
    />
    <span class="tabular w-14 shrink-0 text-right text-xs text-muted">{layout.panelWidth} px</span>
    <button class="btn !h-7 text-xs" disabled={layout.panelWidth === PANEL.default} onclick={() => { setPanelWidth(PANEL.default); onchange() }}>Reset</button>
  </div>
  <p class="px-4 pb-3 text-xs text-muted">Width of the task details panel. Or drag its left edge; double-click the edge to reset.</p>
</section>

<button class="btn !h-7 text-xs" onclick={() => { resetViewsLayout(); resetTableLayout(); onchange() }}>Restore defaults for views and table</button>

<style>
  .card {
    margin-bottom: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-1);
  }
  .card-title {
    padding: 12px 16px 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-subtle);
  }
  .item {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    min-height: 40px;
    padding: 6px 16px;
  }
  .item + .item {
    border-top: 1px solid var(--border);
  }
  ul .item {
    flex-wrap: nowrap;
    gap: 6px;
    padding-block: 4px;
  }
  .seg-group {
    display: flex;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--surface-2);
  }
  .seg {
    height: 24px;
    padding-inline: 10px;
    border-radius: 5px;
    font-size: 12px;
    color: var(--text-muted);
  }
  .seg[aria-checked='true'] {
    background: var(--accent-soft);
    color: var(--accent-soft-text);
    font-weight: 500;
  }
  .order {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--text-muted);
    opacity: 0;
    transition: opacity 120ms;
  }
  .item:hover .order,
  .item:focus-within .order {
    opacity: 1;
  }
  .item:hover .order:disabled,
  .item:focus-within .order:disabled {
    opacity: 0.3;
  }
  .order:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
</style>
