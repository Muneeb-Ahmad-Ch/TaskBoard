<script lang="ts">
  import { CalendarDays, ChevronDown, Inbox, Plus, Search, Star, User } from '@lucide/svelte'
  import { clients, personal } from '../lib/preview'

  let collapsed = $state<Record<string, boolean>>({})
  let selected = $state('All work')

  const nav = [
    { label: 'Inbox', icon: Inbox, count: 6 },
    { label: 'Today', icon: Star, count: 4 },
    { label: 'Upcoming', icon: CalendarDays, count: 18 },
  ]

  function toggle(name: string) {
    collapsed[name] = !collapsed[name]
  }
</script>

<aside class="flex h-full w-[260px] shrink-0 flex-col bg-[var(--sidebar-bg)] px-2.5 pt-3 pb-2" aria-label="Navigation">
  <div class="mb-3 flex items-center gap-2 px-2">
    <div class="grid size-7 place-items-center rounded-lg bg-accent text-on-accent shadow-sm">
      <svg viewBox="0 0 16 16" class="size-4" aria-hidden="true"><path fill="currentColor" d="M3 3h4v4H3zm6 0h4v4H9zM3 9h4v4H3zm6.5 1.8 1.2 1.2 2.3-2.6.9.8-3.2 3.6-2.1-2.1z" /></svg>
    </div>
    <span class="font-display text-[15px] font-semibold tracking-tight">Taskboard</span>
  </div>

  <button class="mb-3 flex h-8 items-center gap-2 rounded-md border border-border bg-surface-1 px-2.5 text-muted transition-colors hover:border-border-strong">
    <Search size={14} />
    <span class="min-w-0 flex-1 truncate text-left">Search or run a command</span>
    <kbd class="shrink-0 rounded border border-border px-1 font-sans text-[11px] whitespace-nowrap text-subtle">Ctrl K</kbd>
  </button>

  <nav class="flex flex-col gap-px">
    {#each nav as item (item.label)}
      <button
        class="nav-item"
        class:active={selected === item.label}
        aria-current={selected === item.label ? 'page' : undefined}
        onclick={() => (selected = item.label)}
      >
        <item.icon size={15} />
        <span class="flex-1 text-left">{item.label}</span>
        <span class="tabular text-xs text-subtle">{item.count}</span>
      </button>
    {/each}
  </nav>

  <div class="mt-5 mb-1 flex items-center justify-between px-2">
    <span class="text-[11px] font-semibold tracking-wide text-subtle uppercase">Clients</span>
    <button class="icon-btn" aria-label="Add client"><Plus size={14} /></button>
  </div>

  <div class="-mx-1 flex min-h-0 flex-1 flex-col gap-px overflow-y-auto px-1">
    {#each clients as client (client.name)}
      <button class="nav-item" aria-expanded={!collapsed[client.name]} onclick={() => toggle(client.name)}>
        <span class="grid size-[15px] place-items-center rounded text-[9px] font-bold text-white" style="background: oklch(0.62 0.15 {client.hue})">
          {client.name[0]}
        </span>
        <span class="flex-1 truncate text-left font-medium">{client.name}</span>
        <ChevronDown size={14} class="text-subtle transition-transform {collapsed[client.name] ? '-rotate-90' : ''}" />
      </button>
      {#if !collapsed[client.name]}
        {#each client.projects as project (project.name)}
          <button
            class="nav-item pl-8"
            class:active={selected === project.name}
            onclick={() => (selected = project.name)}
          >
            <span class="size-2 rounded-full" style="background: oklch(0.65 0.15 {project.hue})"></span>
            <span class="flex-1 truncate text-left">{project.name}</span>
            <span class="tabular text-xs text-subtle">{project.open}</span>
          </button>
        {/each}
      {/if}
    {/each}

    <div class="mt-4 mb-1 px-2">
      <span class="text-[11px] font-semibold tracking-wide text-subtle uppercase">Personal</span>
    </div>
    {#each personal as project (project.name)}
      <button class="nav-item" class:active={selected === project.name} onclick={() => (selected = project.name)}>
        <span class="size-2 rounded-full" style="background: oklch(0.65 0.15 {project.hue})"></span>
        <span class="flex-1 truncate text-left">{project.name}</span>
        <span class="tabular text-xs text-subtle">{project.open}</span>
      </button>
    {/each}
  </div>

  <div class="mt-2 flex items-center gap-2 border-t border-border px-2 pt-2 text-muted">
    <User size={14} />
    <span class="text-xs">Local workspace · no account</span>
  </div>
</aside>

<style>
  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    height: 30px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    color: var(--text);
    transition: background-color 120ms var(--ease-out);
  }
  .nav-item:hover {
    background: var(--hover);
  }
  .nav-item.active {
    background: var(--accent-soft);
    color: var(--accent-soft-text);
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    color: var(--text-subtle);
  }
  .icon-btn:hover {
    background: var(--hover);
    color: var(--text);
  }
</style>
