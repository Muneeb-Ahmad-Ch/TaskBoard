<script lang="ts">
  // Settings → Sections: the sidebar's sections (e.g. Clients, Personal):
  // rename, recolor, reorder, hide, add and delete.
  import { ArrowDown, ArrowUp, Pencil, Plus, Trash2 } from '@lucide/svelte'
  import { plural, swatch } from '../lib/format'
  import { ipc } from '../lib/ipc'
  import { layout, setSectionHidden } from '../lib/layout.svelte'
  import { act, app, groupsIn, itemName, looseProjectsIn } from '../lib/store.svelte'
  import Switch from './Switch.svelte'

  let { onchange = () => {} }: { onchange?: () => void } = $props()
</script>

<p class="mb-3 text-xs text-muted">
  Sections organize the sidebar. Each holds groups (clients, teams, areas…) and projects. Edit a section to rename it, recolor it, or change what its groups are called.
</p>

<section class="card" data-setting="sections">
  <h4 class="card-title">Sections <span class="font-normal tracking-normal normal-case">· point at a row to reorder it</span></h4>
  <ul aria-label="Sidebar sections">
    {#each app.sections as sec, i (sec.id)}
      {@const shown = !layout.hiddenSections.includes(sec.id)}
      <li class="item">
        <span class="size-2 shrink-0 rounded-full" style="background: {swatch(sec.color)}"></span>
        <span class="flex min-w-0 flex-1 flex-col">
          <span class="truncate text-[13px] {shown ? '' : 'text-subtle'}">{sec.name}</span>
          <span class="truncate text-xs text-muted">{plural(groupsIn(sec.id).length, itemName(sec))} · {plural(looseProjectsIn(sec.id).length, 'project')} outside them</span>
        </span>
        <button class="order" aria-label="Move {sec.name} up" title="Move up" disabled={i === 0} onclick={() => void act(() => ipc.moveSection(sec.id, 'up')).then(onchange)}><ArrowUp size={14} /></button>
        <button class="order" aria-label="Move {sec.name} down" title="Move down" disabled={i === app.sections.length - 1} onclick={() => void act(() => ipc.moveSection(sec.id, 'down')).then(onchange)}><ArrowDown size={14} /></button>
        <button class="tool" aria-label="Edit {sec.name}" title="Rename, recolor, or change what its groups are called" onclick={() => (app.sectionDialog = { id: sec.id })}><Pencil size={13} /></button>
        <button class="tool" aria-label="Delete {sec.name}" title={app.sections.length > 1 ? 'Delete section…' : 'Keep at least one section'} disabled={app.sections.length <= 1} onclick={() => (app.deleteSection = sec.id)}><Trash2 size={13} /></button>
        <Switch label="Show {sec.name}" checked={shown} onchange={(v) => { setSectionHidden(sec.id, !v); onchange() }} />
      </li>
    {/each}
  </ul>
  <div class="px-4 pt-1 pb-3">
    <button class="btn !h-7 text-xs" onclick={() => (app.sectionDialog = { id: null })}><Plus size={13} /> New section</button>
  </div>
</section>

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
    min-height: 48px;
    padding: 4px 16px;
  }
  .item + .item {
    border-top: 1px solid var(--border);
  }
  .order,
  .tool {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--text-muted);
  }
  .order {
    opacity: 0;
    transition: opacity 120ms;
  }
  .item:hover .order,
  .item:focus-within .order {
    opacity: 1;
  }
  .item:hover .order:disabled,
  .item:focus-within .order:disabled,
  .tool:disabled {
    opacity: 0.3;
  }
  .order:hover:not(:disabled),
  .tool:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
</style>
