<script lang="ts">
  // Settings → Data: archived groups and projects, with Restore and Delete.
  // Restoring a group brings back the projects archived with it.
  import { RotateCcw, Trash2 } from '@lucide/svelte'
  import type { Client } from '../lib/bindings/Client'
  import type { ProjectSummary } from '../lib/bindings/ProjectSummary'
  import { swatch } from '../lib/format'
  import { inTauri, ipc } from '../lib/ipc'
  import { act, app, itemName, sectionById } from '../lib/store.svelte'

  let groups = $state<Client[]>([])
  let projects = $state<ProjectSummary[]>([])
  /** The row whose Delete was clicked once and now asks to confirm. */
  let confirming = $state<string | null>(null)

  $effect(() => {
    void app.dataVersion
    if (!inTauri) return
    let stale = false
    Promise.all([ipc.listClients(true), ipc.listProjects(true)])
      .then(([cs, ps]) => {
        if (stale) return
        groups = cs.filter((c) => c.archived_at !== null)
        const archivedGroups = new Set(groups.map((c) => c.id))
        // Projects inside an archived group come back with it; list the rest.
        projects = ps.filter((p) => p.archived_at !== null && !(p.client_id && archivedGroups.has(p.client_id)))
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  function where(sectionId: string | null, group: string | null): string {
    return [sectionById(sectionId)?.name, group].filter(Boolean).join(' › ')
  }

  async function remove(key: string, run: () => Promise<unknown>, label: string) {
    if (confirming !== key) {
      confirming = key
      return
    }
    confirming = null
    await act(async () => {
      await run()
      return true
    }, label)
  }
</script>

<section class="card">
  <h4 class="card-title">Archived</h4>
  {#if !groups.length && !projects.length}
    <p class="px-4 pb-3 text-xs text-muted">Nothing archived. Archive a group or project from its right-click menu to hide it without losing anything.</p>
  {:else}
    <ul class="list" aria-label="Archived groups and projects">
      {#each groups as c (c.id)}
        {@const sec = sectionById(c.section_id)}
        <li>
          <span class="grid size-[16px] shrink-0 place-items-center rounded text-[9.5px] font-bold text-white" style="background: {swatch(c.color, 0.6)}">{c.name.charAt(0).toUpperCase()}</span>
          <span class="flex min-w-0 flex-1 flex-col">
            <span class="truncate text-[13px]">{c.name}</span>
            <span class="truncate text-xs text-muted">{itemName(sec)}{sec ? ` in ${sec.name}` : ''} · its projects come back with it</span>
          </span>
          <button class="btn !h-6 !px-2 text-xs" onclick={() => void act(() => ipc.setClientArchived(c.id, false), `Restored “${c.name}”`)}><RotateCcw size={12} /> Restore</button>
          <button
            class="btn !h-6 !px-2 text-xs {confirming === c.id ? 'btn-danger' : ''}"
            title="Only empty groups can be deleted; restore it to move or delete its projects first"
            onclick={() => void remove(c.id, () => ipc.deleteClient(c.id), `Deleted “${c.name}”`)}
          ><Trash2 size={12} /> {confirming === c.id ? 'Click to confirm' : 'Delete'}</button>
        </li>
      {/each}
      {#each projects as p (p.id)}
        <li>
          <span class="size-2 shrink-0 rounded-full" style="background: {swatch(p.color)}"></span>
          <span class="flex min-w-0 flex-1 flex-col">
            <span class="truncate text-[13px]">{p.name}</span>
            <span class="truncate text-xs text-muted">Project{where(p.section_id, p.client_name) ? ` in ${where(p.section_id, p.client_name)}` : ''} · {p.total_tasks} task{p.total_tasks === 1 ? '' : 's'}</span>
          </span>
          <button class="btn !h-6 !px-2 text-xs" onclick={() => void act(() => ipc.setProjectArchived(p.id, false), `Restored “${p.name}”`)}><RotateCcw size={12} /> Restore</button>
          <button
            class="btn !h-6 !px-2 text-xs {confirming === p.id ? 'btn-danger' : ''}"
            title="Deletes the project and its tasks (undoable)"
            onclick={() => void remove(p.id, () => ipc.deleteProject(p.id), `Deleted “${p.name}”`)}
          ><Trash2 size={12} /> {confirming === p.id ? 'Click to confirm' : 'Delete'}</button>
        </li>
      {/each}
    </ul>
  {/if}
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
  .list {
    display: flex;
    flex-direction: column;
    padding: 0 12px 10px;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 4px;
    border-top: 1px solid var(--border);
  }
</style>
