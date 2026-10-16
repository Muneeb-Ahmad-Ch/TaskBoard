<script lang="ts">
  import { Database } from '@lucide/svelte'
  import type { DbInfo } from '../lib/bindings/DbInfo'
  import { inTauri, ipc, isErrorInfo } from '../lib/ipc'

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
  <Database size={12} />
  {#if info}
    <span>SQLite {info.sqlite_version}</span>
    <span aria-hidden="true">·</span>
    <span>{info.journal_mode.toUpperCase()}</span>
    <span aria-hidden="true">·</span>
    <span class={info.fts5 ? 'text-success' : 'text-danger'}>FTS5 {info.fts5 ? 'ready' : 'missing'}</span>
    <span aria-hidden="true">·</span>
    <span>schema v{info.schema_version}</span>
  {:else if error}
    <span>{error}</span>
  {:else}
    <span>Opening database…</span>
  {/if}
  <span class="ml-auto rounded bg-warn/15 px-1.5 py-px font-medium text-warn">Preview data · M0</span>
</footer>
