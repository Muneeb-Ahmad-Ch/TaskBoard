<script lang="ts">
  import { CornerDownLeft, Search } from '@lucide/svelte'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import { swatch } from '../lib/format'
  import { inTauri, ipc } from '../lib/ipc'
  import { activeViewChanged, app, applySavedView, redo, setScope, undo, updateActiveView } from '../lib/store.svelte'
  import { layout, pinProject, unpinProject } from '../lib/layout.svelte'
  import { setThemePref } from '../lib/theme.svelte'
  import { openChangelog } from '../lib/about'
  import { cal, newMeeting } from '../lib/cal.svelte'

  interface Command {
    id: string
    label: string
    group: string
    hint?: string
    color?: string
    run: () => void
  }

  let query = $state('')
  let active = $state(0)
  let input: HTMLInputElement | undefined = $state()
  let taskHits = $state<TaskRow[]>([])

  const commands = $derived.by((): Command[] => {
    const list: Command[] = [
      { id: 'new', group: 'Actions', label: 'New task', hint: 'N', run: () => app.quickAddFocus++ },
      { id: 'undo', group: 'Actions', label: app.history.undo ? `Undo: ${app.history.undo}` : 'Undo', hint: 'Ctrl Z', run: () => void undo() },
      { id: 'redo', group: 'Actions', label: app.history.redo ? `Redo: ${app.history.redo}` : 'Redo', hint: 'Ctrl Shift Z', run: () => void redo() },
      { id: 'done', group: 'Actions', label: app.showDone ? 'Hide completed tasks' : 'Show completed tasks', run: () => (app.showDone = !app.showDone) },
      { id: 'v-table', group: 'View', label: 'Table view', hint: '1', run: () => (app.view = 'table') },
      { id: 'v-board', group: 'View', label: 'Board view', hint: '2', run: () => (app.view = 'board') },
      { id: 'v-cal', group: 'View', label: 'Calendar view', hint: '3', run: () => (app.view = 'calendar') },
      { id: 'v-dash', group: 'View', label: 'Dashboard', run: () => (app.view = 'dashboard') },
      { id: 'v-meet', group: 'View', label: 'Meetings', run: () => (app.view = 'meetings') },
      { id: 'layout', group: 'Actions', label: 'Customize layout (sidebar, views, columns)…', run: () => { app.settingsSection = 'layout'; app.settingsOpen = true } },
      { id: 'settings', group: 'Actions', label: 'Settings…', run: () => (app.settingsOpen = true) },
      { id: 'ai', group: 'Actions', label: 'Connect an AI assistant…', run: () => { app.settingsSection = 'ai'; app.settingsOpen = true } },
      { id: 'shortcuts', group: 'Actions', label: 'Keyboard shortcuts', hint: '?', run: () => (app.shortcutsOpen = true) },
      ...(app.scope.kind === 'project'
        ? [
            layout.pinned.includes(app.scope.id)
              ? { id: 'unpin', group: 'Actions', label: 'Unpin this project', run: () => app.scope.kind === 'project' && unpinProject(app.scope.id) }
              : { id: 'pin', group: 'Actions', label: 'Pin this project to the top', run: () => app.scope.kind === 'project' && pinProject(app.scope.id) },
          ]
        : []),
      { id: 'changelog', group: 'Actions', label: "What's new (changelog)", run: openChangelog },
      { id: 'about', group: 'Actions', label: 'About Taskboard', run: () => { app.settingsSection = 'about'; app.settingsOpen = true } },
      { id: 'find-slot', group: 'Actions', label: 'Find a meeting slot…', run: () => (cal.findSlotOpen = true) },
      { id: 'new-meeting', group: 'Actions', label: 'New meeting…', run: newMeeting },
      { id: 'new-block', group: 'Actions', label: 'New time block…', run: () => { const s = new Date(); s.setMinutes(0, 0, 0); s.setHours(s.getHours() + 1); cal.draft = { kind: 'block', start: s.getTime(), end: s.getTime() + 3_600_000 } } },
      { id: 'hours', group: 'Actions', label: 'Working hours…', run: () => (cal.workingHoursOpen = true) },
      { id: 't-sys', group: 'Theme', label: 'Theme: System', run: () => setThemePref('system') },
      { id: 't-light', group: 'Theme', label: 'Theme: Light', run: () => setThemePref('light') },
      { id: 't-dark', group: 'Theme', label: 'Theme: Dark', run: () => setThemePref('dark') },
      { id: 'g-today', group: 'Go to', label: 'Today', run: () => setScope({ kind: 'today' }) },
      { id: 'g-up', group: 'Go to', label: 'Upcoming', run: () => setScope({ kind: 'upcoming' }) },
      { id: 'g-inbox', group: 'Go to', label: 'Inbox', run: () => setScope({ kind: 'inbox' }) },
      { id: 'g-all', group: 'Go to', label: 'All work', run: () => setScope({ kind: 'all' }) },
    ]
    for (const v of app.views) list.push({ id: `sv-${v.id}`, group: 'Saved views', label: v.name, run: () => applySavedView(v) })
    const active = app.views.find((v) => v.id === app.activeViewId)
    if (active && activeViewChanged())
      list.push({ id: 'sv-update', group: 'Actions', label: `Update view “${active.name}” with current filters`, run: () => void updateActiveView() })
    list.push({ id: 'new-section', group: 'Actions', label: 'New section…', run: () => (app.sectionDialog = { id: null }) })
    for (const s of app.sections)
      list.push({ id: `s-${s.id}`, group: 'Sections', label: s.name, color: s.color, run: () => setScope({ kind: 'section', id: s.id }) })
    for (const c of app.clients)
      list.push({ id: `c-${c.id}`, group: 'Groups', label: c.name, color: c.color, run: () => setScope({ kind: 'client', id: c.id }) })
    for (const p of app.projects)
      if (!p.is_inbox)
        list.push({
          id: `p-${p.id}`,
          group: 'Projects',
          label: p.client_name ? `${p.client_name} / ${p.name}` : p.name,
          color: p.color,
          run: () => setScope({ kind: 'project', id: p.id }),
        })
    return list
  })

  /** Subsequence fuzzy score: higher is better, -1 = no match. */
  function score(text: string, q: string): number {
    if (!q) return 0
    const t = text.toLowerCase()
    const i = t.indexOf(q)
    if (i >= 0) return 1000 - i - t.length * 0.01 + (i === 0 || t[i - 1] === ' ' || t[i - 1] === '/' ? 200 : 0)
    let at = 0
    let s = 0
    for (const ch of q) {
      const found = t.indexOf(ch, at)
      if (found < 0) return -1
      s += found === at ? 3 : 1
      at = found + 1
    }
    return s
  }

  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase()
    return commands
      .map((c) => ({ c, s: score(c.label, q) }))
      .filter((x) => x.s >= 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, q ? 30 : 60)
      .map((x) => x.c)
  })

  // Task search through the core's full-text index.
  $effect(() => {
    const q = query.trim()
    if (q.length < 2 || !inTauri) {
      taskHits = []
      return
    }
    const t = setTimeout(async () => {
      try {
        const page = await ipc.listTasks({ search: q, limit: 8, sort: 'updated', descending: true })
        if (q === query.trim()) taskHits = page.rows
      } catch {
        taskHits = []
      }
    }, 80)
    return () => clearTimeout(t)
  })

  const items = $derived([
    ...matches,
    ...taskHits.map(
      (t): Command => ({
        id: `t-${t.id}`,
        group: 'Tasks',
        label: t.title,
        hint: t.client_name ? `${t.client_name} / ${t.project_name}` : t.project_name,
        color: t.project_color,
        run: () => {
          setScope({ kind: 'project', id: t.project_id })
          app.openTaskId = t.id
        },
      }),
    ),
  ])

  $effect(() => {
    void query
    active = 0
  })

  $effect(() => {
    input?.focus()
  })

  function run(cmd: Command | undefined) {
    if (!cmd) return
    app.paletteOpen = false
    cmd.run()
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      active = Math.min(items.length - 1, active + 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      active = Math.max(0, active - 1)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      run(items[active])
    } else if (e.key === 'Escape') {
      e.preventDefault()
      app.paletteOpen = false
    }
  }

  $effect(() => {
    document.getElementById(`cmd-${active}`)?.scrollIntoView({ block: 'nearest' })
  })
</script>

<div class="backdrop fixed inset-0 z-40 grid justify-items-center bg-black/25 pt-[12vh]" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (app.paletteOpen = false)}>
  <div class="dialog flex h-fit max-h-[70vh] w-[min(640px,92vw)] flex-col overflow-hidden rounded-xl border border-border bg-surface-2 shadow-2xl" role="dialog" aria-label="Command palette" aria-modal="true">
    <div class="flex h-12 shrink-0 items-center gap-2.5 border-b border-border px-4">
      <Search size={17} class="text-subtle" />
      <input
        bind:this={input}
        bind:value={query}
        class="flex-1 bg-transparent text-[15px] outline-none placeholder:text-subtle"
        placeholder="Search tasks, projects, groups, or type a command…"
        role="combobox"
        aria-expanded="true"
        aria-controls="cmd-list"
        aria-activedescendant="cmd-{active}"
        {onkeydown}
      />
      <kbd class="rounded border border-border px-1 text-[11px] text-subtle">Esc</kbd>
    </div>
    <ul id="cmd-list" class="min-h-0 flex-1 overflow-y-auto p-1.5" role="listbox">
      {#each items as cmd, i (cmd.id)}
        {#if i === 0 || items[i - 1].group !== cmd.group}
          <li class="px-2.5 pt-2 pb-1 text-[11px] font-semibold tracking-wide text-subtle uppercase" role="presentation">{cmd.group}</li>
        {/if}
        <li
          id="cmd-{i}"
          role="option"
          aria-selected={i === active}
          class="flex h-9 cursor-pointer items-center gap-2.5 rounded-md px-2.5 {i === active ? 'bg-accent-soft text-accent-soft-text' : ''}"
          onpointermove={() => (active = i)}
          onclick={() => run(cmd)}
          onkeydown={() => {}}
        >
          {#if cmd.color}<span class="size-2 shrink-0 rounded-full" style="background: {swatch(cmd.color)}"></span>{/if}
          <span class="flex-1 truncate">{cmd.label}</span>
          {#if cmd.hint}<span class="shrink-0 truncate text-xs text-subtle">{cmd.hint}</span>{/if}
          {#if i === active}<CornerDownLeft size={13} class="shrink-0 opacity-70" />{/if}
        </li>
      {:else}
        <li class="px-3 py-6 text-center text-muted">No results</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .backdrop {
    animation: fade 120ms var(--ease-out);
  }
  .dialog {
    animation: rise 160ms var(--ease-out);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.99);
    }
  }
</style>
