<script lang="ts">
  import { CheckCircle2, Circle, Copy, Plus, Trash2, X } from '@lucide/svelte'
  import type { Priority } from '../lib/bindings/Priority'
  import type { Status } from '../lib/bindings/Status'
  import type { Task } from '../lib/bindings/Task'
  import type { TaskPatch } from '../lib/bindings/TaskPatch'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import { minutes, parseMinutes, swatch } from '../lib/format'
  import { errorMessage, ipc } from '../lib/ipc'
  import { act, app } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'

  let task = $state<Task | null>(null)
  let statuses = $state<Status[]>([])
  let subtasks = $state<TaskRow[]>([])
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)
  let newSubtask = $state('')

  // Load (and reload on any data change) the open task.
  $effect(() => {
    const id = app.openTaskId
    void app.dataVersion
    if (!id) {
      task = null
      return
    }
    let cancelled = false
    ;(async () => {
      try {
        const t = await ipc.getTask(id)
        const [st, subs] = await Promise.all([
          ipc.listStatuses(t.project_id),
          ipc.listTasks({ parent_id: id, sort: 'manual', limit: 500 }),
        ])
        if (cancelled) return
        task = t
        statuses = st
        subtasks = subs.rows
      } catch {
        // Deleted or moved out from under us.
        if (!cancelled) app.openTaskId = null
      }
    })()
    return () => {
      cancelled = true
    }
  })

  const project = $derived(app.projects.find((p) => p.id === task?.project_id))
  const status = $derived(statuses.find((s) => s.id === task?.status_id))
  const done = $derived(!!task?.completed_at)

  async function save(patch: TaskPatch) {
    if (!task) return
    try {
      task = await ipc.updateTask(task.id, patch)
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  function openMenu(e: MouseEvent, items: MenuEntry[]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    menu = { x: r.left, y: r.bottom + 2, items }
  }

  const PRIORITIES: { value: Priority; label: string }[] = [
    { value: 'urgent', label: 'Urgent' },
    { value: 'high', label: 'High' },
    { value: 'med', label: 'Medium' },
    { value: 'low', label: 'Low' },
    { value: 'none', label: 'No priority' },
  ]

  function commitTitle(value: string) {
    const title = value.trim()
    if (task && title && title !== task.title) void save({ title })
  }

  function commitTags(value: string) {
    if (!task) return
    const tags = value
      .split(/[,\s]+/)
      .map((t) => t.replace(/^#/, '').trim())
      .filter(Boolean)
    if (tags.join(',') !== task.tags.join(',')) void save({ tags })
  }

  function commitEstimate(value: string) {
    if (!task) return
    const m = parseMinutes(value)
    if (Number.isNaN(m)) toast('Use a duration like 45m, 2h or 1h30m', { kind: 'error' })
    else if (m !== task.estimate_minutes) void save({ estimate_minutes: m })
  }

  async function addSubtask() {
    const title = newSubtask.trim()
    if (!task || !title) return
    newSubtask = ''
    await act(() => ipc.createTask({ title, parent_id: task!.id }))
  }

  function close() {
    app.openTaskId = null
  }

  const fmt = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' })
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape' && app.openTaskId && !menu && !(e.target as HTMLElement).closest('input, textarea')) close()
  }}
/>

{#if task}
  <aside class="panel flex w-[380px] shrink-0 flex-col border-l border-border bg-surface-1" aria-label="Task details">
    <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
      <button
        class="flex h-7 items-center gap-1.5 rounded-md border px-2 text-xs font-medium {done ? 'border-success text-success' : 'border-border hover:border-border-strong'}"
        onclick={() => task && void act(() => ipc.setTaskDone(task!.id, !done))}
      >
        {#if done}<CheckCircle2 size={14} /> Done{:else}<Circle size={14} /> Mark done{/if}
      </button>
      <div class="flex-1"></div>
      <button class="icon" title="Duplicate" aria-label="Duplicate task" onclick={() => task && void act(() => ipc.duplicateTask(task!.id), 'Task duplicated')}><Copy size={15} /></button>
      <button class="icon hover:text-danger" title="Delete" aria-label="Delete task" onclick={() => task && void act(() => ipc.deleteTask(task!.id), 'Task deleted').then(close)}><Trash2 size={15} /></button>
      <button class="icon" title="Close (Esc)" aria-label="Close details" onclick={close}><X size={16} /></button>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4">
      {#key task.id}
        <textarea
          class="title-input w-full resize-none bg-transparent font-display text-[17px] leading-snug font-semibold outline-none"
          rows="2"
          value={task.title}
          aria-label="Title"
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              e.currentTarget.blur()
            }
          }}
          onblur={(e) => commitTitle(e.currentTarget.value)}
        ></textarea>
      {/key}

      <dl class="props mt-3">
        <dt>Project</dt>
        <dd>
          <button class="prop-btn" onclick={(e) => openMenu(e, app.projects.map((p) => ({
            label: p.client_name ? `${p.client_name} / ${p.name}` : p.name,
            color: p.color,
            checked: p.id === task?.project_id,
            run: () => void save({ project_id: p.id }),
          })))}>
            {#if project}
              <span class="size-2 rounded-full" style="background: {swatch(project.color)}"></span>
              {#if project.client_name}<span class="text-muted">{project.client_name} /</span>{/if}
              {project.name}
            {/if}
          </button>
        </dd>

        <dt>Status</dt>
        <dd>
          <button class="prop-btn" onclick={(e) => openMenu(e, statuses.map((s) => ({
            label: s.name,
            color: s.color,
            checked: s.id === task?.status_id,
            run: () => void save({ status_id: s.id }),
          })))}>
            {#if status}<span class="size-2 rounded-full" style="background: {swatch(status.color, 0.62, 0.13)}"></span>{status.name}{/if}
          </button>
        </dd>

        <dt>Priority</dt>
        <dd>
          <button class="prop-btn" onclick={(e) => openMenu(e, PRIORITIES.map((p) => ({
            label: p.label,
            checked: p.value === task?.priority,
            run: () => void save({ priority: p.value }),
          })))}>
            {#if task.priority !== 'none'}
              <span class="size-2 rounded-full" style="background: var(--priority-{task.priority})"></span>
              {PRIORITIES.find((p) => p.value === task?.priority)?.label}
            {:else}<span class="text-subtle">None</span>{/if}
          </button>
        </dd>

        <dt>Start</dt>
        <dd>
          <input type="date" class="prop-input" value={task.start_date ?? ''} onchange={(e) => void save({ start_date: e.currentTarget.value || null })} />
        </dd>

        <dt>Due</dt>
        <dd class="flex gap-1.5">
          <input type="date" class="prop-input" value={task.due_date ?? ''} onchange={(e) => void save({ due_date: e.currentTarget.value || null })} />
          <input type="time" class="prop-input w-[104px]" value={task.due_time ?? ''} disabled={!task.due_date} onchange={(e) => void save({ due_time: e.currentTarget.value || null })} />
        </dd>

        <dt>Estimate</dt>
        <dd>
          {#key task.estimate_minutes}
            <input class="prop-input" placeholder="e.g. 1h30m" value={task.estimate_minutes ? minutes(task.estimate_minutes) : ''} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} onblur={(e) => commitEstimate(e.currentTarget.value)} />
          {/key}
        </dd>

        <dt>Tags</dt>
        <dd>
          {#key task.tags.join(',')}
            <input class="prop-input" placeholder="Add tags, comma separated" value={task.tags.join(', ')} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} onblur={(e) => commitTags(e.currentTarget.value)} />
          {/key}
        </dd>
      </dl>

      <h3 class="mt-5 mb-1.5 text-xs font-semibold tracking-wide text-subtle uppercase">Notes</h3>
      {#key task.id}
        <textarea
          class="notes w-full rounded-md border border-border bg-surface-2 p-2.5 outline-none focus:border-accent"
          rows="5"
          placeholder="Add notes (Markdown)…"
          value={task.description_md}
          onblur={(e) => e.currentTarget.value !== task?.description_md && void save({ description_md: e.currentTarget.value })}
        ></textarea>
      {/key}

      <h3 class="mt-5 mb-1.5 flex items-center justify-between text-xs font-semibold tracking-wide text-subtle uppercase">
        <span>Subtasks</span>
        {#if subtasks.length}<span class="tabular font-normal normal-case">{subtasks.filter((s) => s.completed_at).length}/{subtasks.length}</span>{/if}
      </h3>
      <ul class="flex flex-col">
        {#each subtasks as sub (sub.id)}
          <li class="flex h-8 items-center gap-2 rounded-md px-1 hover:bg-hover">
            <button
              class={sub.completed_at ? 'text-success' : 'text-subtle hover:text-accent'}
              aria-label={sub.completed_at ? 'Mark not done' : 'Mark done'}
              onclick={() => void act(() => ipc.setTaskDone(sub.id, !sub.completed_at))}
            >
              {#if sub.completed_at}<CheckCircle2 size={16} />{:else}<Circle size={16} />{/if}
            </button>
            <button class="flex-1 truncate text-left {sub.completed_at ? 'text-subtle line-through' : ''}" onclick={() => (app.openTaskId = sub.id)}>{sub.title}</button>
          </li>
        {/each}
      </ul>
      <div class="mt-1 flex h-8 items-center gap-2 px-1 text-muted">
        <Plus size={16} />
        <input
          class="flex-1 bg-transparent outline-none placeholder:text-subtle"
          placeholder="Add subtask"
          bind:value={newSubtask}
          onkeydown={(e) => e.key === 'Enter' && void addSubtask()}
        />
      </div>
    </div>

    <footer class="shrink-0 border-t border-border px-4 py-2 text-[11.5px] text-subtle">
      Created {fmt.format(task.created_at)} · Updated {fmt.format(task.updated_at)}
    </footer>
  </aside>
{/if}

{#if menu}
  <Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .panel {
    animation: slide 160ms var(--ease-out);
  }
  @keyframes slide {
    from {
      opacity: 0;
      transform: translateX(12px);
    }
  }
  .icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--text-muted);
  }
  .icon:hover {
    background: var(--hover);
  }
  .props {
    display: grid;
    grid-template-columns: 84px 1fr;
    row-gap: 4px;
    align-items: center;
  }
  .props dt {
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .prop-btn,
  .prop-input {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    width: 100%;
    min-height: 30px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    text-align: left;
    outline: none;
  }
  .prop-btn:hover,
  .prop-input:hover {
    background: var(--hover);
  }
  .prop-input:focus {
    border-color: var(--accent);
    background: var(--surface-2);
  }
  .prop-input:disabled {
    opacity: 0.4;
  }
  .notes {
    user-select: text;
    resize: vertical;
    min-height: 90px;
  }
</style>
