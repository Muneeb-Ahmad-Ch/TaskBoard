<script lang="ts">
  // A reminder is a task with a date and a time (it notifies at that time),
  // optionally repeating. This is the plain way to make one.
  import { isoDate, todayIso } from '../lib/format'
  import { ipc } from '../lib/ipc'
  import { fromIso, repeatOptions } from '../lib/cal.svelte'
  import { act, app, inboxId, scopeProjectId } from '../lib/store.svelte'
  import DateField from './DateField.svelte'
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  /** The next whole hour (tomorrow's first hour late at night), as a
   *  sensible default date and time. */
  const next = new Date()
  next.setMinutes(0, 0, 0)
  next.setHours(next.getHours() + 1)

  let title = $state('')
  let date = $state<string | null>(isoDate(next))
  let time = $state<string | null>(`${String(next.getHours()).padStart(2, '0')}:00`)
  /** The rule to store ('' = does not repeat); the same choices as the task panel. */
  let repeat = $state('')
  let projectId = $state(scopeProjectId() ?? inboxId() ?? '')
  let saving = $state(false)

  const options = $derived([
    ...repeatOptions(fromIso(date ?? todayIso())),
    { label: '1 week after it’s done', rule: 'AFTER:1W' },
    { label: '1 month after it’s done', rule: 'AFTER:1M' },
  ])
  // A choice that doesn't exist for the new date (like "last Friday") falls back
  // to the same kind of repeat for that date.
  $effect(() => {
    if (repeat && !options.some((o) => o.rule === repeat)) repeat = repeat.startsWith('FREQ=MONTHLY') ? 'FREQ=MONTHLY' : ''
  })
  const projects = $derived(app.projects.filter((p) => !p.archived_at))

  async function save() {
    const name = title.trim()
    if (!name || !date || saving) return
    saving = true
    const rule = repeat || undefined
    const done = await act(
      () =>
        ipc.createTask({
          title: name,
          project_id: projectId || undefined,
          due_date: date ?? undefined,
          due_time: time ?? undefined,
          recurrence_rule: rule,
        }),
      rule ? 'Repeating reminder added' : 'Reminder added',
    )
    saving = false
    if (done) onclose()
  }
</script>

<Modal title="New reminder" {onclose} width={460}>
  <form class="flex flex-col gap-4" onsubmit={(e) => { e.preventDefault(); void save() }}>
    <label>
      <span class="field-label">Remind me to</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" placeholder="e.g. Pay rent, Call the dentist" bind:value={title} autofocus />
    </label>
    <div class="grid grid-cols-2 gap-3">
      <div>
        <span class="field-label">On</span>
        <DateField value={date} label="Date" placeholder="Pick a date" onchange={(v) => (date = v ?? todayIso())} />
      </div>
      <div>
        <span class="field-label">At</span>
        <DateField kind="time" value={time} label="Time" placeholder="No time (no notification)" onchange={(v) => (time = v)} />
      </div>
    </div>
    <label>
      <span class="field-label">Repeat</span>
      <select class="field" bind:value={repeat}>
        {#each options as o (o.label)}<option value={o.rule ?? ''}>{o.label}</option>{/each}
      </select>
    </label>
    <label>
      <span class="field-label">In</span>
      <select class="field" bind:value={projectId}>
        {#each projects as p (p.id)}<option value={p.id}>{p.client_name ? `${p.client_name} / ${p.name}` : p.name}</option>{/each}
      </select>
    </label>
    {#if !time}
      <p class="text-xs text-muted">Without a time it shows on its day but sends no notification.</p>
    {/if}
    <!-- Makes Enter submit; not a Tab stop (it has no visible focus). -->
    <button type="submit" class="sr-only" tabindex="-1" aria-hidden="true">Add</button>
  </form>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={!title.trim() || !date || saving} onclick={() => void save()}>Add reminder</button>
  {/snippet}
</Modal>
