<script lang="ts" module>
  export interface MenuItem {
    label: string
    run: () => void
    hint?: string
    danger?: boolean
    disabled?: boolean
    checked?: boolean
    /** OKLCH hue string or "gray" for a color dot. */
    color?: string
  }
  export type MenuEntry = MenuItem | 'separator'
</script>

<script lang="ts">
  import { Check } from '@lucide/svelte'
  import { swatch } from '../lib/format'

  let { items, x, y, onclose }: { items: MenuEntry[]; x: number; y: number; onclose: () => void } =
    $props()

  let el: HTMLDivElement | undefined = $state()
  let active = $state(-1)
  let pos = $state({ left: 0, top: 0 })

  const actionable = $derived(
    items.map((item, i) => ({ item, i })).filter(({ item }) => item !== 'separator' && !item.disabled),
  )

  // Keep the menu inside the window.
  $effect(() => {
    if (!el) return
    const r = el.getBoundingClientRect()
    pos = {
      left: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    }
    el.focus()
  })

  function choose(item: MenuEntry) {
    if (item === 'separator' || item.disabled) return
    onclose()
    item.run()
  }

  function onkeydown(e: KeyboardEvent) {
    const n = actionable.length
    if (e.key === 'Escape') {
      e.preventDefault()
      onclose()
    } else if (e.key === 'ArrowDown' && n) {
      e.preventDefault()
      const at = actionable.findIndex((a) => a.i === active)
      active = actionable[(at + 1) % n].i
    } else if (e.key === 'ArrowUp' && n) {
      e.preventDefault()
      const at = actionable.findIndex((a) => a.i === active)
      active = actionable[(at - 1 + n) % n].i
    } else if (e.key === 'Enter' && active >= 0) {
      e.preventDefault()
      choose(items[active])
    }
  }
</script>

<svelte:window onpointerdown={(e) => el && !el.contains(e.target as Node) && onclose()} onblur={onclose} />

<div
  bind:this={el}
  class="menu fixed z-50 min-w-48 rounded-lg border border-border bg-surface-2 p-1 shadow-xl outline-none"
  style="left: {pos.left}px; top: {pos.top}px"
  role="menu"
  tabindex="-1"
  {onkeydown}
>
  {#each items as item, i (i)}
    {#if item === 'separator'}
      <div class="my-1 h-px bg-border" role="separator"></div>
    {:else}
      <button
        role="menuitem"
        class="item"
        class:danger={item.danger}
        class:active={active === i}
        disabled={item.disabled}
        onpointerenter={() => (active = i)}
        onclick={() => choose(item)}
      >
        <span class="grid w-4 place-items-center">
          {#if item.checked}<Check size={14} />{:else if item.color}<span class="size-2.5 rounded-full" style="background: {swatch(item.color)}"></span>{/if}
        </span>
        <span class="flex-1 truncate text-left">{item.label}</span>
        {#if item.hint}<span class="text-xs text-subtle">{item.hint}</span>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    max-height: min(420px, calc(100vh - 8px));
    overflow-y: auto;
    animation: pop 120ms var(--ease-out);
  }
  .item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.5rem;
    height: 30px;
    padding-inline: 0.5rem;
    border-radius: 5px;
  }
  .item.active {
    background: var(--hover);
  }
  .item.danger {
    color: var(--danger);
  }
  .item:disabled {
    opacity: 0.45;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-2px) scale(0.98);
    }
  }
</style>
