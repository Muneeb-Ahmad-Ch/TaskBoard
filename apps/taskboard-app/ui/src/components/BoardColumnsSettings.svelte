<script lang="ts">
  // Settings → Board columns: the columns new projects start with (name,
  // what it means, color, order), and adding the missing ones to the
  // projects you already have. Columns belong to each project, so one
  // project can keep its own workflow; this is the starting set.
  import { ArrowDown, ArrowUp, Plus, Trash2 } from '@lucide/svelte'
  import type { Category } from '../lib/bindings/Category'
  import type { DefaultColumn } from '../lib/bindings/DefaultColumn'
  import { PALETTE, plural, swatch } from '../lib/format'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { act, app, undo } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'

  let { onchange = () => {} }: { onchange?: () => void } = $props()

  let cols = $state<DefaultColumn[]>([])
  let missing = $state(0)
  let newName = $state('')
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)

  const MEANINGS: { value: Category; label: string }[] = [
    { value: 'todo', label: 'To do' },
    { value: 'active', label: 'In progress' },
    { value: 'done', label: 'Done' },
    { value: 'cancelled', label: 'Cancelled' },
  ]

  $effect(() => {
    void app.dataVersion
    if (!inTauri) return
    let stale = false
    Promise.all([ipc.defaultColumns(), ipc.projectsMissingColumns()])
      .then(([c, m]) => {
        if (stale) return
        cols = c
        missing = m
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  /** Saves the whole list; a refused change (say, no Done column left) puts it back. */
  async function save(next: DefaultColumn[]): Promise<boolean> {
    const before = cols
    cols = next
    try {
      cols = await ipc.setDefaultColumns(next)
      missing = await ipc.projectsMissingColumns()
      onchange()
      return true
    } catch (err) {
      cols = before
      toast(errorMessage(err), { kind: 'error' })
      return false
    }
  }

  const update = (i: number, patch: Partial<DefaultColumn>) => save(cols.map((c, k) => (k === i ? { ...c, ...patch } : c)))

  async function rename(i: number, input: HTMLInputElement) {
    const name = input.value.trim()
    if (!name || name === cols[i]?.name) {
      input.value = cols[i]?.name ?? ''
      return
    }
    if (!(await update(i, { name }))) input.value = cols[i]?.name ?? ''
  }

  function move(i: number, dir: -1 | 1) {
    const next = cols.slice()
    const j = i + dir
    if (j < 0 || j >= next.length) return
    ;[next[i], next[j]] = [next[j], next[i]]
    void save(next)
  }

  /** A new column goes before the finished ones, as in-progress work. */
  async function add() {
    const name = newName.trim()
    if (!name) return
    const at = cols.findIndex((c) => c.category === 'done' || c.category === 'cancelled')
    const next = cols.slice()
    next.splice(at < 0 ? next.length : at, 0, { name, category: 'active', color: '70' })
    if (await save(next)) newName = ''
  }

  function colorMenu(e: MouseEvent, i: number) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    menu = {
      x: r.left,
      y: r.bottom + 2,
      items: PALETTE.map((c) => ({
        label: c === 'gray' ? 'Gray' : `Hue ${c}`,
        color: c,
        checked: c === cols[i]?.color,
        run: () => void update(i, { color: c }),
      })),
    }
  }

  async function addToProjects() {
    const n = await act(() => ipc.addMissingColumns())
    if (n === undefined) return
    if (n) toast(`Added the missing columns to ${plural(n, 'project')}`, { action: { label: 'Undo', run: () => void undo() } })
    else toast('Every project already has these columns')
    missing = await ipc.projectsMissingColumns().catch(() => 0)
  }

  async function restore() {
    try {
      cols = await ipc.resetDefaultColumns()
      missing = await ipc.projectsMissingColumns()
      onchange()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }
</script>

<p class="mb-3 text-xs text-muted">
  Each project has its own board columns, so a project can follow its own workflow. New projects start with these. To change one
  project's columns, use the ⋯ menu on a column in its Board.
</p>

<section class="card" data-setting="columns-default">
  <h4 class="card-title">Columns for new projects</h4>
  <ul aria-label="Columns for new projects">
    {#each cols as c, i (i)}
      <li class="item">
        <button class="dot" style="--c: {swatch(c.color, 0.62, 0.13)}" aria-label="Color of {c.name}" title="Color" onclick={(e) => colorMenu(e, i)}></button>
        <input
          class="name"
          value={c.name}
          aria-label="Name of column {i + 1}"
          onkeydown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur()
            if (e.key === 'Escape') {
              e.stopPropagation()
              e.currentTarget.value = c.name
              e.currentTarget.blur()
            }
          }}
          onblur={(e) => void rename(i, e.currentTarget)}
        />
        <select class="meaning" aria-label="What {c.name} means" value={c.category} onchange={(e) => void update(i, { category: e.currentTarget.value as Category })}>
          {#each MEANINGS as m (m.value)}<option value={m.value}>{m.label}</option>{/each}
        </select>
        <button class="order" aria-label="Move {c.name} up" title="Move up" disabled={i === 0} onclick={() => move(i, -1)}><ArrowUp size={14} /></button>
        <button class="order" aria-label="Move {c.name} down" title="Move down" disabled={i === cols.length - 1} onclick={() => move(i, 1)}><ArrowDown size={14} /></button>
        <button class="order" aria-label="Remove {c.name}" title="Remove from the starting columns" onclick={() => void save(cols.filter((_, k) => k !== i))}><Trash2 size={13} /></button>
      </li>
    {/each}
    <li class="item">
      <Plus size={14} class="mx-[3px] shrink-0 text-subtle" />
      <input
        class="name"
        placeholder="New column, e.g. Blocked"
        aria-label="New column name"
        bind:value={newName}
        onkeydown={(e) => e.key === 'Enter' && void add()}
      />
      <button class="btn !h-7 text-xs" disabled={!newName.trim()} onclick={() => void add()}>Add</button>
    </li>
  </ul>
  <p class="px-4 pb-3 text-xs text-muted">
    "To do" and "In progress" columns hold open work; a task moved to a "Done" column counts as finished. Keep at least one of each.
  </p>
</section>

<section class="card" data-setting="columns-apply">
  <div class="set-row">
    <div class="min-w-0 flex-1">
      <div class="text-[13px] font-medium">Add missing columns to your projects</div>
      <div class="mt-0.5 text-xs text-muted">
        {missing ? `${plural(missing, 'project')} ${missing === 1 ? 'lacks' : 'lack'} some of these columns.` : 'Every project has all of these columns.'}
        Nothing is removed or renamed; Ctrl+Z undoes it.
      </div>
    </div>
    <button class="btn !h-7 text-xs" disabled={!missing} onclick={() => void addToProjects()}>Add to all projects</button>
  </div>
</section>

<button class="btn !h-7 text-xs" onclick={() => void restore()}>Restore the built-in columns</button>

{#if menu}
  <Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

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
    padding: 4px 16px;
  }
  .item + .item {
    border-top: 1px solid var(--border);
  }
  .set-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    min-height: 56px;
    padding: 10px 16px;
  }
  .dot {
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: 6px;
  }
  .dot::after {
    content: '';
    width: 10px;
    height: 10px;
    border-radius: 99px;
    background: var(--c);
  }
  .dot:hover {
    background: var(--hover);
  }
  .name {
    min-width: 0;
    flex: 1;
    height: 28px;
    padding-inline: 6px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    font-size: 13px;
    outline: none;
  }
  .name:hover {
    border-color: var(--border);
  }
  .name:focus {
    border-color: var(--accent);
    background: var(--surface-2);
  }
  .meaning {
    height: 28px;
    padding-inline: 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    font-size: 12px;
    color: var(--text);
  }
  .order {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex-shrink: 0;
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
