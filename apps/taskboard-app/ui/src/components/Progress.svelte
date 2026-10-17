<script lang="ts">
  /**
   * Compact progress bar. The fill shifts from blue (just started) through
   * teal to green as work completes; 100% is the success green with a check.
   * A cool ramp avoids the muddy olive midpoint of an amber→green ramp.
   */
  import { Check } from '@lucide/svelte'

  let {
    value,
    width = 56,
    height = 5,
    label = false,
    name = 'Progress',
  }: { value: number; width?: number; height?: number; label?: boolean; name?: string } = $props()

  const pct = $derived(Math.max(0, Math.min(100, Math.round(value))))
  /** OKLCH hue: 255 (blue) at 0% → 165 (green-teal) just before 100%. */
  const hue = $derived(255 - (90 * pct) / 100)
</script>

<span class="inline-flex items-center gap-1.5" role="progressbar" aria-label={name} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
  <span class="track" style="width: {width}px; height: {height}px">
    <span class="fill" class:complete={pct === 100} style="width: {pct}%; --h: {hue}"></span>
  </span>
  {#if label}
    <span class="tabular inline-flex items-center gap-0.5 text-xs {pct === 100 ? 'font-medium text-text' : 'text-muted'}">
      {#if pct === 100}<Check size={12} class="text-success" strokeWidth={3} aria-hidden="true" />{/if}{pct}%
    </span>
  {/if}
</span>

<style>
  .track {
    display: inline-block;
    flex-shrink: 0;
    overflow: hidden;
    border-radius: 99px;
    background: var(--surface-3);
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: 99px;
    background: light-dark(oklch(0.6 0.13 var(--h)), oklch(0.72 0.12 var(--h)));
    transition:
      width 200ms var(--ease-out),
      background-color 200ms var(--ease-out);
  }
  .fill.complete {
    background: var(--success);
  }
</style>
