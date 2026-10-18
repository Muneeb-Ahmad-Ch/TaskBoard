<script lang="ts">
  // Settings → Sidebar: which items show and in which order, the small
  // details (counts, color dots) and the width. Hidden items stay in Ctrl+K.
  import { ArrowDown, ArrowUp } from '@lucide/svelte'
  import {
    COUNT_LABELS,
    layout,
    moveNav,
    NAV_LABELS,
    resetSidebarLayout,
    SECTION_LABELS,
    setCount,
    setHeaderDot,
    setNavVisible,
    setSection,
    setSectionDots,
    setSidebarWidth,
    SIDEBAR,
    type CountId,
    type SectionId,
  } from '../lib/layout.svelte'
  import Switch from './Switch.svelte'

  let { onchange = () => {} }: { onchange?: () => void } = $props()

  const SECTIONS = Object.keys(SECTION_LABELS) as SectionId[]
  const COUNTS = Object.keys(COUNT_LABELS) as CountId[]

  /** The checkbox grid: dots first, then the counts. */
  const details = $derived([
    { id: 'sectionDots', label: 'Section color dots', hint: 'The colored dot before each section name', on: layout.sectionDots, set: setSectionDots },
    { id: 'headerDot', label: 'Header color dot', hint: 'The colored dot before "Section › Group" in the page header', on: layout.headerDot, set: setHeaderDot },
    ...COUNTS.map((id) => ({ id, label: COUNT_LABELS[id].name, hint: COUNT_LABELS[id].hint, on: layout.counts[id], set: (v: boolean) => setCount(id, v) })),
  ])
</script>

<p class="mb-3 text-xs text-muted">What the sidebar shows, and in which order. Hidden items stay available from search (Ctrl+K).</p>

<section class="card">
  <h4 class="card-title">Items <span class="font-normal tracking-normal normal-case">· point at a row to reorder it</span></h4>
  <ul aria-label="Sidebar items">
    {#each layout.nav as item, i (item.id)}
      <li class="item">
        <span class="flex-1 text-[13px] {item.visible ? '' : 'text-subtle'}">{NAV_LABELS[item.id]}</span>
        <button class="order" aria-label="Move {NAV_LABELS[item.id]} up" title="Move up" disabled={i === 0} onclick={() => { moveNav(item.id, -1); onchange() }}><ArrowUp size={14} /></button>
        <button class="order" aria-label="Move {NAV_LABELS[item.id]} down" title="Move down" disabled={i === layout.nav.length - 1} onclick={() => { moveNav(item.id, 1); onchange() }}><ArrowDown size={14} /></button>
        <Switch label="Show {NAV_LABELS[item.id]}" checked={item.visible} onchange={(v) => { setNavVisible(item.id, v); onchange() }} />
      </li>
    {/each}
    {#each SECTIONS as id (id)}
      <li class="item">
        <span class="flex-1 text-[13px] {layout.sections[id] ? '' : 'text-subtle'}">{SECTION_LABELS[id]}</span>
        <Switch label="Show {SECTION_LABELS[id]}" checked={layout.sections[id]} onchange={(v) => { setSection(id, v); onchange() }} />
      </li>
    {/each}
  </ul>
</section>

<section class="card">
  <h4 class="card-title">Details</h4>
  <div class="grid grid-cols-2 gap-x-4 gap-y-1 px-4 pt-1 pb-3 max-[700px]:grid-cols-1" role="group" aria-label="Sidebar details">
    {#each details as d (d.id)}
      <label class="check" title={d.hint}>
        <input type="checkbox" class="accent-[var(--accent)]" aria-label="Show {d.label.toLowerCase()}" aria-describedby="detail-{d.id}" checked={d.on} onchange={(e) => { d.set(e.currentTarget.checked); onchange() }} />
        <span class="min-w-0">
          <span class="block text-[13px]">{d.label}</span>
          <span id="detail-{d.id}" class="block truncate text-xs text-muted">{d.hint}</span>
        </span>
      </label>
    {/each}
  </div>
</section>

<section class="card">
  <h4 class="card-title">Width</h4>
  <div class="item">
    <input
      type="range"
      class="min-w-0 flex-1 accent-[var(--accent)]"
      min={SIDEBAR.min}
      max={SIDEBAR.max}
      step="8"
      value={layout.sidebarWidth}
      aria-label="Sidebar width"
      oninput={(e) => setSidebarWidth(Number(e.currentTarget.value))}
      onchange={onchange}
    />
    <span class="tabular w-14 shrink-0 text-right text-xs text-muted">{layout.sidebarWidth} px</span>
    <button class="btn !h-7 text-xs" disabled={layout.sidebarWidth === SIDEBAR.default} onclick={() => { setSidebarWidth(SIDEBAR.default); onchange() }}>Reset</button>
  </div>
  <p class="px-4 pb-3 text-xs text-muted">Or drag the sidebar's right edge. Double-click the edge to reset.</p>
</section>

<button class="btn !h-7 text-xs" onclick={() => { resetSidebarLayout(); onchange() }}>Restore sidebar defaults</button>

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
    align-items: center;
    gap: 6px;
    min-height: 40px;
    padding: 4px 16px;
  }
  .item + .item {
    border-top: 1px solid var(--border);
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    min-width: 0;
    padding: 6px 0;
    cursor: pointer;
  }
  .check input {
    margin-top: 3px;
    flex-shrink: 0;
  }
  .order {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--text-muted);
    /* Shown on the row being pointed at or focused, so the list reads cleanly. */
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
