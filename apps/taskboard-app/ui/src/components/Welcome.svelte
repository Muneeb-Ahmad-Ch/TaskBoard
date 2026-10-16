<script lang="ts">
  import { CalendarRange, KanbanSquare, Keyboard, Sparkles, Upload } from '@lucide/svelte'
  import { importFrom } from '../lib/data'
  import { errorMessage, ipc } from '../lib/ipc'
  import { toast } from '../lib/toast.svelte'

  let { ondismiss }: { ondismiss: () => void } = $props()
  let busy = $state(false)

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
    Plan work across clients and projects, track it on a board, and schedule it on your calendar. Everything stays on this
    computer, with no account needed.
  </p>
  <ul class="mt-4 grid gap-3 text-[13px] sm:grid-cols-3">
    <li class="tip"><Keyboard size={16} class="text-accent" /> Press <kbd>N</kbd> and type <em>Send invoice fri 3pm #admin</em></li>
    <li class="tip"><KanbanSquare size={16} class="text-accent" /> Drag cards on the Board, or press <kbd>Space</kbd> to move them</li>
    <li class="tip"><CalendarRange size={16} class="text-accent" /> Find a meeting slot with a client in their time zone</li>
  </ul>
  <div class="mt-5 flex flex-wrap gap-2">
    <button class="btn btn-primary" disabled={busy} onclick={() => void sample()}><Sparkles size={14} /> Explore with sample data</button>
    <button class="btn" onclick={() => void importFrom('csv').then(ondismiss)}><Upload size={14} /> Import from Todoist / Trello…</button>
    <button class="btn" onclick={ondismiss}>Start empty</button>
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
  kbd {
    padding: 0 4px;
    border-radius: 4px;
    border: 1px solid var(--border-strong);
    font-size: 11px;
  }
</style>
