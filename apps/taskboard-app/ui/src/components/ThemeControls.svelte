<script lang="ts">
  import { Monitor, Moon, Palette, Sun } from '@lucide/svelte'
  import { ACCENTS, setAccentHue, setThemePref, theme, type ThemePref } from '../lib/theme.svelte'

  const modes: { pref: ThemePref; label: string; icon: typeof Sun }[] = [
    { pref: 'system', label: 'System theme', icon: Monitor },
    { pref: 'light', label: 'Light theme', icon: Sun },
    { pref: 'dark', label: 'Dark theme', icon: Moon },
  ]

  let accentOpen = $state(false)
  let root: HTMLDivElement | undefined = $state()

  function onWindowPointerDown(event: PointerEvent) {
    if (accentOpen && root && !root.contains(event.target as Node)) accentOpen = false
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} onkeydown={(e) => e.key === 'Escape' && (accentOpen = false)} />

<div class="flex items-center gap-1.5" bind:this={root}>
  <div class="flex rounded-md border border-border bg-surface-2 p-0.5" role="radiogroup" aria-label="Theme">
    {#each modes as mode (mode.pref)}
      <button
        role="radio"
        aria-checked={theme.pref === mode.pref}
        aria-label={mode.label}
        title={mode.label}
        class="grid size-6 place-items-center rounded-[5px] text-muted transition-colors hover:text-text"
        class:selected={theme.pref === mode.pref}
        onclick={() => setThemePref(mode.pref)}
      >
        <mode.icon size={14} />
      </button>
    {/each}
  </div>

  <div class="relative">
    <button
      class="grid size-7 place-items-center rounded-md border border-border bg-surface-2 text-muted hover:text-text"
      aria-label="Accent color"
      aria-expanded={accentOpen}
      onclick={() => (accentOpen = !accentOpen)}
    >
      <Palette size={14} />
    </button>
    {#if accentOpen}
      <div
        class="absolute top-9 right-0 z-10 w-44 rounded-lg border border-border bg-surface-2 p-2 shadow-lg"
        role="radiogroup"
        aria-label="Accent color"
      >
        <div class="mb-1.5 px-1 text-xs font-medium text-muted">Accent</div>
        <div class="grid grid-cols-6 gap-1">
          {#each ACCENTS as accent (accent.hue)}
            <button
              role="radio"
              aria-checked={theme.accentHue === accent.hue}
              aria-label={accent.name}
              title={accent.name}
              class="swatch"
              style="--swatch: oklch(0.6 0.17 {accent.hue})"
              onclick={() => setAccentHue(accent.hue)}
            ></button>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .selected {
    background: var(--surface-3);
    color: var(--text);
    box-shadow: 0 1px 2px oklch(0 0 0 / 0.08);
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 999px;
    background: var(--swatch);
    transition: transform 120ms var(--ease-out);
  }
  .swatch:hover {
    transform: scale(1.1);
  }
  .swatch[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface-2),
      0 0 0 4px var(--swatch);
  }
</style>
