<script lang="ts">
  // Delete a section: move its groups and projects to another section, or
  // delete everything in it. Either way one Undo brings it all back.
  import { untrack } from 'svelte'
  import type { Id } from '../lib/bindings/Id'
  import { ipc } from '../lib/ipc'
  import { act, app, groupsIn, itemName, looseProjectsIn, sectionById } from '../lib/store.svelte'
  import Modal from './Modal.svelte'

  let { id, onclose }: { id: Id; onclose: () => void } = $props()

  const section = untrack(() => sectionById(id))
  const others = $derived(app.sections.filter((s) => s.id !== id))
  const groups = $derived(groupsIn(id).length)
  const projects = $derived(app.projects.filter((p) => p.section_id === id && !p.is_inbox).length)
  const loose = $derived(looseProjectsIn(id).length)
  let mode = $state<'move' | 'everything'>('move')
  let target = $state<Id | ''>(untrack(() => app.sections.find((s) => s.id !== id)?.id ?? ''))

  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`

  async function confirm() {
    if (!section) return
    const how = mode === 'move' && target ? { kind: 'move_to' as const, section_id: target } : { kind: 'everything' as const }
    // act() resolves undefined on failure; a void command needs an explicit result.
    const ok = await act(async () => {
      await ipc.deleteSection(section.id, how)
      return true
    }, `Deleted section “${section.name}”`)
    if (ok) onclose()
  }
</script>

<Modal title="Delete section" {onclose} width={460}>
  {#if !section}
    <p>This section no longer exists.</p>
  {:else if !others.length}
    <p>“{section.name}” is your only section. Add another section first; new projects need somewhere to go.</p>
  {:else}
    <p class="mb-3">
      “{section.name}” has {plural(groups, itemName(section))} and {plural(projects, 'project')}{loose && groups ? ` (${loose} not in a ${itemName(section)})` : ''}.
    </p>
    <fieldset class="flex flex-col gap-2 text-[13px]">
      <legend class="sr-only">What happens to its contents</legend>
      <label class="flex items-center gap-2">
        <input type="radio" name="how" value="move" bind:group={mode} />
        Move everything to
        <select class="field !h-7 !w-auto" aria-label="Section to move to" bind:value={target} disabled={mode !== 'move'}>
          {#each others as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
        </select>
      </label>
      <label class="flex items-center gap-2 text-danger">
        <input type="radio" name="how" value="everything" bind:group={mode} />
        Delete everything in it (tasks included)
      </label>
    </fieldset>
    <p class="mt-3 text-xs text-muted">You can undo this with Ctrl+Z.</p>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    {#if section && others.length}
      <button class="btn {mode === 'everything' ? 'btn-danger' : 'btn-primary'}" onclick={() => void confirm()}>Delete section</button>
    {/if}
  {/snippet}
</Modal>
