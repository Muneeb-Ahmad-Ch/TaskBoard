<script lang="ts">
  import { Database, Redo2, Undo2 } from '@lucide/svelte'
  import type { DbInfo } from '../lib/bindings/DbInfo'
  import { inTauri, ipc, isErrorInfo } from '../lib/ipc'
  import { app, redo, undo } from '../lib/store.svelte'
  import { appVersion, openChangelog } from '../lib/about'

  let version = $state('')
  void appVersion().then((v) => (version = v))

  let info = $state<DbInfo | null>(null)
  let error = $state<string | null>(null)

  $effect(() => {
    if (!inTauri) {
      error = 'Browser preview: no database'
      return
    }
    ipc
      .dbInfo()
      .then((value) => (info = value))
      .catch((err: unknown) => (error = isErrorInfo(err) ? err.message : String(err)))
  })
</script>

<footer class="flex h-7 shrink-0 items-center gap-3 border-t border-border px-4 text-[11.5px] text-subtle">
  {#if version}
    <button class="version" title="What's new in Taskboard (opens the changelog)" onclick={openChangelog}>Taskboard v{version}</button>
    <span aria-hidden="true">·</span>
  {/if}
  <Database size={12} />
  {#if info}
    <span>SQLite {info.sqlite_version} · {info.journal_mode.toUpperCase()} · schema v{info.schema_version}</span>
  {:else if error}
    <span>{error}</span>
  {:else}
    <span>Opening database…</span>
  {/if}
  <span class="ml-auto flex items-center gap-1">
    <button class="hist" disabled={!app.history.undo} title={app.history.undo ? `Undo: ${app.history.undo} (Ctrl+Z)` : 'Nothing to undo'} onclick={() => void undo()}><Undo2 size={13} /></button>
    <button class="hist" disabled={!app.history.redo} title={app.history.redo ? `Redo: ${app.history.redo} (Ctrl+Shift+Z)` : 'Nothing to redo'} onclick={() => void redo()}><Redo2 size={13} /></button>
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
    width: 22px;
    height: 20px;
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
