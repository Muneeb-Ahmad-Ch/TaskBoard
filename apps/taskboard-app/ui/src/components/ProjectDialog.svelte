<script lang="ts">
  import { untrack } from 'svelte'
  import type { Id } from '../lib/bindings/Id'
  import type { ProjectState } from '../lib/bindings/ProjectState'
  import { PALETTE, swatch } from '../lib/format'
  import { errorMessage, ipc } from '../lib/ipc'
  import { act, app } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Modal from './Modal.svelte'
  import Progress from './Progress.svelte'

  let { id, onclose }: { id: Id; onclose: () => void } = $props()

  const project = untrack(() => app.projects.find((p) => p.id === id))
  const f = $state({
    name: project?.name ?? '',
    color: project?.color ?? '262',
    client: project?.client_id ?? '',
    code: project?.code ?? '',
    state: (project?.state ?? 'active') as ProjectState,
    start: project?.start_date ?? '',
    deadline: project?.deadline ?? '',
  })
  let saving = $state(false)

  async function save() {
    if (!f.name.trim()) return toast('Give the project a name', { kind: 'error' })
    saving = true
    try {
      await ipc.updateProject(id, {
        name: f.name,
        color: f.color,
        client_id: project?.is_inbox ? undefined : f.client || null,
        code: f.code || null,
        state: f.state,
        start_date: f.start || null,
        deadline: f.deadline || null,
      })
      toast('Project saved', { action: { label: 'Undo', run: () => void ipc.undo() } })
      onclose()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      saving = false
    }
  }
</script>

<Modal title="Project settings" {onclose} width={520}>
  {#if !project}
    <p class="text-muted">This project no longer exists.</p>
  {:else}
    <div class="flex flex-col gap-3">
      <div class="grid grid-cols-[1fr_140px] gap-2">
        <label><span class="field-label">Name</span><input class="field" bind:value={f.name} /></label>
        <label><span class="field-label">Job / PO code</span><input class="field" placeholder="Optional" bind:value={f.code} /></label>
      </div>
      <div>
        <span class="field-label">Color</span>
        <div class="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Color">
          {#each PALETTE as c (c)}
            <button role="radio" aria-checked={f.color === c} aria-label={c === 'gray' ? 'Gray' : `Hue ${c}`} class="swatch" style="--s: {swatch(c, 0.6)}" onclick={() => (f.color = c)}></button>
          {/each}
        </div>
      </div>
      <div class="grid grid-cols-2 gap-2">
        <label>
          <span class="field-label">Client</span>
          <select class="field" bind:value={f.client} disabled={project.is_inbox}>
            <option value="">Personal (no client)</option>
            {#each app.clients as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
          </select>
        </label>
        <label>
          <span class="field-label">State</span>
          <select class="field" bind:value={f.state}>
            <option value="active">Active</option>
            <option value="on_hold">On hold</option>
            <option value="completed">Completed</option>
          </select>
        </label>
        <label><span class="field-label">Start date</span><input type="date" class="field" bind:value={f.start} /></label>
        <label><span class="field-label">Deadline</span><input type="date" class="field" bind:value={f.deadline} /></label>
      </div>
      <div class="flex items-center gap-3 rounded-md border border-border px-3 py-2 text-[13px]">
        <span class="text-muted">Progress</span>
        <Progress value={project.total_tasks ? (project.done_tasks / project.total_tasks) * 100 : 0} width={140} label />
        <span class="text-xs text-subtle">{project.done_tasks} of {project.total_tasks} tasks done · {project.open_tasks} open</span>
      </div>
    </div>
  {/if}

  {#snippet footer()}
    {#if project && !project.is_inbox}
      <button class="btn" onclick={() => void act(() => ipc.setProjectArchived(id, true), `Archived “${project.name}”`).then(onclose)}>Archive</button>
    {/if}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={saving || !project} onclick={() => void save()}>Save</button>
  {/snippet}
</Modal>

<style>
  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 99px;
    background: var(--s);
  }
  .swatch[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface-2),
      0 0 0 4px var(--s);
  }
</style>
