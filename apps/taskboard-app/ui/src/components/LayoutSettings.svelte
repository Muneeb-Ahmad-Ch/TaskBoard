<script lang="ts">
  // Settings → Sidebar & views: show, hide and order sidebar items; show or
  // hide sections and view tabs. Hidden items stay reachable from Ctrl+K.
  import { ArrowDown, ArrowUp } from '@lucide/svelte'
  import {
    layout,
    moveNav,
    NAV_LABELS,
    resetLayout,
    SECTION_LABELS,
    setNavVisible,
    setSection,
    setTab,
    TAB_LABELS,
    type SectionId,
    type TabId,
  } from '../lib/layout.svelte'
  import { toast } from '../lib/toast.svelte'

  const SECTIONS = Object.keys(SECTION_LABELS) as SectionId[]
  const TABS = Object.keys(TAB_LABELS) as TabId[]

  function toggleTab(id: TabId, visible: boolean, input: HTMLInputElement) {
    if (!setTab(id, visible)) {
      input.checked = true
      toast('Keep at least one view')
    }
  }
</script>

<div class="flex flex-col gap-4 text-[13px]">
  <div>
    <p class="mb-1.5 text-xs text-muted">Sidebar items — hidden items are still in Ctrl K</p>
    <ul class="flex flex-col gap-0.5" aria-label="Sidebar items">
      {#each layout.nav as item, i (item.id)}
        <li class="flex h-8 items-center gap-2 rounded-md px-1.5 hover:bg-hover">
          <label class="flex flex-1 items-center gap-2">
            <input type="checkbox" checked={item.visible} onchange={(e) => setNavVisible(item.id, e.currentTarget.checked)} />
            <span class={item.visible ? '' : 'text-subtle'}>{NAV_LABELS[item.id]}</span>
          </label>
          <button class="order" aria-label="Move {NAV_LABELS[item.id]} up" title="Move up" disabled={i === 0} onclick={() => moveNav(item.id, -1)}><ArrowUp size={14} /></button>
          <button class="order" aria-label="Move {NAV_LABELS[item.id]} down" title="Move down" disabled={i === layout.nav.length - 1} onclick={() => moveNav(item.id, 1)}><ArrowDown size={14} /></button>
        </li>
      {/each}
    </ul>
  </div>

  <div class="flex flex-wrap gap-x-8 gap-y-3">
    <fieldset>
      <legend class="mb-1.5 text-xs text-muted">Sidebar sections</legend>
      {#each SECTIONS as id (id)}
        <label class="flex h-7 items-center gap-2">
          <input type="checkbox" checked={layout.sections[id]} onchange={(e) => setSection(id, e.currentTarget.checked)} />
          {SECTION_LABELS[id]}
        </label>
      {/each}
    </fieldset>
    <fieldset>
      <legend class="mb-1.5 text-xs text-muted">View tabs</legend>
      {#each TABS as id (id)}
        <label class="flex h-7 items-center gap-2">
          <input type="checkbox" checked={layout.tabs[id]} onchange={(e) => toggleTab(id, e.currentTarget.checked, e.currentTarget)} />
          {TAB_LABELS[id]}
        </label>
      {/each}
    </fieldset>
  </div>

  <div>
    <button class="text-xs text-muted underline-offset-2 hover:text-text hover:underline" onclick={resetLayout}>Restore default layout</button>
  </div>
</div>

<style>
  .order {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 5px;
    color: var(--text-muted);
  }
  .order:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text);
  }
  .order:disabled {
    opacity: 0.3;
  }
  input[type='checkbox'] {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
  }
</style>
