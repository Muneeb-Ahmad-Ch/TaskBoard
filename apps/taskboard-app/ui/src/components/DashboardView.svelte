<script lang="ts">
  import { untrack } from 'svelte'
  import { AlertTriangle, CalendarClock, CheckCircle2, Clock, Flame, Sparkles } from '@lucide/svelte'
  import type { Dashboard } from '../lib/bindings/Dashboard'
  import { minutes, swatch } from '../lib/format'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { app, setScope } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Progress from './Progress.svelte'

  let d = $state<Dashboard | null>(null)

  async function load() {
    if (!inTauri) return
    try {
      d = await ipc.dashboard()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }
  $effect(() => {
    void app.dataVersion
    untrack(() => void load())
  })

  const maxDay = $derived(Math.max(1, ...(d?.completions.map((c) => c.count) ?? [1])))
  const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'narrow' })
  const longFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', month: 'short', day: 'numeric' })
  const trend = $derived(d ? d.completed_this_week - d.completed_last_week : 0)

  /** SVG polyline points for a burndown series. */
  function spark(values: number[], w = 120, h = 28): string {
    const max = Math.max(1, ...values)
    return values.map((v, i) => `${(i / Math.max(1, values.length - 1)) * w},${h - (v / max) * h}`).join(' ')
  }

  function go(kind: 'today' | 'upcoming') {
    setScope({ kind })
    app.view = 'table'
  }
</script>

<div class="min-h-0 flex-1 overflow-y-auto pb-4">
  {#if d}
    <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <div class="stat">
        <span class="label"><CheckCircle2 size={14} /> Done this week</span>
        <span class="value">{d.completed_this_week}</span>
        <span class="sub {trend >= 0 ? 'text-success' : 'text-muted'}">{trend >= 0 ? '+' : ''}{trend} vs last week ({d.completed_last_week})</span>
      </div>
      <button class="stat text-left" onclick={() => go('today')}>
        <span class="label"><AlertTriangle size={14} /> Due today</span>
        <span class="value">{d.due_today}</span>
        <span class="sub"><span class={d.overdue ? 'font-medium text-danger' : ''}>{d.overdue} overdue</span> · {d.due_next_7_days} more in the next 7 days</span>
      </button>
      <div class="stat">
        <span class="label"><CalendarClock size={14} /> Meetings today</span>
        <span class="value">{d.meetings_today}</span>
        <span class="sub"><Clock size={11} class="inline" /> {minutes(d.logged_min_this_week) || '0m'} logged this week</span>
      </div>
      <div class="stat">
        <span class="label"><Flame size={14} /> Streak</span>
        <span class="value">{d.streak_days}<span class="ml-1 text-[15px] font-normal text-muted">day{d.streak_days === 1 ? '' : 's'}</span></span>
        <span class="sub">Best: {d.best_streak_days} day{d.best_streak_days === 1 ? '' : 's'}</span>
      </div>
    </div>

    <div class="mt-3 grid gap-3 lg:grid-cols-[1.1fr_1fr]">
      <section class="panel">
        <h3 class="title">Completed per day <span class="font-normal text-subtle">· last 14 days</span></h3>
        {#if !d.completions.some((c) => c.count)}
          <div class="grid h-32 place-items-center text-center text-[13px] text-muted">
            <p>Nothing completed in the last two weeks yet.<br /><span class="text-xs text-subtle">Finished tasks show up here as a daily bar.</span></p>
          </div>
        {:else}
        <div class="flex h-32 items-end gap-1.5" role="img" aria-label="Completed tasks per day for the last 14 days">
          {#each d.completions as c (c.date)}
            <div class="flex flex-1 flex-col items-center gap-1" title="{longFmt.format(new Date(`${c.date}T00:00`))}: {c.count}">
              <span class="tabular text-[10.5px] text-subtle">{c.count || ''}</span>
              <div class="w-full rounded-t bg-accent" style="height: {(c.count / maxDay) * 88}px; min-height: {c.count ? 3 : 1}px; opacity: {c.count ? 1 : 0.25}"></div>
              <span class="text-[10.5px] text-subtle">{dayFmt.format(new Date(`${c.date}T00:00`))}</span>
            </div>
          {/each}
        </div>
        {/if}
      </section>

      <section class="panel">
        <h3 class="title">Groups</h3>
        <table class="w-full text-[13px]">
          <thead>
            <tr class="text-left text-xs text-muted">
              <th class="pb-1 font-medium">Group</th>
              <th class="pb-1 text-right font-medium">Open</th>
              <th class="pb-1 text-right font-medium">Overdue</th>
              <th class="pb-1 text-right font-medium">Done (wk)</th>
              <th class="pb-1 text-right font-medium">Hours (wk)</th>
            </tr>
          </thead>
          <tbody>
            {#each d.clients as c (c.client_id ?? `s:${c.section_id ?? ''}`)}
              <tr class="row" onclick={() => (c.client_id ? setScope({ kind: 'client', id: c.client_id }) : c.section_id ? setScope({ kind: 'section', id: c.section_id }) : setScope({ kind: 'all' }))}>
                <td class="py-1" title={c.section_name && c.client_id ? `${c.section_name} › ${c.name}` : c.name}><span class="flex items-center gap-1.5"><span class="size-2 rounded-full" style="background: {swatch(c.color)}"></span>{c.name}</span></td>
                <td class="tabular text-right">{c.open}</td>
                <td class="tabular text-right {c.overdue ? 'font-medium text-danger' : 'text-subtle'}">{c.overdue}</td>
                <td class="tabular text-right">{c.done_this_week}</td>
                <td class="tabular text-right text-muted">{minutes(c.logged_min_this_week) || '—'}</td>
              </tr>
            {:else}
              <tr><td colspan="5" class="py-2 text-subtle">Add a group or a project to see a summary here.</td></tr>
            {/each}
          </tbody>
        </table>
      </section>
    </div>

    <section class="panel mt-3">
      <h3 class="title">Projects <span class="font-normal text-subtle">· progress and open tasks over 14 days</span></h3>
      <div class="grid gap-x-6 gap-y-3 md:grid-cols-2">
        {#each d.projects as p (p.project_id)}
          <button class="proj text-left" onclick={() => setScope({ kind: 'project', id: p.project_id })}>
            <div class="flex items-center gap-2">
              <span class="size-2 shrink-0 rounded-full" style="background: {swatch(p.color)}"></span>
              <span class="min-w-0 flex-1 truncate font-medium">{p.client_name ? `${p.client_name} / ` : ''}{p.name}</span>
              {#if p.deadline}<span class="text-xs text-muted">due {p.deadline}</span>{/if}
            </div>
            <div class="mt-1.5 flex items-center gap-3">
              <Progress value={p.total ? (p.done / p.total) * 100 : 0} width={110} label />
              <span class="text-xs text-subtle">{p.open} open</span>
              <span class="flex-1"></span>
              <!-- A trend needs history: skip it until open tasks have changed over the 14 days. -->
              {#if new Set(p.burndown).size > 1 && p.burndown.filter((v) => v > 0).length > 1}
                <svg width="120" height="28" class="overflow-visible" role="img" aria-label="Open tasks over the last 14 days">
                  <polyline points={spark(p.burndown)} fill="none" stroke={swatch(p.color, 0.6, 0.14)} stroke-width="2" stroke-linejoin="round" />
                </svg>
              {/if}
            </div>
          </button>
        {:else}
          <p class="text-subtle">Projects with tasks will appear here.</p>
        {/each}
      </div>
    </section>

    {#if d.completed_this_week === 0 && d.due_today === 0 && d.overdue === 0}
      <p class="mt-4 flex items-center gap-2 text-muted"><Sparkles size={15} /> A clean slate. Add tasks with N, or plan meetings in the Calendar.</p>
    {/if}
  {:else}
    <p class="py-10 text-center text-muted">Loading…</p>
  {/if}
</div>

<style>
  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface-2);
  }
  button.stat:hover,
  .proj:hover {
    border-color: var(--border-strong);
  }
  .label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 12px;
    color: var(--text-muted);
  }
  .value {
    font-family: var(--font-display);
    font-size: 26px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .sub {
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .panel {
    padding: 12px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface-2);
  }
  .title {
    margin-bottom: 10px;
    font-size: 13px;
    font-weight: 600;
  }
  .row {
    cursor: pointer;
  }
  .row:hover td {
    background: var(--hover);
  }
  .proj {
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid transparent;
  }
</style>
