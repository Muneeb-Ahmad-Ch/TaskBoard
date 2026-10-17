<script lang="ts">
  import type { Snippet } from 'svelte'
  import { X } from '@lucide/svelte'

  let {
    title,
    onclose,
    width = 520,
    height,
    bare = false,
    children,
    footer,
  }: {
    title: string
    onclose: () => void
    width?: number
    /** Fixed height (px), e.g. for multi-page dialogs that shouldn't jump. */
    height?: number
    /** No body padding or scrolling: the content lays out its own panes. */
    bare?: boolean
    children: Snippet
    footer?: Snippet
  } = $props()

  let dialog: HTMLDivElement | undefined = $state()

  $effect(() => {
    // Focus the first field; restore focus to whatever opened us on close.
    const opener = document.activeElement as HTMLElement | null
    queueMicrotask(() => dialog?.querySelector<HTMLElement>('input, select, textarea, button')?.focus())
    return () => opener?.focus?.()
  })

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onclose()
    }
    // Keep Tab inside the dialog.
    if (e.key === 'Tab' && dialog) {
      const items = [...dialog.querySelectorAll<HTMLElement>('input, select, textarea, button, [tabindex="0"]')].filter((el) => !el.hasAttribute('disabled'))
      const first = items[0]
      const last = items[items.length - 1]
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last?.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first?.focus()
      }
    }
  }
</script>

<div class="backdrop fixed inset-0 z-40 grid place-items-center bg-black/30 p-4" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div
    bind:this={dialog}
    class="dialog flex max-h-[90vh] w-full flex-col overflow-hidden rounded-xl border border-border bg-surface-2 shadow-2xl"
    style="max-width: {width}px;{height ? ` height: min(${height}px, 90vh);` : ''}"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    {onkeydown}
  >
    <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
      <h2 class="font-display flex-1 truncate text-[15px] font-semibold">{title}</h2>
      <button class="grid size-7 place-items-center rounded-md text-muted hover:bg-hover" aria-label="Close" onclick={onclose}><X size={16} /></button>
    </header>
    <div class="min-h-0 flex-1 {bare ? 'flex flex-col' : 'overflow-y-auto px-4 py-4'}">
      {@render children()}
    </div>
    {#if footer}
      <footer class="flex shrink-0 items-center gap-2 border-t border-border px-4 py-3">
        {@render footer()}
      </footer>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    animation: fade 120ms var(--ease-out);
  }
  .dialog {
    animation: rise 160ms var(--ease-out);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.99);
    }
  }
  :global(.field-label) {
    display: block;
    margin-bottom: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-muted);
  }
  :global(.field) {
    width: 100%;
    height: 32px;
    padding-inline: 0.6rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface-1);
    outline: none;
  }
  :global(.field:focus) {
    border-color: var(--accent);
  }
  :global(textarea.field) {
    height: auto;
    padding-block: 0.45rem;
    resize: vertical;
  }
  :global(.btn) {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    height: 30px;
    padding-inline: 0.8rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    font-weight: 500;
  }
  :global(.btn:hover) {
    background: var(--hover);
  }
  :global(.btn-primary) {
    border-color: transparent;
    background: var(--accent);
    color: var(--on-accent);
  }
  :global(.btn-primary:hover) {
    background: var(--accent-hover);
  }
  :global(.btn-danger) {
    color: var(--danger);
  }
</style>
