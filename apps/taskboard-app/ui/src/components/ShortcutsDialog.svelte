<script lang="ts">
  // Every keyboard shortcut in one place (press ? anywhere). Mirrors
  // docs/SHORTCUTS.md.
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  const GROUPS: { title: string; rows: [string, string][] }[] = [
    {
      title: 'Everywhere',
      rows: [
        ['Ctrl+K', 'Search and commands (works while typing)'],
        ['N', 'Add a task'],
        ['/ or Ctrl+F', 'Search tasks (Filter, Sort and Group are next to it); Ctrl+F works while typing'],
        ['Ctrl+= / Ctrl+−, Ctrl+0', 'Zoom in / out, reset zoom'],
        ['Ctrl+F in Settings', 'Search the settings'],
        ['1 / 2 / 3', 'Switch view tab'],
        ['Ctrl+Z', 'Undo'],
        ['Ctrl+Shift+Z or Ctrl+Y', 'Redo'],
        ['?', 'This list'],
        ['Esc', 'Close the dialog, menu or task panel'],
        ['Ctrl+Shift+Space', 'From anywhere in Windows: bring Taskboard forward to add a task'],
      ],
    },
    {
      title: 'Table',
      rows: [
        ['↑ ↓ or J / K', 'Move the selection (Shift extends it)'],
        ['Enter', 'Open the task'],
        ['F2 or E', 'Rename'],
        ['Space', 'Mark done / not done'],
        ['A', 'Archive selected tasks (restore from Settings → Data)'],
        ['Delete', 'Delete (undoable)'],
        ['Ctrl+A', 'Select all loaded rows'],
        ['Esc', 'Clear selection'],
        ['Click a column header', 'Sort by it; again to reverse'],
        ['Enter or ← → on a group header', 'Fold or unfold the group'],
      ],
    },
    {
      title: 'Board',
      rows: [
        ['Tab', 'Focus a card'],
        ['Enter', 'Open the task'],
        ['Space', 'Pick up a card; arrows move it; Space drops, Esc cancels'],
      ],
    },
    {
      title: 'Calendar',
      rows: [
        ['← / →', 'Previous / next period'],
        ['T', 'Today'],
        ['M / W / D / A', 'Month / Week / Day / Agenda'],
        ['Alt + drag', 'Create a time block instead of a meeting'],
      ],
    },
    {
      title: 'Sidebar',
      rows: [
        ['Alt+↑ / Alt+↓', 'Move the focused section, group or project'],
        ['Double-click', 'Rename'],
        ['Right-click', 'More actions (pin, move, archive…)'],
        ['Drag', 'Reorder sections, groups and projects; drop tasks on a project'],
      ],
    },
    {
      title: 'Quick add',
      rows: [
        ['#design', 'Tag'],
        ['!high  !urgent  !low', 'Priority'],
        ['@website', 'Project'],
        ['tomorrow  fri  oct 20', 'Due date'],
        ['3pm  at 9:30', 'Due time'],
        ['~30m  ~2h', 'Estimate'],
        ['every mon  every! 3 days', 'Repeat (on a schedule / after completion)'],
      ],
    },
  ]
</script>

<Modal title="Keyboard shortcuts" {onclose} width={680}>
  <!-- Focusable so the list can be scrolled with the keyboard in small windows. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="grid gap-x-6 gap-y-4 rounded sm:grid-cols-2" tabindex="0" role="region" aria-label="Shortcut list">
    {#each GROUPS as g (g.title)}
      <section>
        <h3 class="mb-1.5 text-xs font-semibold text-muted">{g.title}</h3>
        <dl class="flex flex-col gap-1">
          {#each g.rows as [keys, action] (keys + action)}
            <div class="flex items-baseline gap-3 text-[13px]">
              <dt class="w-[132px] shrink-0"><kbd>{keys}</kbd></dt>
              <dd class="min-w-0 text-muted">{action}</dd>
            </div>
          {/each}
        </dl>
      </section>
    {/each}
  </div>
  {#snippet footer()}
    <span class="flex-1 text-xs text-subtle">Shortcuts don't fire while you're typing in a field, except Ctrl+K and Ctrl+F.</span>
    <button class="btn" onclick={onclose}>Close</button>
  {/snippet}
</Modal>

<style>
  kbd {
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--border-strong);
    font-family: inherit;
    font-size: 11.5px;
    white-space: nowrap;
  }
</style>
