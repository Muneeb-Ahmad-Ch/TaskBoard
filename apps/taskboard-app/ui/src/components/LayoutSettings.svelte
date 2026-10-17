<script lang="ts">
  // Settings → Layout: sidebar items, sections, view tabs and table columns —
  // show, hide and order. Hidden items stay reachable from Ctrl+K.
  import { ArrowDown, ArrowUp } from '@lucide/svelte'
  import {
    layout,
    moveNav,
    NAV_LABELS,
    resetLayout,
    SECTION_LABELS,
    setNavVisible,
    setSection,
    setSectionHidden,
    setTab,
    TAB_LABELS,
    type SectionId,
    type TabId,
  } from '../lib/layout.svelte'
  import { toast } from '../lib/toast.svelte'
  import { moveColumn, orderedColumns, resetTableLayout, setColumnVisible, tableLayout } from '../lib/tableLayout.svelte'
  import Switch from './Switch.svelte'
  import { swatch } from '../lib/format'
  import { ipc } from '../lib/ipc'
  import { act, app } from '../lib/store.svelte'

  let { onchange = () => {} }: { onchange?: () => void } = $props()

  const SECTIONS = Object.keys(SECTION_LABELS) as SectionId[]
  const TABS = Object.keys(TAB_LABELS) as TabId[]

  function toggleTab(id: TabId, visible: boolean) {
    if (!setTab(id, visible)) toast('Keep at least one view')
    else onchange()
  }
</script>

<p class="mb-3 text-xs text-muted">
  Choose what the sidebar, views and table show, and in which order. Hidden sidebar items stay available from search (Ctrl+K).
</p>

<section class="card">
  <h4 class="card-title">Sidebar items</h4>
  <ul aria-label="Sidebar items">
    {#each layout.nav as item, i (item.id)}
      <li class="item">
        <span class="flex-1 text-[13px] {item.visible ? '' : 'text-subtle'}">{NAV_LABELS[item.id]}</span>
        <button class="order" aria-label="Move {NAV_LABELS[item.id]} up" title="Move up" disabled={i === 0} onclick={() => { moveNav(item.id, -1); onchange() }}><ArrowUp size={14} /></button>
        <button class="order" aria-label="Move {NAV_LABELS[item.id]} down" title="Move down" disabled={i === layout.nav.length - 1} onclick={() => { moveNav(item.id, 1); onchange() }}><ArrowDown size={14} /></button>
        <Switch label="Show {NAV_LABELS[item.id]}" checked={item.visible} onchange={(v) => { setNavVisible(item.id, v); onchange() }} />
      </li>
    {/each}
  </ul>
</section>

<section class="card">
  <h4 class="card-title">Sidebar sections</h4>
  <ul aria-label="Sidebar sections">
    {#each app.sections as sec, i (sec.id)}
      {@const shown = !layout.hiddenSections.includes(sec.id)}
      <li class="item">
        <span class="size-2 shrink-0 rounded-full" style="background: {swatch(sec.color)}"></span>
        <span class="flex-1 text-[13px] {shown ? '' : 'text-subtle'}">{sec.name}</span>
        <button class="order" aria-label="Move {sec.name} up" title="Move up" disabled={i === 0} onclick={() => void act(() => ipc.moveSection(sec.id, 'up')).then(onchange)}><ArrowUp size={14} /></button>
        <button class="order" aria-label="Move {sec.name} down" title="Move down" disabled={i === app.sections.length - 1} onclick={() => void act(() => ipc.moveSection(sec.id, 'down')).then(onchange)}><ArrowDown size={14} /></button>
        <Switch label="Show {sec.name}" checked={shown} onchange={(v) => { setSectionHidden(sec.id, !v); onchange() }} />
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
  <h4 class="card-title">View tabs</h4>
  <ul aria-label="View tabs">
    {#each TABS as id (id)}
      <li class="item">
        <span class="flex-1 text-[13px] {layout.tabs[id] ? '' : 'text-subtle'}">{TAB_LABELS[id]}</span>
        <Switch label="Show {TAB_LABELS[id]} tab" checked={layout.tabs[id]} onchange={(v) => toggleTab(id, v)} />
      </li>
    {/each}
  </ul>
</section>

<section class="card">
  <h4 class="card-title">Table columns</h4>
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
  <p class="px-4 pb-3 text-xs text-muted">Task always comes first. On small windows some columns hide automatically to fit.</p>
</section>

<button class="btn !h-7 text-xs" onclick={() => { resetLayout(); resetTableLayout(); onchange() }}>Restore default layout</button>

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
    min-height: 44px;
    padding: 6px 16px;
  }
  .item + .item {
    border-top: 1px solid var(--border);
  }
  .order {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--text-muted);
  }
  .order:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .order:disabled {
    opacity: 0.3;
  }
</style>
