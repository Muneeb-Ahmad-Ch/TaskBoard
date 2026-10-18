<script lang="ts">
  // A date or time shown the way the rest of the app writes it ("Fri, 23 Oct",
  // "15:00"), with the system picker behind it and a clear button. Replaces a
  // bare <input type="date"> whose "dd/mm/yyyy" mask looked unfinished.
  import { X } from '@lucide/svelte'

  let {
    kind = 'date',
    value,
    label,
    placeholder,
    disabled = false,
    onchange,
  }: {
    kind?: 'date' | 'time'
    value: string | null
    /** What it is, for screen readers ("Due date"). */
    label: string
    /** Shown when empty ("Add due date"). */
    placeholder: string
    disabled?: boolean
    onchange: (value: string | null) => void
  } = $props()

  let input: HTMLInputElement | undefined = $state()

  const shown = $derived.by(() => {
    if (!value) return ''
    if (kind === 'time') return value
    const [y, m, d] = value.split('-').map(Number)
    const date = new Date(y, (m ?? 1) - 1, d ?? 1)
    const sameYear = date.getFullYear() === new Date().getFullYear()
    return new Intl.DateTimeFormat(undefined, { weekday: 'short', day: 'numeric', month: 'short', year: sameYear ? undefined : 'numeric' }).format(date)
  })

  function open() {
    if (disabled || !input) return
    try {
      input.showPicker()
    } catch {
      input.focus()
    }
  }
</script>

<span class="df" class:disabled>
  <button type="button" class="face" {disabled} aria-label="{label}: {shown || 'not set'}" onclick={open}>
    {#if shown}<span class="tabular">{shown}</span>{:else}<span class="text-subtle">{placeholder}</span>{/if}
  </button>
  {#if value && !disabled}
    <button type="button" class="clear" aria-label="Clear {label.toLowerCase()}" title="Clear" onclick={() => onchange(null)}><X size={12} /></button>
  {/if}
  <!-- The real input: hidden, opened by showPicker(); keeps native validation and locale. -->
  <input bind:this={input} type={kind} class="native" tabindex="-1" aria-hidden="true" value={value ?? ''} onchange={(e) => onchange(e.currentTarget.value || null)} />
</span>

<style>
  .df {
    position: relative;
    display: inline-flex;
    align-items: center;
    min-width: 0;
    border-radius: 6px;
  }
  .df:hover:not(.disabled) {
    background: var(--hover);
  }
  .face {
    display: flex;
    align-items: center;
    min-height: 30px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    text-align: left;
    white-space: nowrap;
  }
  .face:disabled {
    cursor: default;
    opacity: 0.55;
  }
  .clear {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-right: 2px;
    border-radius: 5px;
    color: var(--text-subtle);
    opacity: 0;
  }
  .df:hover .clear,
  .clear:focus-visible {
    opacity: 1;
  }
  .clear:hover {
    background: var(--hover);
    color: var(--text);
  }
  .native {
    position: absolute;
    left: 0;
    bottom: 0;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }
</style>
