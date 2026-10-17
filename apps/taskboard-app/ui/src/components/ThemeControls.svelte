<script lang="ts">
  // Quick theme switch in the top bar. Accent color lives in Settings → Appearance.
  import { Monitor, Moon, Sun } from '@lucide/svelte'
  import { setThemePref, theme, type ThemePref } from '../lib/theme.svelte'

  const modes: { pref: ThemePref; label: string; icon: typeof Sun }[] = [
    { pref: 'system', label: 'System theme', icon: Monitor },
    { pref: 'light', label: 'Light theme', icon: Sun },
    { pref: 'dark', label: 'Dark theme', icon: Moon },
  ]
</script>

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

<style>
  .selected {
    background: var(--surface-3);
    color: var(--text);
    box-shadow: 0 1px 2px oklch(0 0 0 / 0.08);
  }
</style>
