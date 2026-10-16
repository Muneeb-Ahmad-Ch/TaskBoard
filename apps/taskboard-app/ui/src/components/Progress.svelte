<script lang="ts">
  /** Compact progress bar; turns green at 100%. */
  let {
    value,
    width = 56,
    height = 5,
    label = false,
  }: { value: number; width?: number; height?: number; label?: boolean } = $props()

  const pct = $derived(Math.max(0, Math.min(100, Math.round(value))))
</script>

<span class="inline-flex items-center gap-2" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
  <span class="track" style="width: {width}px; height: {height}px">
    <span class="fill" class:complete={pct === 100} style="width: {pct}%"></span>
  </span>
  {#if label}<span class="tabular text-xs text-muted">{pct}%</span>{/if}
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
    background: var(--accent);
    transition: width 200ms var(--ease-out);
  }
  .fill.complete {
    background: var(--success);
  }
</style>
