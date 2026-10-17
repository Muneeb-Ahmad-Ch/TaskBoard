<script lang="ts">
  import { AlertCircle, X } from '@lucide/svelte'
  import { dismiss, toasts } from '../lib/toast.svelte'
</script>

<!-- Bottom-centre for short confirmations; the bottom-right corner is for reminders. -->
<div class="pointer-events-none fixed bottom-10 left-1/2 z-50 flex w-max max-w-[calc(100vw-32px)] -translate-x-1/2 flex-col items-center gap-2" aria-live="polite" role="status">
  {#each toasts as t (t.id)}
    <div class="toast pointer-events-auto flex max-w-[420px] items-center gap-3 rounded-lg border border-border bg-surface-2 py-2 pr-2 pl-3 shadow-lg">
      {#if t.kind === 'error'}<AlertCircle size={16} class="shrink-0 text-danger" />{/if}
      <span class="flex-1">{t.message}</span>
      {#if t.action}
        <button
          class="shrink-0 rounded-md px-2 py-1 font-medium text-accent hover:bg-hover"
          onclick={() => {
            t.action?.run()
            dismiss(t.id)
          }}>{t.action.label}</button
        >
      {/if}
      <button class="grid size-6 shrink-0 place-items-center rounded text-subtle hover:bg-hover" aria-label="Dismiss" onclick={() => dismiss(t.id)}><X size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toast {
    animation: in 160ms var(--ease-out);
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
</style>
