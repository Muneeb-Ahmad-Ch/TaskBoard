<script lang="ts">
  import { AlarmClock, Bell, CalendarClock, ExternalLink, Sun, X } from '@lucide/svelte'
  import type { Notification } from '../lib/bindings/Notification'
  import { errorMessage, ipc, onReminders, openLink } from '../lib/ipc'
  import { app, setScope } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'

  // In-app copies of reminders (the Windows toast can't carry a Join button).
  let items = $state<Notification[]>([])

  $effect(() => {
    let unlisten: (() => void) | undefined
    void onReminders((list) => {
      items = [...list, ...items].slice(0, 4)
      for (const n of list) setTimeout(() => dismiss(n.key), 3 * 60_000)
    }).then((u) => (unlisten = u))
    return () => unlisten?.()
  })

  function dismiss(key: string) {
    items = items.filter((n) => n.key !== key)
  }

  // ---------- snooze ----------
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)
  const timeFmt = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' })

  async function snooze(n: Notification, until: number) {
    try {
      await ipc.snoozeReminder(n.key, until)
      dismiss(n.key)
      toast(`Snoozed until ${timeFmt.format(until)}${new Date(until).getDate() !== new Date().getDate() ? ' tomorrow' : ''}`)
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  function snoozeMenu(e: MouseEvent, n: Notification) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const now = Date.now()
    const tomorrow = new Date()
    tomorrow.setDate(tomorrow.getDate() + 1)
    tomorrow.setHours(9, 0, 0, 0)
    menu = {
      x: r.left,
      y: r.top - 4 - 4 * 30,
      items: [
        { label: '10 minutes', run: () => void snooze(n, now + 10 * 60_000) },
        { label: '30 minutes', run: () => void snooze(n, now + 30 * 60_000) },
        { label: '1 hour', run: () => void snooze(n, now + 60 * 60_000) },
        { label: 'Tomorrow at 9:00', run: () => void snooze(n, tomorrow.getTime()) },
      ],
    }
  }
</script>

{#if items.length}
  <!-- Bottom-right, like Windows notifications: never over the page header or content top. -->
  <div class="fixed right-4 bottom-10 z-50 flex w-[min(340px,calc(100vw-32px))] flex-col-reverse gap-2" role="region" aria-label="Reminders" aria-live="assertive">
    {#each items as n (n.key)}
      <div class="card flex gap-3 rounded-xl border border-border bg-surface-2 p-3 shadow-xl">
        <span class="mt-0.5 shrink-0 text-accent">
          {#if n.kind === 'meeting'}<CalendarClock size={18} />{:else if n.kind === 'digest'}<Sun size={18} />{:else}<Bell size={18} />{/if}
        </span>
        <div class="min-w-0 flex-1">
          <p class="font-medium leading-snug">{n.title}</p>
          <p class="mt-0.5 text-xs text-muted">{n.body}</p>
          <div class="mt-2 flex gap-1.5">
            {#if n.url}
              <button class="act primary" onclick={() => { void openLink(n.url ?? ''); dismiss(n.key) }}><ExternalLink size={13} /> Join</button>
            {/if}
            {#if n.task_id}
              <button class="act" onclick={() => { app.view = 'table'; app.openTaskId = n.task_id; dismiss(n.key) }}>Open task</button>
            {/if}
            {#if n.kind === 'digest'}
              <button class="act" onclick={() => { setScope({ kind: 'today' }); app.view = 'table'; dismiss(n.key) }}>Show today</button>
            {:else}
              <button class="act" aria-haspopup="menu" title="Remind me again later" onclick={(e) => snoozeMenu(e, n)}><AlarmClock size={13} /> Snooze</button>
            {/if}
          </div>
        </div>
        <button class="grid size-6 shrink-0 place-items-center rounded text-subtle hover:bg-hover" aria-label="Dismiss" onclick={() => dismiss(n.key)}><X size={14} /></button>
      </div>
    {/each}
  </div>
{/if}

{#if menu}
  <Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .card {
    animation: in 200ms var(--ease-out);
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
  }
  .act {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    height: 26px;
    padding-inline: 0.6rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    font-size: 12.5px;
    font-weight: 500;
  }
  .act:hover {
    background: var(--hover);
  }
  .act.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--on-accent);
  }
</style>
