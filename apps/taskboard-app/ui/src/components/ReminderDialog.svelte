<script lang="ts">
  // A reminder is a task with a date and a time (it notifies at that time),
  // optionally repeating. This is the plain way to make one.
  import { todayIso } from '../lib/format'
  import { ipc } from '../lib/ipc'
  import { repeatOptions, repeatRule, type RepeatChoice } from '../lib/repeat'
  import { act, app, inboxId, scopeProjectId } from '../lib/store.svelte'
  import DateField from './DateField.svelte'
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  /** The next whole hour, as a sensible default time. */
  function nextHour(): string {
    const h = (new Date().getHours() + 1) % 24
    return `${String(h).padStart(2, '0')}:00`
  }

  let title = $state('')
  let date = $state<string | null>(todayIso())
  let time = $state<string | null>(nextHour())
  let repeat = $state<RepeatChoice>('none')
  let projectId = $state(scopeProjectId() ?? inboxId() ?? '')
  let saving = $state(false)

  const options = $derived(repeatOptions(date ?? todayIso()))
  // A choice that doesn't exist for the new date (last day of month) falls back.
  $effect(() => {
    if (!options.some((o) => o.value === repeat)) repeat = 'monthly'
  })
  const projects = $derived(app.projects.filter((p) => !p.archived_at))

  async function save() {
    const name = title.trim()
    if (!name || !date || saving) return
    saving = true
    const rule = repeatRule(repeat, date)
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
        {#each options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
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
    <button type="submit" class="sr-only">Add</button>
  </form>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={!title.trim() || !date || saving} onclick={() => void save()}>Add reminder</button>
  {/snippet}
</Modal>
