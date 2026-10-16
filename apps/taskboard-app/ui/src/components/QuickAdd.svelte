<script lang="ts">
  import { AlertTriangle, CalendarDays, Clock, Flag, Folder, Hash, Plus, Timer } from '@lucide/svelte'
  import type { QuickAddPreview } from '../lib/bindings/QuickAddPreview'
  import type { TokenKind } from '../lib/bindings/TokenKind'
  import { dueLabel, minutes, swatch, todayIso } from '../lib/format'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { app, scopeProjectId, scopeTitle } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'

  let line = $state('')
  let preview = $state<QuickAddPreview | null>(null)
  let input: HTMLInputElement | undefined = $state()
  let scroll = $state(0)
  let busy = $state(false)

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

<div class="quick-add rounded-[var(--radius)] border border-border bg-surface-2 focus-within:border-accent">
  <div class="flex h-10 items-center gap-2 px-3">
    <Plus size={16} class="shrink-0 text-subtle" />
    <div class="relative min-w-0 flex-1">
      <div class="mirror" aria-hidden="true" style="transform: translateX({-scroll}px)">
        {#each segments as seg, i (i)}<span class="seg {seg.kind ?? ''}">{seg.text}</span>{/each}
      </div>
      <input
        bind:this={input}
        bind:value={line}
        class="field"
        placeholder="Add a task — e.g. Send wireframes tomorrow 3pm #design !high @website ~2h"
        aria-label="Quick add task"
        spellcheck="false"
        autocomplete="off"
        {onkeydown}
        onscroll={(e) => (scroll = e.currentTarget.scrollLeft)}
      />
    </div>
    <kbd class="hidden shrink-0 rounded border border-border px-1 text-[11px] text-subtle lg:block">Enter</kbd>
  </div>

  {#if preview && line.trim()}
    <div class="flex flex-wrap items-center gap-1.5 border-t border-border px-3 py-2 text-xs">
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
      {#if preview.parsed.estimate_minutes}<span class="chip"><Timer size={12} /> {minutes(preview.parsed.estimate_minutes)}</span>{/if}
    </div>
  {/if}
</div>

<style>
  .mirror,
  .field {
    font: inherit;
    font-size: 13.5px;
    letter-spacing: normal;
    white-space: pre;
  }
  .field {
    position: relative;
    width: 100%;
    background: transparent;
    outline: none;
    caret-color: var(--accent);
  }
  .field::placeholder {
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
  .seg.estimate {
    background: color-mix(in oklch, var(--text-muted) 22%, transparent);
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
