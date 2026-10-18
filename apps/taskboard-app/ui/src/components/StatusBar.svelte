<script lang="ts">
  import { Redo2, Undo2 } from '@lucide/svelte'
  import { inTauri } from '../lib/ipc'
  import { app, redo, undo } from '../lib/store.svelte'
  import { appVersion, openChangelog } from '../lib/about'

  let version = $state('')
  void appVersion().then((v) => (version = v))
</script>

<footer class="flex h-8 shrink-0 items-center gap-3 border-t border-border px-4 text-[11.5px] text-subtle">
  {#if version}
    <button class="version" title="What's new in Taskboard (opens the changelog)" onclick={openChangelog}>Taskboard v{version}</button>
  {/if}
  {#if !inTauri}<span>Browser preview: no database</span>{/if}
  <span class="ml-auto flex items-center gap-1">
    <button class="hist" disabled={!app.history.undo} title={app.history.undo ? `Undo: ${app.history.undo} (Ctrl+Z)` : 'Nothing to undo'} onclick={() => void undo()}><Undo2 size={14} /></button>
    <button class="hist" disabled={!app.history.redo} title={app.history.redo ? `Redo: ${app.history.redo} (Ctrl+Shift+Z)` : 'Nothing to redo'} onclick={() => void redo()}><Redo2 size={14} /></button>
  </span>
</footer>

<style>
  .version {
    color: var(--text-muted);
    text-underline-offset: 2px;
  }
  .version:hover {
    color: var(--text);
    text-decoration: underline;
  }
  .hist {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 4px;
  }
  .hist:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .hist:disabled {
    opacity: 0.35;
  }
</style>
