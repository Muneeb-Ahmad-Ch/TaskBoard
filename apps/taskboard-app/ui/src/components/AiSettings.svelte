<script lang="ts">
  // Settings → AI assistants: the on/off switch, ready-made connection
  // snippets for each assistant, a self-test, and who has connected.
  import { Check, Copy, PlugZap } from '@lucide/svelte'
  import type { AppSettings } from '../lib/bindings/AppSettings'
  import { errorMessage, ipc, type McpInfo, type McpTest } from '../lib/ipc'
  import { toast } from '../lib/toast.svelte'
  import Switch from './Switch.svelte'

  let {
    settings,
    onchange,
  }: { settings: AppSettings | null; onchange: (patch: Partial<AppSettings>) => void } = $props()

  let info = $state<McpInfo | null>(null)
  let test = $state<McpTest | null>(null)
  let testing = $state(false)
  let copied = $state<string | null>(null)

  async function load() {
    try {
      info = await ipc.mcpInfo()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }
  $effect(() => {
    void load()
    // Assistants that connect while this page is open show up within a few seconds.
    const timer = setInterval(() => void load(), 5000)
    return () => clearInterval(timer)
  })

  const SNIPPETS = $derived.by(() => {
    if (!info) return []
    const exe = info.command
    const json = JSON.stringify({ mcpServers: { tb: { command: exe, args: info.args } } }, null, 2)
    const toml = `[mcp_servers.tb]\ncommand = ${JSON.stringify(exe)}\nargs = [${info.args.map((a) => JSON.stringify(a)).join(', ')}]`
    const quoted = exe.includes(' ') ? `"${exe}"` : exe
    return [
      { id: 'claude-desktop', name: 'Claude Desktop', hint: 'Claude → Settings → Developer → Edit Config, paste into claude_desktop_config.json', text: json, label: 'Copy config' },
      { id: 'claude-code', name: 'Claude Code', hint: 'Run once in a terminal.', text: `claude mcp add tb -- ${quoted} ${info.args.join(' ')}`, label: 'Copy command' },
      { id: 'codex', name: 'Codex', hint: 'Add to ~/.codex/config.toml', text: toml, label: 'Copy config' },
      { id: 'other', name: 'Cursor, Windsurf, VS Code and others', hint: 'Any app that supports MCP servers over stdio; paste into its MCP settings.', text: json, label: 'Copy config' },
    ]
  })

  async function copy(id: string, text: string) {
    try {
      await navigator.clipboard.writeText(text)
      copied = id
      setTimeout(() => (copied = null), 1500)
    } catch {
      toast('Could not copy to the clipboard', { kind: 'error' })
    }
  }

  async function runTest() {
    testing = true
    test = null
    try {
      test = await ipc.mcpSelfTest()
    } catch (err) {
      test = { ok: false, message: errorMessage(err), tools: 0 }
    } finally {
      testing = false
      void load()
    }
  }

  const ago = (ms: number) => {
    const s = Math.max(0, Math.round((Date.now() - ms) / 1000))
    if (s < 60) return 'just now'
    if (s < 3600) return `${Math.round(s / 60)} min ago`
    if (s < 86_400) return `${Math.round(s / 3600)} h ago`
    return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(ms)
  }
  const prettyName = (n: string) =>
    ({ 'claude-desktop': 'Claude Desktop', 'claude-code': 'Claude Code', 'claude-ai': 'Claude', codex: 'Codex', 'codex-cli': 'Codex', cursor: 'Cursor' })[n.toLowerCase()] ?? n
</script>

<p class="mb-3 text-xs text-muted">
  Taskboard is an MCP server, the standard that Claude, Codex, Cursor and other assistants use to work with apps. A connected
  assistant can read, add, change and remove sections, groups, projects, tasks, meetings and logged time. Every change it makes
  is undoable (Ctrl+Z) and shows up here as it happens. Nothing leaves this computer unless the assistant you connect sends it.
</p>

<section class="card" data-setting="ai-access">
  <div class="set-row">
    <div class="min-w-0 flex-1">
      <div class="text-[13px] font-medium">Let AI assistants read and change your data</div>
      <div class="mt-0.5 text-xs text-muted">Off: connected assistants get a message saying access is turned off.</div>
    </div>
    {#if settings}
      <Switch label="Let AI assistants read and change your data" checked={settings.ai_access} onchange={(v) => onchange({ ai_access: v })} />
    {/if}
  </div>
</section>

<section class="card" data-setting="ai-connect">
  <h4 class="card-title">Connect an assistant</h4>
  {#each SNIPPETS as s (s.id)}
    <div class="set-row">
      <div class="min-w-0 flex-1">
        <div class="text-[13px] font-medium">{s.name}</div>
        <div class="mt-0.5 text-xs text-muted">{s.hint}</div>
      </div>
      <button class="btn !h-7 min-w-[132px] justify-center text-xs" onclick={() => void copy(s.id, s.text)}>
        {#if copied === s.id}<Check size={13} /> Copied{:else}<Copy size={13} /> {s.label}{/if}
      </button>
    </div>
  {/each}
  <div class="set-row" data-setting="ai-test">
    <div class="min-w-0 flex-1">
      <div class="text-[13px] font-medium">Check that it works</div>
      <div class="mt-0.5 text-xs text-muted">
        {#if test}
          <span class={test.ok ? 'text-success' : 'text-danger'}>{test.message}</span>
        {:else}
          Starts the server the way an assistant would and asks it for its tools.
        {/if}
      </div>
    </div>
    <button class="btn !h-7 min-w-[132px] justify-center text-xs" disabled={testing} onclick={() => void runTest()}><PlugZap size={13} /> {testing ? 'Testing…' : 'Run test'}</button>
  </div>
  {#if info}
    <p class="px-4 pb-3 text-xs text-subtle break-all">
      Server: <code>{info.command} {info.args.join(' ')}</code>. These set it up as <code>tb</code>, so you can say "tb" (or "Taskboard") to your assistant, e.g. "what's due in tb today?". A setup under another name keeps working.
    </p>
  {/if}
</section>

<section class="card">
  <h4 class="card-title">Connected assistants</h4>
  {#if info && info.connections.length}
    <ul class="px-4 pb-2" aria-label="Connected assistants">
      {#each info.connections as c (c.name)}
        <li class="flex items-center gap-3 py-1.5 text-[13px]">
          <span class="flex-1">{prettyName(c.name)}{#if c.version}<span class="text-subtle"> {c.version}</span>{/if}</span>
          <span class="text-xs text-muted">{ago(c.last_seen)} · {c.connections} {c.connections === 1 ? 'session' : 'sessions'}</span>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="px-4 pb-3 text-xs text-muted">None yet. Connect one above; it appears here the first time it talks to Taskboard.</p>
  {/if}
</section>

<style>
  .card {
    margin-bottom: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-1);
  }
  .card-title {
    padding: 12px 16px 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-subtle);
  }
  .set-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 16px;
  }
  .set-row + .set-row {
    border-top: 1px solid var(--border);
  }
  code {
    font-size: 11px;
  }
</style>
