<script lang="ts">
  import { Briefcase, Building2, CalendarRange, Home, KanbanSquare, Keyboard, Sparkles, Upload } from '@lucide/svelte'
  import type { Setup } from '../lib/bindings/Setup'
  import { importFrom } from '../lib/data'
  import { errorMessage, ipc } from '../lib/ipc'
  import { act, setScope } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'

  let { ondismiss }: { ondismiss: () => void } = $props()
  let busy = $state(false)

  const SETUPS: { setup: Setup; title: string; sections: string; icon: typeof Briefcase }[] = [
    { setup: 'freelancer', title: 'Freelancer', sections: 'Clients · Personal', icon: Briefcase },
    { setup: 'company', title: 'Company', sections: 'Departments · Clients · Internal', icon: Building2 },
    { setup: 'personal_life', title: 'Personal life', sections: 'Home · Family · Health · Personal', icon: Home },
  ]

  /** Adds the setup's sections; rename, add or remove them any time. */
  async function choose(setup: Setup, label: string) {
    busy = true
    const sections = await act(() => ipc.applySetup(setup), label)
    busy = false
    if (!sections) return
    if (sections[0]) setScope({ kind: 'section', id: sections[0].id })
    ondismiss()
  }

  async function sample() {
    busy = true
    try {
      await ipc.createSample()
      toast('Sample workspace added. Remove it any time in Settings → Data.', {
        action: { label: 'Undo', run: () => void ipc.undo() },
      })
      ondismiss()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      busy = false
    }
  }
</script>

<section class="mx-auto mt-4 w-full max-w-[680px] rounded-xl border border-border bg-surface-2 p-6" aria-labelledby="welcome-title">
  <h2 id="welcome-title" class="font-display text-xl font-semibold">Welcome to Taskboard</h2>
  <p class="mt-1 text-muted">
    Plan work in your own sections and projects, track it on a board, and schedule it on your calendar. Everything stays on
    this computer, with no account needed.
  </p>
  <ul class="mt-4 grid gap-3 text-[13px] sm:grid-cols-3">
    <li class="tip"><Keyboard size={16} class="text-accent" /> Press <kbd>N</kbd> and type <em>Send invoice fri 3pm #admin</em></li>
    <li class="tip"><KanbanSquare size={16} class="text-accent" /> Drag cards on the Board, or press <kbd>Space</kbd> to move them</li>
    <li class="tip"><CalendarRange size={16} class="text-accent" /> Find a meeting slot in someone else’s time zone</li>
  </ul>
  <h3 class="mt-5 text-[13px] font-semibold">How do you organize your work?</h3>
  <p class="text-xs text-muted">Pick a starting point for the sidebar. You can rename, add or remove sections any time.</p>
  <div class="mt-2 grid gap-2 sm:grid-cols-3">
    {#each SETUPS as s (s.setup)}
      <button class="setup" disabled={busy} onclick={() => void choose(s.setup, `Set up for ${s.title.toLowerCase()}`)}>
        <s.icon size={16} class="text-accent" />
        <span class="font-medium">{s.title}</span>
        <span class="text-xs text-muted">{s.sections}</span>
      </button>
    {/each}
  </div>
  <div class="mt-4 flex flex-wrap gap-2">
    <button class="btn" disabled={busy} onclick={() => void choose('blank', 'Started empty')}>Start empty</button>
    <button class="btn" disabled={busy} onclick={() => void sample()}><Sparkles size={14} /> Explore with sample data</button>
    <button class="btn" onclick={() => void importFrom('csv').then(ondismiss)}><Upload size={14} /> Import from Todoist / Trello…</button>
  </div>
</section>

<style>
  .tip {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--surface-3);
  }
  .setup {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
  }
  .setup:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .setup:disabled {
    opacity: 0.6;
  }
  kbd {
    padding: 0 4px;
    border-radius: 4px;
    border: 1px solid var(--border-strong);
    font-size: 11px;
  }
</style>
