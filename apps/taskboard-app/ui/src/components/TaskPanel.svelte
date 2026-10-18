<script lang="ts">
  import { Archive, ArchiveRestore, ArrowLeft, CheckCircle2, ChevronRight, Circle, Copy, Plus, Timer, Trash2, X } from '@lucide/svelte'
  import type { Priority } from '../lib/bindings/Priority'
  import type { Status } from '../lib/bindings/Status'
  import type { Task } from '../lib/bindings/Task'
  import type { TaskPatch } from '../lib/bindings/TaskPatch'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import { dueLabel, isOverdue, isTyping, minutes, parseMinutes, swatch } from '../lib/format'
  import { errorMessage, ipc } from '../lib/ipc'
  import { act, app } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'
  import Notes from './Notes.svelte'
  import DateField from './DateField.svelte'
  import Progress from './Progress.svelte'
  import Splitter from './Splitter.svelte'
  import TaskBlocks from './TaskBlocks.svelte'
  import { layout, PANEL, setPanelWidth } from '../lib/layout.svelte'
  import { fromIso, repeatOptions } from '../lib/cal.svelte'

  let task = $state<Task | null>(null)
  let statuses = $state<Status[]>([])
  let subtasks = $state<TaskRow[]>([])
  /** The parent, when the open task is a subtask (for the back link). */
  let parent = $state<Task | null>(null)
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)
  let newSubtask = $state('')
  /** Progress as the core computes it (derived from subtasks when present). */
  let computed = $state(0)
  let slider = $state(0)

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
        const [st, subs, self, up] = await Promise.all([
          ipc.listStatuses(t.project_id),
          ipc.listTasks({ parent_id: id, sort: 'manual', limit: 500 }),
          ipc.listTasks({ id, include_archived: true, limit: 1 }),
          t.parent_id ? ipc.getTask(t.parent_id).catch(() => null) : Promise.resolve(null),
        ])
        if (cancelled) return
        task = t
        parent = up
        statuses = st
        subtasks = subs.rows
        computed = self.rows[0]?.progress ?? 0
        slider = computed
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
  const repeatChoices = $derived([
    ...repeatOptions(task?.due_date ? fromIso(task.due_date) : new Date()),
    { label: '1 day after done', rule: 'AFTER:1D' },
    { label: '1 week after done', rule: 'AFTER:1W' },
    { label: '1 month after done', rule: 'AFTER:1M' },
  ])
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

  /** Manual progress; 100% completes the task, dragging below 100 reopens it. */
  async function commitProgress(value: number) {
    if (!task) return
    const id = task.id
    if (value >= 100) {
      if (!task.completed_at) await act(() => ipc.setTaskDone(id, true))
      return
    }
    if (task.completed_at) await act(() => ipc.setTaskDone(id, false))
    await save({ progress: value })
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

  /** Esc and the back link: from a subtask go to its parent, otherwise close. */
  function back() {
    if (task?.parent_id) app.openTaskId = task.parent_id
    else close()
  }

  /** Turns a subtask into an ordinary task in the same project. */
  function detach() {
    if (!task) return
    const title = task.title
    void act(() => ipc.updateTask(task!.id, { parent_id: null }), `“${title}” is now a separate task`)
  }

  const fmt = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' })
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape' && app.openTaskId && !menu && !isTyping(e)) back()
  }}
/>

{#if task}
  <!-- Docked beside the list on wide windows; overlays it on narrow ones. -->
  <aside class="panel relative flex w-[var(--panel-w)] shrink-0 flex-col border-l border-border bg-surface-1 max-[1200px]:absolute max-[1200px]:inset-y-0 max-[1200px]:right-0 max-[1200px]:z-30 max-[1200px]:w-[min(var(--panel-w),100%)] max-[1200px]:shadow-2xl" style="--panel-w: {layout.panelWidth}px" aria-label="Task details">
    <Splitter value={layout.panelWidth} min={PANEL.min} max={PANEL.max} initial={PANEL.default} edge="left" label="Resize the task panel" onchange={setPanelWidth} />
    <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
      <button
        class="flex h-7 items-center gap-1.5 rounded-md border px-2 text-xs font-medium {done ? 'border-success text-success' : 'border-border hover:border-border-strong'}"
        onclick={() => task && void act(() => ipc.setTaskDone(task!.id, !done))}
      >
        {#if done}<CheckCircle2 size={14} /> Done{:else}<Circle size={14} /> Mark done{/if}
      </button>
      <div class="flex-1"></div>
      <button
        class="icon"
        title="Start a timer for this task (stops any running timer)"
        aria-label="Start timer"
        onclick={() => task && void act(() => ipc.logTime({ task_id: task!.id, started_at: Date.now() }), 'Timer started — see Calendar')}
      ><Timer size={15} /></button>
      <button class="icon" title="Duplicate" aria-label="Duplicate task" onclick={() => task && void act(() => ipc.duplicateTask(task!.id), 'Task duplicated')}><Copy size={15} /></button>
      {#if task.archived_at}
        <button class="icon" title="Restore from the archive" aria-label="Restore task" onclick={() => task && void act(() => ipc.setTaskArchived(task!.id, false), 'Task restored')}><ArchiveRestore size={15} /></button>
      {:else}
        <button class="icon" title="Archive (hide it, keep it)" aria-label="Archive task" onclick={() => task && void act(() => ipc.setTaskArchived(task!.id, true), 'Task archived').then(close)}><Archive size={15} /></button>
      {/if}
      <button class="icon hover:text-danger" title="Delete" aria-label="Delete task" onclick={() => task && void act(() => ipc.deleteTask(task!.id), 'Task deleted').then(close)}><Trash2 size={15} /></button>
      <button class="icon" title="Close (Esc)" aria-label="Close details" onclick={close}><X size={16} /></button>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4">
      {#if task.archived_at}
        <p class="mb-3 flex items-center gap-2 rounded-md bg-surface-2 px-3 py-2 text-xs text-muted" role="status">
          <Archive size={13} class="shrink-0" /> Archived. It stays out of your lists until you restore it.
        </p>
      {/if}
      {#if task.parent_id}
        <nav class="mb-2 flex items-center gap-2 text-xs" aria-label="Parent task">
          <button class="back flex min-w-0 items-center gap-1 rounded-md py-1 pr-2 pl-1 text-muted hover:bg-hover hover:text-text" onclick={back} title="Back to the parent task (Esc)">
            <ArrowLeft size={14} class="shrink-0" />
            <span class="truncate">{parent?.title ?? 'Parent task'}</span>
          </button>
          <span class="shrink-0 rounded-full bg-surface-2 px-2 py-0.5 text-subtle">Subtask</span>
        </nav>
      {/if}
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
          {#if task.parent_id}
            <span class="flex min-h-[30px] items-center gap-2 px-2" title="Subtasks stay in their parent's project">
              {#if project}
                <span class="size-2 shrink-0 rounded-full" style="background: {swatch(project.color)}"></span>
                <span class="truncate">{#if project.client_name}<span class="text-muted">{project.client_name}&nbsp;/&nbsp;</span>{/if}{project.name}</span>
              {/if}
              <button class="ml-auto shrink-0 text-xs text-muted underline-offset-2 hover:text-text hover:underline" onclick={detach}>Make separate task</button>
            </span>
          {:else}
          <button class="prop-btn" onclick={(e) => openMenu(e, app.projects.map((p) => ({
            label: p.client_name ? `${p.client_name} / ${p.name}` : p.name,
            color: p.color,
            checked: p.id === task?.project_id,
            run: () => void save({ project_id: p.id }),
          })))}>
            {#if project}
              <span class="size-2 shrink-0 rounded-full" style="background: {swatch(project.color)}"></span>
              <span class="min-w-0 truncate" title="{project.client_name ? `${project.client_name} / ` : ''}{project.name}">{#if project.client_name}<span class="text-muted">{project.client_name}&nbsp;/&nbsp;</span>{/if}{project.name}</span>
            {/if}
          </button>
          {/if}
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
          <DateField label="Start date" placeholder="Add start date" value={task.start_date} onchange={(v) => void save({ start_date: v })} />
        </dd>

        <dt>Due</dt>
        <dd class="flex gap-1.5">
          <DateField label="Due date" placeholder="Add due date" value={task.due_date} onchange={(v) => void save(v ? { due_date: v } : { due_date: null, due_time: null })} />
          {#if task.due_date}
            <DateField kind="time" label="Due time" placeholder="Add time" value={task.due_time} onchange={(v) => void save({ due_time: v })} />
          {/if}
        </dd>

        {#if !task.parent_id || task.recurrence_rule}
        <dt>Repeat</dt>
        <dd>
          <select
            class="prop-input"
            value={task.recurrence_rule ?? ''}
            aria-label="Repeat"
            onchange={(e) => void save({ recurrence_rule: e.currentTarget.value || null })}
          >
            {#each repeatChoices as o (o.label)}<option value={o.rule ?? ''}>{o.label}</option>{/each}
            {#if task.recurrence_rule && !repeatChoices.some((o) => o.rule === task?.recurrence_rule)}
              <option value={task.recurrence_rule}>Custom: {task.recurrence_rule}</option>
            {/if}
          </select>
        </dd>
        {/if}

        <dt>Estimate</dt>
        <dd>
          {#key task.estimate_minutes}
            <input class="prop-input" placeholder="e.g. 1h30m" value={task.estimate_minutes ? minutes(task.estimate_minutes) : ''} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} onblur={(e) => commitEstimate(e.currentTarget.value)} />
          {/key}
        </dd>

        <dt>Progress</dt>
        <dd>
          {#if subtasks.length}
            <span class="flex min-h-[30px] items-center gap-2 px-2" title="Calculated from subtasks">
              <Progress value={computed} width={120} label />
              <span class="text-xs text-subtle">from {subtasks.filter((s) => s.completed_at).length}/{subtasks.length} subtasks</span>
            </span>
          {:else}
            <span class="flex min-h-[30px] items-center gap-2 px-2">
              <input
                type="range"
                class="slider flex-1"
                min="0"
                max="100"
                step="5"
                aria-label="Progress"
                bind:value={slider}
                onchange={() => void commitProgress(slider)}
              />
              <span class="tabular w-9 text-right text-xs text-muted">{slider}%</span>
            </span>
          {/if}
        </dd>

        <dt>Tags</dt>
        <dd>
          {#key task.tags.join(',')}
            <input class="prop-input" placeholder="Add tags, comma separated" value={task.tags.join(', ')} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} onblur={(e) => commitTags(e.currentTarget.value)} />
          {/key}
        </dd>
      </dl>


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
            <button
              class="group flex min-w-0 flex-1 items-center gap-2 text-left"
              aria-label="Open subtask {sub.title}"
              onclick={() => (app.openTaskId = sub.id)}
            >
              <span class="flex-1 truncate {sub.completed_at ? 'text-subtle line-through' : ''}">{sub.title}</span>
              {#if sub.subtasks_total}<span class="tabular shrink-0 text-xs text-subtle">{sub.subtasks_done}/{sub.subtasks_total}</span>{/if}
              {#if sub.due_date}<span class="tabular shrink-0 text-xs {isOverdue(sub.due_date, !!sub.completed_at) ? 'text-danger' : 'text-subtle'}">{dueLabel(sub.due_date)}</span>{/if}
              <ChevronRight size={14} class="shrink-0 text-subtle opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100" />
            </button>
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

      <TaskBlocks {task} />

      <h3 class="mt-5 mb-1.5 text-xs font-semibold tracking-wide text-subtle uppercase">Notes</h3>
      {#key task.id}
        <Notes value={task.description_md} onsave={(description_md) => void save({ description_md })} />
      {/key}
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
  .slider {
    accent-color: var(--accent);
    height: 18px;
  }
</style>
