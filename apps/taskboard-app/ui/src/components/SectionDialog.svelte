<script lang="ts">
  // Create or edit a sidebar section: name, what one group in it is called,
  // and its color.
  import { untrack } from 'svelte'
  import type { Id } from '../lib/bindings/Id'
  import { PALETTE, singular, swatch } from '../lib/format'
  import { ipc } from '../lib/ipc'
  import { act, app, sectionById, setScope } from '../lib/store.svelte'
  import Modal from './Modal.svelte'

  let { id, onclose }: { id: Id | null; onclose: () => void } = $props()

  const existing = untrack(() => sectionById(id))
  let name = $state(existing?.name ?? '')
  let item = $state(existing?.item_name ?? '')
  /** Until the user types an item name, it follows the section name. */
  let itemTouched = $state(!!existing?.item_name)
  let color = $state(existing?.color ?? PALETTE[(untrack(() => app.sections.length) + 2) % PALETTE.length])
  let saving = $state(false)

  function onName(value: string) {
    name = value
    if (!itemTouched) item = singular(value)
  }

  async function save() {
    if (!name.trim() || saving) return
    saving = true
    const input = { name, item_name: item, color }
    const result = existing
      ? await act(() => ipc.updateSection(existing.id, input), `Saved “${name.trim()}”`)
      : await act(() => ipc.createSection(input), `Added section “${name.trim()}”`)
    saving = false
    if (!result) return
    if (!existing) setScope({ kind: 'section', id: result.id })
    onclose()
  }
</script>

<Modal title={existing ? 'Edit section' : 'New section'} {onclose} width={440}>
  <form class="flex flex-col gap-4" onsubmit={(e) => { e.preventDefault(); void save() }}>
    <label>
      <span class="field-label">Name</span>
      <input class="field" placeholder="e.g. Clients, Teams, Family" value={name} oninput={(e) => onName(e.currentTarget.value)} />
    </label>
    <label>
      <span class="field-label">One item in it is called</span>
      <input class="field" placeholder="group" value={item} oninput={(e) => { item = e.currentTarget.value; itemTouched = true }} />
      <span class="mt-1 block text-xs text-muted">Used in buttons like “Add {item.trim() || 'group'}”.</span>
    </label>
    <div>
      <span class="field-label">Color</span>
      <div class="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Section color">
        {#each PALETTE as c (c)}
          <button type="button" role="radio" aria-checked={color === c} aria-label="Color {c}" class="swatch" style="--s: {swatch(c)}" onclick={() => (color = c)}></button>
        {/each}
      </div>
    </div>
    <button type="submit" class="sr-only">Save</button>
  </form>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={!name.trim() || saving} onclick={() => void save()}>{existing ? 'Save' : 'Add section'}</button>
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
