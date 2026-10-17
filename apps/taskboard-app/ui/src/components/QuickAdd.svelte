<script lang="ts">
  import { AlertTriangle, CalendarDays, Clock, Flag, Folder, Hash, Plus, Repeat, Timer } from '@lucide/svelte'
  import type { ProjectSummary } from '../lib/bindings/ProjectSummary'
  import type { QuickAddPreview } from '../lib/bindings/QuickAddPreview'
  import type { TokenKind } from '../lib/bindings/TokenKind'
  import { dueLabel, minutes, swatch, todayIso } from '../lib/format'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { app, scopeProjectId, scopeTitle } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'

  /** Where the box sits; the suggestions and preview open toward the list. */
  let { placement = 'bottom' }: { placement?: 'top' | 'bottom' } = $props()

  let line = $state('')
  let preview = $state<QuickAddPreview | null>(null)
  let input: HTMLInputElement | undefined = $state()
  let scroll = $state(0)
  let busy = $state(false)
  /** Caret position, for @project suggestions. */
  let caret = $state(0)
  let pick = $state(0)
  /** True once ↑/↓ chose a suggestion; then Enter inserts it instead of adding the task. */
  let navigated = $state(false)
  let dismissed = $state(false)

  /** The `@word` (or `@"two words`) being typed at the caret, if any. */
  const mention = $derived.by(() => {
    const before = line.slice(0, caret)
    const m = /(?:^|\s)@("?)([^"@]*)$/.exec(before)
    if (!m || (!m[1] && /\s/.test(m[2]))) return null
    return { start: before.length - m[2].length - m[1].length - 1, query: m[2].toLowerCase() }
  })

  const suggestions = $derived.by(() => {
    if (!mention || dismissed) return []
    const q = mention.query
    const label = (p: ProjectSummary) => (p.client_name ? `${p.client_name} / ${p.name}` : p.name)
    return app.projects
      .filter((p) => !p.is_inbox && (!q || label(p).toLowerCase().includes(q)))
      .sort((a, b) => Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q)))
      .slice(0, 8)
  })

  $effect(() => {
    void mention?.query
    pick = 0
    navigated = false
  })

  /** Replaces the `@…` at the caret with the chosen project. */
  function choose(p: ProjectSummary) {
    if (!mention) return
    const name = p.client_name ? `${p.client_name}/${p.name}` : p.name
    const token = /[\s"]/.test(name) || p.client_name ? `@"${name.replaceAll('"', '')}"` : `@${name}`
    const end = caret + (line.slice(caret).match(/^[^\s]*/)?.[0].length ?? 0)
    line = `${line.slice(0, mention.start)}${token} ${line.slice(end).trimStart()}`
    const at = mention.start + token.length + 1
    queueMicrotask(() => {
      input?.focus()
      input?.setSelectionRange(at, at)
      caret = at
    })
  }

  const trackCaret = () => {
    caret = input?.selectionStart ?? line.length
    dismissed = false
  }

  // Global shortcut / "N" key ask us to focus.
  $effect(() => {
    if (app.quickAddFocus) input?.focus()
  })

  // Debounced live preview from the core parser.
  $effect(() => {
    const text = line
    if (!text.trim() || !inTauri) {
      preview = null
      return
    }
    const timer = setTimeout(async () => {
      try {
        const p = await ipc.previewQuickAdd(text)
        if (text === line) preview = p
      } catch {
        preview = null
      }
    }, 60)
    return () => clearTimeout(timer)
  })

  const segments = $derived.by(() => {
    const out: { text: string; kind: TokenKind | null }[] = []
    if (!preview || preview.parsed.tokens.length === 0) return [{ text: line, kind: null }]
    // Token offsets are UTF-8 byte offsets; map them to JS string indices.
    const bytes = new TextEncoder().encode(line)
    const decode = (a: number, b: number) => new TextDecoder().decode(bytes.slice(a, b))
    let at = 0
    for (const t of preview.parsed.tokens) {
      if (t.start > at) out.push({ text: decode(at, t.start), kind: null })
      out.push({ text: decode(t.start, t.end), kind: t.kind })
      at = t.end
    }
    out.push({ text: decode(at, bytes.length), kind: null })
    return out
  })

  const dueText = $derived.by(() => {
    const p = preview?.parsed
    let date = p?.due_date ?? null
    if (!date && app.scope.kind === 'today' && p) date = todayIso()
    if (!date) return null
    const time = p?.end_time ? `${p.due_time}–${p.end_time}` : (p?.due_time ?? '')
    return `${dueLabel(date)}${time ? ` ${time}` : ''}`
  })

  const targetName = $derived.by(() => {
    if (preview?.project) {
      const p = preview.project
      return p.client_name ? `${p.client_name} / ${p.name}` : p.name
    }
    return scopeProjectId() ? scopeTitle() : 'Inbox'
  })

  async function submit() {
    const text = line.trim()
    if (!text || busy) return
    busy = true
    try {
      const p = await ipc.previewQuickAdd(text)
      if (!p.parsed.title.trim()) {
        toast('Type a task title', { kind: 'error' })
        return
      }
      if (p.project_missing) {
        toast(`No project matches “@${p.parsed.project_hint}”`, { kind: 'error' })
        return
      }
      const parsed = p.parsed
      const due = parsed.due_date ?? (app.scope.kind === 'today' ? todayIso() : null)
      await ipc.createTask({
        title: parsed.title,
        project_id: p.project?.id ?? scopeProjectId() ?? undefined,
        priority: parsed.priority ?? undefined,
        due_date: due ?? undefined,
        due_time: parsed.due_time ?? undefined,
        estimate_minutes: parsed.estimate_minutes ?? undefined,
        tags: parsed.tags,
        recurrence_rule: parsed.recurrence_rule ?? undefined,
      })
      line = ''
      preview = null
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      busy = false
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (suggestions.length) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        const n = suggestions.length
        pick = (pick + (e.key === 'ArrowDown' ? 1 : n - 1)) % n
        navigated = true
        return
      }
      // Enter adds the task (the parser matches "@web" to Website by itself)
      // unless a suggestion was chosen with the arrows; Tab always inserts.
      if (e.key === 'Tab' || (e.key === 'Enter' && navigated)) {
        e.preventDefault()
        choose(suggestions[pick])
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        dismissed = true
        return
      }
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      void submit()
    } else if (e.key === 'Escape') {
      line = ''
      input?.blur()
    }
  }

  const priorityNames = { none: '', low: 'Low', med: 'Medium', high: 'High', urgent: 'Urgent' }
</script>

<!-- What the line will create: next to the input, on the side facing the list. -->
{#snippet previewRow(edge: string)}
  {#if preview && line.trim()}
    <div class="flex flex-wrap items-center gap-1.5 {edge} border-border px-3 py-2 text-xs">
      {#if preview.project_missing}
        <span class="chip text-warn"><AlertTriangle size={12} /> No project matches “@{preview.parsed.project_hint}”</span>
      {:else}
        <span class="chip">
          {#if preview.project}<span class="size-2 rounded-full" style="background: {swatch(preview.project.color)}"></span>{:else}<Folder size={12} />{/if}
          {targetName}
        </span>
      {/if}
      {#if dueText}<span class="chip date"><CalendarDays size={12} /> {dueText}</span>{/if}
      {#if preview.parsed.end_time}<span class="chip text-muted"><Clock size={12} /> Time range — becomes a meeting in M4</span>{/if}
      {#if preview.parsed.priority && preview.parsed.priority !== 'none'}
        <span class="chip"><Flag size={12} style="color: var(--priority-{preview.parsed.priority})" /> {priorityNames[preview.parsed.priority]}</span>
      {/if}
      {#each preview.parsed.tags as tag (tag)}<span class="chip tag"><Hash size={12} />{tag}</span>{/each}
      {#if preview.recurrence_text}<span class="chip date"><Repeat size={12} /> {preview.recurrence_text}</span>{/if}
      {#if preview.parsed.estimate_minutes}<span class="chip"><Timer size={12} /> {minutes(preview.parsed.estimate_minutes)}</span>{/if}
    </div>
  {/if}
{/snippet}

<div class="quick-add relative rounded-[var(--radius)] border border-border bg-surface-2 focus-within:border-accent">
  {#if suggestions.length}
    <!-- Opens toward the list: up when quick add is at the bottom, down when on top. -->
    <ul id="qa-suggestions" class="suggest" class:down={placement === 'top'} role="listbox" aria-label="Projects">
      {#each suggestions as p, i (p.id)}
        <li
          id="qa-opt-{i}"
          role="option"
          aria-selected={i === pick}
          class:picked={i === pick}
          onpointerdown={(e) => {
            e.preventDefault()
            choose(p)
          }}
          onpointermove={() => (pick = i)}
        >
          <span class="size-2 shrink-0 rounded-full" style="background: {swatch(p.color)}"></span>
          {#if p.client_name}<span class="truncate text-muted">{p.client_name}</span><span class="text-subtle">/</span>{/if}
          <span class="truncate">{p.name}</span>
          {#if p.open_tasks}<span class="ml-auto tabular text-xs text-subtle">{p.open_tasks}</span>{/if}
        </li>
      {/each}
      <li class="hint" role="presentation">Tab to insert · ↑ ↓ then Enter to pick · Esc to close</li>
    </ul>
  {/if}
  {#if placement === 'bottom'}{@render previewRow('border-b')}{/if}
  <div class="flex h-10 items-center gap-2 px-3">
    <Plus size={16} class="shrink-0 text-subtle" />
    <div class="relative min-w-0 flex-1">
      <div class="mirror" aria-hidden="true" style="transform: translateX({-scroll}px)">
        {#each segments as seg, i (i)}<span class="seg {seg.kind ?? ''}">{seg.text}</span>{/each}
      </div>
      <input
        bind:this={input}
        bind:value={line}
        class="qa-input"
        placeholder="Add a task — e.g. Send wireframes tomorrow 3pm #design !high @website ~2h"
        aria-label="Quick add task"
        spellcheck="false"
        autocomplete="off"
        role="combobox"
        aria-expanded={suggestions.length > 0}
        aria-controls="qa-suggestions"
        aria-autocomplete="list"
        aria-activedescendant={suggestions.length ? `qa-opt-${pick}` : undefined}
        {onkeydown}
        oninput={trackCaret}
        onkeyup={trackCaret}
        onclick={trackCaret}
        onblur={() => setTimeout(() => (dismissed = true), 120)}
        onfocus={trackCaret}
        onscroll={(e) => (scroll = e.currentTarget.scrollLeft)}
      />
    </div>
    <kbd class="hidden shrink-0 rounded border border-border px-1 text-[11px] text-subtle lg:block">Enter</kbd>
  </div>
  {#if placement === 'top'}{@render previewRow('border-t')}{/if}

</div>

<style>
  .mirror,
  .qa-input {
    font: inherit;
    font-size: 13.5px;
    letter-spacing: normal;
    white-space: pre;
  }
  .qa-input {
    position: relative;
    width: 100%;
    background: transparent;
    outline: none;
    caret-color: var(--accent);
  }
  .qa-input::placeholder {
    color: var(--text-subtle);
  }
  .mirror {
    position: absolute;
    inset: 0;
    pointer-events: none;
    color: transparent;
  }
  .seg {
    border-radius: 4px;
  }
  .seg.date,
  .seg.time,
  .seg.time_range {
    background: color-mix(in oklch, var(--accent) 22%, transparent);
  }
  .seg.tag {
    background: color-mix(in oklch, var(--success) 24%, transparent);
  }
  .seg.priority {
    background: color-mix(in oklch, var(--danger) 22%, transparent);
  }
  .seg.project {
    background: color-mix(in oklch, var(--warn) 26%, transparent);
  }
  .seg.recurrence {
    background: color-mix(in oklch, var(--accent) 14%, transparent);
    text-decoration: underline dotted var(--accent);
  }
  .seg.estimate {
    background: color-mix(in oklch, var(--text-muted) 22%, transparent);
  }
  .suggest {
    position: absolute;
    left: 28px;
    bottom: calc(100% + 6px);
    z-index: 30;
    width: min(360px, calc(100% - 36px));
    padding: 4px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface-2);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.25);
  }
  .suggest.down {
    top: calc(100% + 6px);
    bottom: auto;
  }
  .suggest li[role='option'] {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding-inline: 8px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
  }
  .suggest li.picked {
    background: var(--accent-soft);
    color: var(--accent-soft-text);
  }
  .suggest .hint {
    padding: 6px 8px 2px;
    font-size: 11px;
    color: var(--text-subtle);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    height: 22px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    background: var(--surface-3);
    color: var(--text);
  }
  .chip.date {
    background: var(--accent-soft);
    color: var(--accent-soft-text);
  }
  .chip.tag {
    color: var(--success);
  }
</style>
