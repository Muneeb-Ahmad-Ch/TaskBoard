<script lang="ts">
  // Filter, Sort and Group for the table and the board (in the page header).
  // The filter is shared by both views; the board keeps its own card order.
  import { ArrowUpDown, Layers, ListFilter } from '@lucide/svelte'
  import type { Priority } from '../lib/bindings/Priority'
  import type { TaskSort } from '../lib/bindings/TaskSort'
  import { buildColumns } from '../lib/board'
  import { ADDED_LABEL, DUE_LABEL, filterCount, type AddedFilter, type DueFilter } from '../lib/filters'
  import { swatch } from '../lib/format'
  import { GROUP_LABEL, type GroupBy } from '../lib/grouping'
  import { inTauri, ipc } from '../lib/ipc'
  import { app, clearFilter, scopeProjectIds } from '../lib/store.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'

  const board = $derived(app.view === 'board')

  // ---------- sort ----------
  const SORT_LABEL: Record<TaskSort, string> = {
    manual: 'Board order',
    title: 'Task name',
    project: 'Group / Project',
    status: 'Status',
    priority: 'Priority',
    due: 'Due date',
    progress: 'Progress',
    estimate: 'Estimate',
    tags: 'Tags',
    created: 'Date added',
    updated: 'Last changed',
  }
  const TABLE_SORTS: TaskSort[] = ['due', 'priority', 'title', 'project', 'status', 'progress', 'estimate', 'tags', 'created', 'updated', 'manual']
  const BOARD_SORTS: TaskSort[] = ['manual', 'due', 'priority', 'title', 'project', 'progress', 'estimate', 'created', 'updated']
  /** [ascending, descending] in words that fit the field. */
  const DIRECTION: Partial<Record<TaskSort, [string, string]>> = {
    due: ['Soonest first', 'Latest first'],
    priority: ['Highest first', 'Lowest first'],
    title: ['A to Z', 'Z to A'],
    project: ['A to Z', 'Z to A'],
    tags: ['A to Z', 'Z to A'],
    status: ['To do first', 'Done first'],
    progress: ['Least done first', 'Most done first'],
    estimate: ['Shortest first', 'Longest first'],
    created: ['Oldest first', 'Newest first'],
    updated: ['Oldest first', 'Newest first'],
  }
  /** Fields people mostly want biggest / newest first. */
  const FIRST_DESCENDING: TaskSort[] = ['progress', 'created', 'updated']

  const sort = $derived(board ? app.boardSort : app.sort)
  const descending = $derived(board ? app.boardDescending : app.descending)

  function setSort(s: TaskSort, desc = FIRST_DESCENDING.includes(s)) {
    if (board) {
      app.boardSort = s
      app.boardDescending = s === 'manual' ? false : desc
    } else {
      app.sort = s
      app.descending = desc
    }
  }

  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)

  function at(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    return { x: r.left, y: r.bottom + 4 }
  }

  function sortMenu(e: MouseEvent) {
    const words = DIRECTION[sort]
    const items: MenuEntry[] = (board ? BOARD_SORTS : TABLE_SORTS).map((s) => ({
      label: SORT_LABEL[s],
      checked: s === sort,
      run: () => setSort(s),
    }))
    if (words) {
      items.push(
        'separator',
        { label: words[0], checked: !descending, run: () => setSort(sort, false) },
        { label: words[1], checked: descending, run: () => setSort(sort, true) },
      )
    }
    menu = { ...at(e), items }
  }

  // ---------- group (table) ----------
  const GROUPS: GroupBy[] = ['none', 'status', 'priority', 'project', 'client', 'due', 'added']

  function groupMenu(e: MouseEvent) {
    menu = {
      ...at(e),
      items: GROUPS.map((g) => ({ label: GROUP_LABEL[g], checked: app.groupBy === g, run: () => (app.groupBy = g) })),
    }
  }

  // ---------- filter ----------
  let open = $state(false)
  let filterButton: HTMLButtonElement | undefined = $state()
  let pop: HTMLDivElement | undefined = $state()
  let popAt = $state({ right: 16, top: 56 })
  let tags = $state<string[]>([])
  const count = $derived(filterCount(app.filter))

  const PRIORITIES: { value: Priority; label: string }[] = [
    { value: 'urgent', label: 'Urgent' },
    { value: 'high', label: 'High' },
    { value: 'med', label: 'Medium' },
    { value: 'low', label: 'Low' },
    { value: 'none', label: 'No priority' },
  ]
  const DUES: DueFilter[] = ['overdue', 'today', 'week', 'dated', 'none']
  const ADDEDS: AddedFilter[] = ['today', 'week', 'month']
  const columnNames = $derived(buildColumns(app.statuses, scopeProjectIds()).map((c) => c.name))
  const scopeProjects = $derived.by(() => {
    const ids = new Set(scopeProjectIds())
    return app.projects.filter((p) => ids.has(p.id) && !(p.is_inbox && ids.size > 1))
  })

  async function openFilter() {
    if (open) return closeFilter()
    const r = filterButton?.getBoundingClientRect()
    if (r) popAt = { right: Math.max(8, window.innerWidth - r.right), top: r.bottom + 6 }
    open = true
    if (inTauri) tags = await ipc.listTags().catch(() => [])
  }

  function closeFilter(refocus = true) {
    open = false
    if (refocus) filterButton?.focus()
  }

  // First pick in the panel gets focus when it opens.
  $effect(() => {
    if (open && pop) pop.querySelector<HTMLElement>('button')?.focus()
  })

  function toggle<T>(list: T[], value: T): T[] {
    return list.includes(value) ? list.filter((v) => v !== value) : [...list, value]
  }

  const sameName = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase()

  function toggleStatus(name: string) {
    const has = app.filter.statuses.some((s) => sameName(s, name))
    app.filter.statuses = has ? app.filter.statuses.filter((s) => !sameName(s, name)) : [...app.filter.statuses, name]
  }

  function onPopKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      closeFilter()
    }
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (open && pop && !pop.contains(e.target as Node) && !filterButton?.contains(e.target as Node)) closeFilter(false)
  }}
/>

<button
  bind:this={filterButton}
  class="ctl {count ? 'on' : ''}"
  aria-haspopup="dialog"
  aria-expanded={open}
  title="Filter by priority, status, tag, project or date"
  onclick={() => void openFilter()}
>
  <ListFilter size={14} />
  <span class="max-[1180px]:sr-only">Filter</span>
  {#if count}<span class="badge tabular" aria-label="{count} on">{count}</span>{/if}
</button>
<button class="ctl {sort !== (board ? 'manual' : 'due') ? 'on' : ''}" aria-haspopup="menu" title="Sort {board ? 'cards' : 'tasks'}: {SORT_LABEL[sort]}" onclick={sortMenu}>
  <ArrowUpDown size={14} />
  <span class="max-[1180px]:sr-only">Sort<span class="max-[1400px]:sr-only">: {SORT_LABEL[sort]}</span></span>
</button>
{#if !board}
  <button class="ctl {app.groupBy !== 'none' ? 'on' : ''}" aria-haspopup="menu" title="Group tasks: {GROUP_LABEL[app.groupBy]}" onclick={groupMenu}>
    <Layers size={14} />
    <span class="max-[1180px]:sr-only">Group<span class="max-[1400px]:sr-only">{app.groupBy !== 'none' ? `: ${GROUP_LABEL[app.groupBy]}` : ''}</span></span>
  </button>
{/if}

{#if menu}
  <Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if open}
  <div bind:this={pop} class="pop" style="right: {popAt.right}px; top: {popAt.top}px" role="dialog" aria-label="Filter tasks" tabindex="-1" onkeydown={onPopKey}>
    <section aria-labelledby="f-priority">
      <h3 id="f-priority">Priority</h3>
      <div class="chips">
        {#each PRIORITIES as p (p.value)}
          <button class="chip" aria-pressed={app.filter.priorities.includes(p.value)} onclick={() => (app.filter.priorities = toggle(app.filter.priorities, p.value))}>
            {#if p.value !== 'none'}<span class="size-1.5 rounded-full" style="background: var(--priority-{p.value})"></span>{/if}{p.label}
          </button>
        {/each}
      </div>
    </section>

    {#if columnNames.length}
      <section aria-labelledby="f-status">
        <h3 id="f-status">Status</h3>
        <div class="chips">
          {#each columnNames as name (name)}
            <button class="chip" aria-pressed={app.filter.statuses.some((s) => sameName(s, name))} onclick={() => toggleStatus(name)}>{name}</button>
          {/each}
        </div>
      </section>
    {/if}

    <section aria-labelledby="f-due">
      <h3 id="f-due">Due</h3>
      <div class="chips" role="radiogroup" aria-labelledby="f-due">
        {#each DUES as d (d)}
          <button class="chip" role="radio" aria-checked={app.filter.due === d} onclick={() => (app.filter.due = app.filter.due === d ? null : d)}>{DUE_LABEL[d]}</button>
        {/each}
      </div>
    </section>

    <section aria-labelledby="f-added">
      <h3 id="f-added">Added</h3>
      <div class="chips" role="radiogroup" aria-labelledby="f-added">
        {#each ADDEDS as a (a)}
          <button class="chip" role="radio" aria-checked={app.filter.added === a} onclick={() => (app.filter.added = app.filter.added === a ? null : a)}>{ADDED_LABEL[a]}</button>
        {/each}
      </div>
    </section>

    {#if tags.length || app.filter.tags.length}
      <section aria-labelledby="f-tags">
        <h3 id="f-tags">Tags <span class="font-normal text-subtle">· any of them</span></h3>
        <div class="chips scroll">
          {#each [...new Set([...app.filter.tags, ...tags])] as tag (tag.toLowerCase())}
            <button class="chip" aria-pressed={app.filter.tags.some((t) => sameName(t, tag))} onclick={() => (app.filter.tags = app.filter.tags.some((t) => sameName(t, tag)) ? app.filter.tags.filter((t) => !sameName(t, tag)) : [...app.filter.tags, tag])}>#{tag}</button>
          {/each}
        </div>
      </section>
    {/if}

    {#if scopeProjects.length > 1}
      <section aria-labelledby="f-project">
        <h3 id="f-project">Project</h3>
        <div class="chips scroll">
          {#each scopeProjects as p (p.id)}
            <button class="chip" aria-pressed={app.filter.projects.includes(p.id)} onclick={() => (app.filter.projects = toggle(app.filter.projects, p.id))}>
              <span class="size-1.5 shrink-0 rounded-full" style="background: {swatch(p.color)}"></span>
              {p.client_name && !(app.display.hide_repeated_context && app.scope.kind === 'client') ? `${p.client_name} / ${p.name}` : p.name}
            </button>
          {/each}
        </div>
      </section>
    {/if}

    <div class="flex items-center justify-between border-t border-border pt-2.5">
      <button class="btn !h-7 text-xs" disabled={!count} onclick={clearFilter}>Clear all</button>
      <button class="btn !h-7 text-xs" onclick={() => closeFilter()}>Done</button>
    </div>
  </div>
{/if}

<style>
  .ctl {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.4rem;
    height: 28px;
    padding-inline: 0.6rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text-muted);
    white-space: nowrap;
  }
  .ctl:hover {
    color: var(--text);
  }
  .ctl.on {
    color: var(--text);
    border-color: color-mix(in oklch, var(--accent) 55%, var(--border));
  }
  .badge {
    min-width: 16px;
    height: 16px;
    padding-inline: 4px;
    border-radius: 99px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 10.5px;
    font-weight: 600;
    line-height: 16px;
    text-align: center;
  }
  .pop {
    position: fixed;
    z-index: 50;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: min(360px, calc(100vw - 16px));
    max-height: calc(100vh - 96px);
    overflow-y: auto;
    padding: 0.85rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.22);
    outline: none;
    animation: pop 120ms var(--ease-out);
  }
  h3 {
    margin-bottom: 0.4rem;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .chips.scroll {
    max-height: 132px;
    overflow-y: auto;
  }
  .chip {
    display: inline-flex;
    max-width: 100%;
    align-items: center;
    gap: 0.35rem;
    height: 26px;
    padding-inline: 0.55rem;
    border: 1px solid var(--border);
    border-radius: 99px;
    color: var(--text-muted);
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip:hover {
    color: var(--text);
    background: var(--hover);
  }
  .chip[aria-pressed='true'],
  .chip[aria-checked='true'] {
    color: var(--text);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-2px);
    }
  }
</style>
