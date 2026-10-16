<script lang="ts">
  import { Circle, CircleCheck, CircleDashed, CircleDot } from '@lucide/svelte'
  import { tasks, type Category, type Priority } from '../lib/preview'

  const priorityLabel: Record<Priority, string> = {
    urgent: 'Urgent',
    high: 'High',
    med: 'Medium',
    low: 'Low',
    none: '',
  }

  const categoryIcon: Record<Category, typeof Circle> = {
    todo: Circle,
    active: CircleDot,
    done: CircleCheck,
  }
</script>

<div class="overflow-hidden rounded-[var(--radius)] border border-border bg-surface-2">
  <table class="w-full border-collapse text-left">
    <thead>
      <tr class="h-9 border-b border-border text-xs text-muted">
        <th class="w-10 font-medium"><span class="sr-only">Status</span></th>
        <th class="font-medium">Task</th>
        <th class="w-56 font-medium">Client / Project</th>
        <th class="w-28 font-medium">Status</th>
        <th class="w-24 font-medium">Priority</th>
        <th class="w-24 font-medium">Due</th>
        <th class="w-20 pr-4 text-right font-medium">Estimate</th>
      </tr>
    </thead>
    <tbody>
      {#each tasks as task (task.title)}
        {@const Icon = categoryIcon[task.category]}
        <tr class="row h-10 border-b border-border last:border-0" class:done={task.category === 'done'}>
          <td class="pl-3">
            <span class="grid place-items-center {task.category === 'done' ? 'text-success' : task.category === 'active' ? 'text-accent' : 'text-subtle'}">
              {#if task.status === 'Backlog'}<CircleDashed size={16} />{:else}<Icon size={16} />{/if}
            </span>
          </td>
          <td class="title truncate pr-4">{task.title}</td>
          <td class="pr-4">
            <span class="inline-flex max-w-full items-center gap-1.5 truncate text-muted">
              <span class="size-2 shrink-0 rounded-full" style="background: oklch(0.65 0.15 {task.hue})"></span>
              {#if task.client}<span class="truncate">{task.client}</span><span class="text-subtle">/</span>{/if}
              <span class="truncate text-text">{task.project}</span>
            </span>
          </td>
          <td class="text-muted">{task.status}</td>
          <td>
            {#if task.priority !== 'none'}
              <span class="inline-flex items-center gap-1.5 text-muted">
                <span class="size-1.5 rounded-full" style="background: var(--priority-{task.priority})"></span>
                {priorityLabel[task.priority]}
              </span>
            {/if}
          </td>
          <td class="tabular {task.overdue ? 'font-medium text-danger' : 'text-muted'}">{task.due}</td>
          <td class="tabular pr-4 text-right text-muted">{task.estimate}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  table {
    table-layout: fixed;
  }
  .row {
    transition: background-color 100ms var(--ease-out);
  }
  .row:hover {
    background: var(--hover);
  }
  .done .title {
    color: var(--text-subtle);
    text-decoration: line-through;
    text-decoration-color: var(--border-strong);
  }
</style>
