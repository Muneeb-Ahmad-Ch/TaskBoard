<script lang="ts">
  import { CheckCircle2, Circle, ExternalLink, Repeat } from '@lucide/svelte'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import { cal, fmtRange, sameDay } from '../lib/cal.svelte'
  import { isoDate, isOverdue, swatch } from '../lib/format'
  import { app } from '../lib/store.svelte'

  let { days, occ, tasks }: { days: Date[]; occ: Occurrence[]; tasks: TaskRow[] } = $props()

  type Entry = { key: string; sort: number; o?: Occurrence; t?: TaskRow }
  const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' })
  const today = new Date()

  function entries(day: Date): Entry[] {
    const s = day.getTime()
    const e = new Date(day.getFullYear(), day.getMonth(), day.getDate() + 1).getTime()
    const iso = isoDate(day)
    const list: Entry[] = occ
      .filter((o) => o.status !== 'cancelled' && (o.kind === 'meeting' ? cal.layers.meetings : cal.layers.blocks))
      .filter((o) => o.start_at < e && o.end_at > s)
      .map((o) => ({ key: `e-${o.event_id}-${o.original_start}`, sort: o.all_day ? s - 1 : o.start_at, o }))
    if (cal.layers.tasks)
      for (const t of tasks)
        if (t.due_date === iso) {
          const [h, m] = (t.due_time ?? '23:59').split(':').map(Number)
          list.push({ key: `t-${t.id}`, sort: s + (h * 60 + m) * 60_000 + 1, t })
        }
    return list.sort((a, b) => a.sort - b.sort)
  }

  const grouped = $derived(days.map((d) => ({ day: d, list: entries(d) })).filter((g) => g.list.length))
</script>

<div class="min-h-0 flex-1 overflow-y-auto rounded-[var(--radius)] border border-border bg-surface-2 px-4 py-2">
  {#each grouped as { day, list } (isoDate(day))}
    <section class="py-2">
      <h3 class="mb-1 text-[13px] font-semibold {sameDay(day, today) ? 'text-accent' : ''}">{sameDay(day, today) ? 'Today · ' : ''}{dayFmt.format(day)}</h3>
      <ul class="flex flex-col">
        {#each list as entry (entry.key)}
          <li>
            {#if entry.o}
              {@const o = entry.o}
              <div class="row">
                <span class="tabular w-36 shrink-0 text-xs text-muted">{o.all_day ? 'All day' : fmtRange(o.start_at, o.end_at)}</span>
                <span class="h-4 w-1 shrink-0 rounded" style="background: {swatch(o.color, 0.6, 0.14)}"></span>
                <button class="flex min-w-0 flex-1 items-center gap-1.5 text-left" onclick={() => (cal.draft = { id: o.event_id, originalStart: o.recurring ? o.original_start : undefined, kind: o.kind, start: o.start_at, end: o.end_at, allDay: o.all_day, title: o.title })}>
                  <span class="truncate font-medium">{o.title}</span>
                  {#if o.recurring}<Repeat size={11} class="shrink-0 text-subtle" />{/if}
                  {#if o.kind === 'block'}<span class="tag">block</span>{/if}
                  {#if o.client_name}<span class="truncate text-xs text-muted">{o.client_name}{o.project_name ? ` / ${o.project_name}` : ''}</span>{/if}
                </button>
                {#if o.meeting_url}
                  <a class="join" href={o.meeting_url} target="_blank" rel="noreferrer"><ExternalLink size={12} /> Join</a>
                {/if}
              </div>
            {:else if entry.t}
              {@const t = entry.t}
              <div class="row">
                <span class="tabular w-36 shrink-0 text-xs {isOverdue(t.due_date, !!t.completed_at) ? 'text-danger' : 'text-muted'}">{t.due_time ?? 'Task'}</span>
                <span class="h-4 w-1 shrink-0 rounded" style="background: {swatch(t.project_color, 0.6, 0.14)}"></span>
                <button class="flex min-w-0 flex-1 items-center gap-1.5 text-left" onclick={() => (app.openTaskId = t.id)}>
                  {#if t.completed_at}<CheckCircle2 size={13} class="shrink-0 text-success" />{:else}<Circle size={13} class="shrink-0 text-subtle" />{/if}
                  <span class="truncate" class:line-through={!!t.completed_at}>{t.title}</span>
                  <span class="truncate text-xs text-muted">{t.client_name ? `${t.client_name} / ` : ''}{t.project_name}</span>
                </button>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {:else}
    <p class="py-10 text-center text-muted">Nothing scheduled in the next three weeks.</p>
  {/each}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-height: 32px;
    padding-inline: 0.4rem;
    border-radius: 6px;
  }
  .row:hover {
    background: var(--hover);
  }
  .tag {
    border-radius: 4px;
    padding: 0 5px;
    font-size: 10.5px;
    color: var(--text-muted);
    background: var(--surface-3);
  }
  .join {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--accent-soft-text);
    background: var(--accent-soft);
  }
</style>
