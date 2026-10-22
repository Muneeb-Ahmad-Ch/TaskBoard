<script lang="ts">
  // What the Filter menu is hiding, above the table / board: one chip per kind
  // of filter (× removes it), and Clear all.
  import { X } from '@lucide/svelte'
  import type { Priority } from '../lib/bindings/Priority'
  import { ADDED_LABEL, DUE_LABEL, filterCount } from '../lib/filters'
  import { app, clearFilter } from '../lib/store.svelte'

  const PRIORITY: Record<Priority, string> = { urgent: 'Urgent', high: 'High', med: 'Medium', low: 'Low', none: 'No priority' }
  const f = $derived(app.filter)

  const chips = $derived.by(() => {
    const out: { key: string; label: string; clear: () => void }[] = []
    if (f.priorities.length) out.push({ key: 'priority', label: `Priority: ${f.priorities.map((p) => PRIORITY[p]).join(', ')}`, clear: () => (app.filter.priorities = []) })
    if (f.statuses.length) out.push({ key: 'status', label: `Status: ${f.statuses.join(', ')}`, clear: () => (app.filter.statuses = []) })
    if (f.due) out.push({ key: 'due', label: DUE_LABEL[f.due], clear: () => (app.filter.due = null) })
    if (f.added) out.push({ key: 'added', label: `Added: ${ADDED_LABEL[f.added].replace('Added ', '')}`, clear: () => (app.filter.added = null) })
    if (f.tags.length) out.push({ key: 'tags', label: f.tags.map((t) => `#${t}`).join(' or '), clear: () => (app.filter.tags = []) })
    if (f.projects.length) {
      const names = f.projects.map((id) => app.projects.find((p) => p.id === id)?.name ?? 'Project')
      out.push({ key: 'projects', label: `Project: ${names.join(', ')}`, clear: () => (app.filter.projects = []) })
    }
    return out
  })
</script>

{#if filterCount(f)}
  <div class="flex shrink-0 flex-wrap items-center gap-1.5 text-xs" role="group" aria-label="Filters on">
    {#each chips as c (c.key)}
      <span class="chip">
        <span class="truncate">{c.label}</span>
        <button class="grid size-4 place-items-center rounded-full text-muted hover:bg-surface-3 hover:text-text" aria-label="Remove filter {c.label}" onclick={c.clear}><X size={11} /></button>
      </span>
    {/each}
    <button class="px-1.5 text-muted underline-offset-2 hover:text-text hover:underline" onclick={clearFilter}>Clear all</button>
  </div>
{/if}

<style>
  .chip {
    display: inline-flex;
    max-width: 320px;
    align-items: center;
    gap: 0.25rem;
    height: 24px;
    padding-left: 0.55rem;
    padding-right: 0.25rem;
    border: 1px solid color-mix(in oklch, var(--accent) 45%, var(--border));
    border-radius: 99px;
    background: var(--accent-soft);
    color: var(--text);
  }
</style>
