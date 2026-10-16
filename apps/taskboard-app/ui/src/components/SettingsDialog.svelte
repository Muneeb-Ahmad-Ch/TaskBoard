<script lang="ts">
  import { Bell, Clock, Monitor, Palette } from '@lucide/svelte'
  import type { AppSettings } from '../lib/bindings/AppSettings'
  import type { NotifySettings } from '../lib/bindings/NotifySettings'
  import { cal } from '../lib/cal.svelte'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { ACCENTS, setAccentHue, setThemePref, theme, type ThemePref } from '../lib/theme.svelte'
  import { toast } from '../lib/toast.svelte'
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  let notify = $state<NotifySettings | null>(null)
  let appSettings = $state<AppSettings | null>(null)
  let saving = $state(false)
  let weekStart = $state(cal.weekStart)

  $effect(() => {
    if (!inTauri) return
    Promise.all([ipc.notifySettings(), ipc.appSettings()])
      .then(([n, a]) => {
        notify = n
        appSettings = a
      })
      .catch((err) => toast(errorMessage(err), { kind: 'error' }))
  })

  async function save() {
    saving = true
    try {
      if (notify) await ipc.setNotifySettings(notify)
      if (appSettings) await ipc.setAppSettings(appSettings)
      cal.weekStart = weekStart
      try {
        localStorage.setItem('taskboard.weekStart', String(weekStart))
      } catch {
        // per-device convenience only
      }
      toast('Settings saved')
      onclose()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      saving = false
    }
  }

  const THEMES: { v: ThemePref; l: string }[] = [
    { v: 'system', l: 'System' },
    { v: 'light', l: 'Light' },
    { v: 'dark', l: 'Dark' },
  ]
</script>

<Modal title="Settings" {onclose} width={560}>
  <div class="flex flex-col gap-5">
    <section>
      <h3 class="sec"><Palette size={14} /> Appearance</h3>
      <div class="flex flex-wrap items-center gap-3">
        <div class="flex rounded-md border border-border bg-surface-1 p-0.5" role="radiogroup" aria-label="Theme">
          {#each THEMES as t (t.v)}
            <button role="radio" aria-checked={theme.pref === t.v} class="seg" onclick={() => setThemePref(t.v)}>{t.l}</button>
          {/each}
        </div>
        <div class="flex gap-1.5" role="radiogroup" aria-label="Accent color">
          {#each ACCENTS as a (a.hue)}
            <button role="radio" aria-checked={theme.accentHue === a.hue} aria-label={a.name} title={a.name} class="swatch" style="--s: oklch(0.6 0.17 {a.hue})" onclick={() => setAccentHue(a.hue)}></button>
          {/each}
        </div>
      </div>
    </section>

    {#if notify}
      <section>
        <h3 class="sec"><Bell size={14} /> Reminders</h3>
        <div class="flex flex-col gap-2 text-[13px]">
          <label class="row"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={notify.enabled} /> Show reminders</label>
          <div class="grid grid-cols-2 gap-2" class:opacity-50={!notify.enabled}>
            <label>
              <span class="field-label">Meetings — minutes before</span>
              <input type="number" class="field" min="0" max="1440" bind:value={notify.meeting_lead_min} disabled={!notify.enabled} />
            </label>
            <label>
              <span class="field-label">Timed tasks — minutes before</span>
              <input type="number" class="field" min="0" max="1440" bind:value={notify.task_lead_min} disabled={!notify.enabled} />
            </label>
          </div>
          <div class="flex items-center gap-2" class:opacity-50={!notify.enabled}>
            <label class="row"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={notify.digest} disabled={!notify.enabled} /> Morning summary at</label>
            <input type="time" class="field !h-7 !w-[110px]" bind:value={notify.digest_time} disabled={!notify.enabled || !notify.digest} />
          </div>
          <div>
            <button class="btn !h-7 text-xs" onclick={() => void ipc.testNotification().catch((err) => toast(errorMessage(err), { kind: 'error' }))}>Send a test notification</button>
          </div>
        </div>
      </section>
    {/if}

    {#if appSettings}
      <section>
        <h3 class="sec"><Monitor size={14} /> Windows</h3>
        <div class="flex flex-col gap-2 text-[13px]">
          <label class="row"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={appSettings.close_to_tray} /> Keep running in the tray when the window is closed <span class="text-subtle">(needed for reminders)</span></label>
          <label class="row"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={appSettings.autostart} /> Start with Windows (minimized to the tray)</label>
          <p class="text-xs text-subtle">Quick add from anywhere: Ctrl+Shift+Space.</p>
        </div>
      </section>
    {/if}

    <section>
      <h3 class="sec"><Clock size={14} /> Calendar</h3>
      <div class="flex flex-wrap items-center gap-3 text-[13px]">
        <label class="flex items-center gap-2">
          Week starts on
          <select class="field !h-7 !w-[120px]" bind:value={weekStart}>
            <option value={1}>Monday</option>
            <option value={0}>Sunday</option>
            <option value={6}>Saturday</option>
          </select>
        </label>
        <button class="btn !h-7 text-xs" onclick={() => { onclose(); cal.workingHoursOpen = true }}>Working hours &amp; days off…</button>
      </div>
    </section>
  </div>

  {#snippet footer()}
    <span class="text-xs text-subtle">Settings are stored in your local database and included in backups.</span>
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={saving} onclick={() => void save()}>Save</button>
  {/snippet}
</Modal>

<style>
  .sec {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-bottom: 8px;
    font-size: 13px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }
  .seg {
    height: 26px;
    padding-inline: 0.7rem;
    border-radius: 5px;
    color: var(--text-muted);
  }
  .seg[aria-checked='true'] {
    background: var(--surface-3);
    color: var(--text);
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 99px;
    background: var(--s);
  }
  .swatch[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface-2),
      0 0 0 4px var(--s);
  }
</style>
