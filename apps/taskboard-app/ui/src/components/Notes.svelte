<script lang="ts">
  // Markdown notes: rendered (sanitized in core) until clicked, then a textarea.
  // Links open in the browser; the webview itself never navigates.
  import { inTauri, ipc, openLink } from '../lib/ipc'

  let {
    value,
    onsave,
    placeholder = 'Add notes (Markdown)…',
    rows = 5,
    label = 'Notes',
  }: {
    value: string
    onsave: (value: string) => void
    placeholder?: string
    rows?: number
    label?: string
  } = $props()

  let editing = $state(false)
  let html = $state('')
  let area = $state<HTMLTextAreaElement | null>(null)

  $effect(() => {
    const source = value
    if (!source.trim() || !inTauri) {
      html = ''
      return
    }
    let stale = false
    ipc
      .renderMarkdown(source)
      .then((out) => {
        if (!stale) html = out
      })
      .catch(() => {
        if (!stale) html = ''
      })
    return () => {
      stale = true
    }
  })

  $effect(() => {
    if (editing) area?.focus()
  })

  function finish(next: string) {
    editing = false
    if (next !== value) onsave(next)
  }

  function onPreviewClick(e: MouseEvent) {
    const link = (e.target as Element).closest('a')
    if (link) {
      e.preventDefault()
      void openLink(link.getAttribute('href') ?? '')
      return
    }
    editing = true
  }
</script>

{#if editing || !value.trim() || !html}
  <textarea
    bind:this={area}
    class="notes w-full rounded-md border border-border bg-surface-2 p-2.5 outline-none focus:border-accent"
    {rows}
    {placeholder}
    aria-label={label}
    {value}
    onfocus={() => (editing = true)}
    onkeydown={(e) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        finish(e.currentTarget.value)
      }
    }}
    onblur={(e) => finish(e.currentTarget.value)}
  ></textarea>
{:else}
  <!-- Rendered notes: click to edit, Enter from the keyboard. -->
  <div
    class="md w-full cursor-text rounded-md border border-transparent p-2.5 hover:border-border"
    role="button"
    tabindex="0"
    aria-label="{label} (press Enter to edit)"
    onclick={onPreviewClick}
    onkeydown={(e) => {
      if (e.key === 'Enter' && e.target === e.currentTarget) {
        e.preventDefault()
        editing = true
      }
    }}
  >
    {@html html}
  </div>
{/if}

<style>
  .notes {
    user-select: text;
    resize: vertical;
    min-height: 90px;
  }
  .md {
    font-size: 13px;
    line-height: 1.55;
    overflow-wrap: anywhere;
  }
  .md :global(:first-child) {
    margin-top: 0;
  }
  .md :global(:last-child) {
    margin-bottom: 0;
  }
  .md :global(p),
  .md :global(ul),
  .md :global(ol),
  .md :global(pre),
  .md :global(table),
  .md :global(blockquote) {
    margin: 0 0 0.6em;
  }
  .md :global(h1),
  .md :global(h2),
  .md :global(h3),
  .md :global(h4) {
    margin: 0.8em 0 0.35em;
    font-weight: 600;
    line-height: 1.3;
  }
  .md :global(h1) {
    font-size: 1.25em;
  }
  .md :global(h2) {
    font-size: 1.12em;
  }
  .md :global(h3),
  .md :global(h4) {
    font-size: 1em;
  }
  .md :global(ul) {
    padding-left: 1.25em;
    list-style: disc;
  }
  .md :global(ol) {
    padding-left: 1.4em;
    list-style: decimal;
  }
  .md :global(a) {
    color: var(--accent-soft-text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .md :global(code) {
    font-family: 'Cascadia Mono', Consolas, monospace;
    font-size: 0.92em;
    padding: 0.05em 0.3em;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .md :global(pre) {
    padding: 0.6em 0.75em;
    border-radius: 6px;
    background: var(--surface-2);
    overflow-x: auto;
  }
  .md :global(pre code) {
    padding: 0;
    background: none;
  }
  .md :global(blockquote) {
    padding-left: 0.75em;
    border-left: 3px solid var(--border-strong);
    color: var(--text-muted);
  }
  .md :global(table) {
    border-collapse: collapse;
  }
  .md :global(th),
  .md :global(td) {
    padding: 0.25em 0.6em;
    border: 1px solid var(--border);
    text-align: left;
  }
  .md :global(hr) {
    margin: 0.8em 0;
    border: 0;
    border-top: 1px solid var(--border);
  }
  /* Checklist items show their box instead of a bullet; done ones fade. */
  .md :global(li.task) {
    list-style: none;
    margin-left: -1.1em;
  }
  .md :global(li.task.done) {
    color: var(--text-subtle);
    text-decoration: line-through;
    text-decoration-color: var(--border-strong);
  }
</style>
