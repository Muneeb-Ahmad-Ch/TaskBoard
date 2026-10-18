<script lang="ts">
  // Settings → Updates: the installed version, checking for a newer one,
  // what's new in it, and installing it in place (download, verify, back up,
  // then Taskboard closes and reopens on the new version).
  import { AlertTriangle, CheckCircle2, Download, ExternalLink, LoaderCircle, RefreshCw } from '@lucide/svelte'
  import { appVersion, CHANGELOG_URL, RELEASES_URL } from '../lib/about'
  import { inTauri, ipc, openLink } from '../lib/ipc'
  import { checkForUpdates, installUpdate, megabytes, setAutoCheck, skipVersion, updateBusy, updates } from '../lib/updater.svelte'
  import Switch from './Switch.svelte'

  let version = $state('')
  void appVersion().then((v) => (version = v))

  /** Release notes, rendered by the app's Markdown sanitizer. */
  let notesHtml = $state('')
  $effect(() => {
    const notes = updates.info?.notes
    notesHtml = ''
    if (!notes || !inTauri) return
    let stale = false
    ipc
      .renderMarkdown(notes)
      .then((html) => {
        if (!stale) notesHtml = html
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  // Links in the notes open in the browser, not inside the app.
  function onNotesClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest('a')
    if (!a) return
    e.preventDefault()
    const href = a.getAttribute('href')
    if (href && /^https?:\/\//.test(href)) void openLink(href)
  }

  const ago = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
  function since(ms: number): string {
    const minutes = Math.round((Date.now() - ms) / 60_000)
    if (minutes < 1) return 'just now'
    if (minutes < 60) return ago.format(-minutes, 'minute')
    const hours = Math.round(minutes / 60)
    if (hours < 24) return ago.format(-hours, 'hour')
    return ago.format(-Math.round(hours / 24), 'day')
  }
  const published = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' })

  const pct = $derived(updates.total ? Math.min(100, (updates.downloaded / updates.total) * 100) : null)
  const skipped = $derived(!!updates.info && updates.skipped === updates.info.version)
</script>

<section class="card">
  <div class="flex items-start gap-3 p-4">
    <span class="mt-0.5 shrink-0" aria-hidden="true">
      {#if updates.phase === 'checking' || updates.phase === 'downloading' || updates.phase === 'installing'}
        <LoaderCircle size={20} class="spin text-accent" />
      {:else if updates.phase === 'available'}
        <Download size={20} class="text-accent" />
      {:else if updates.phase === 'error'}
        <AlertTriangle size={20} class="text-danger" />
      {:else}
        <CheckCircle2 size={20} class={updates.phase === 'none' ? 'text-success' : 'text-subtle'} />
      {/if}
    </span>
    <div id="update-status" class="min-w-0 flex-1" role="status" aria-live="polite">
      <div class="text-[13px] font-semibold">Taskboard {version || '…'}</div>
      <div class="mt-0.5 text-xs text-muted">
        {#if updates.phase === 'checking'}
          Checking for updates…
        {:else if updates.phase === 'available' && updates.info}
          Version {updates.info.version} is available{updates.info.date ? `, released ${published.format(updates.info.date)}` : ''}.{skipped ? ' You chose to skip it.' : ''}
        {:else if updates.phase === 'downloading'}
          Downloading{updates.total ? ` ${megabytes(updates.downloaded)} of ${megabytes(updates.total)}` : ` ${megabytes(updates.downloaded)}`}…
        {:else if updates.phase === 'installing'}
          Backing up your data and starting the installer. Taskboard will close and reopen.
        {:else if updates.phase === 'error'}
          <span class="text-danger">{updates.error}</span>
        {:else if updates.phase === 'none'}
          You're up to date{updates.checkedAt ? ` · checked ${since(updates.checkedAt)}` : ''}.
        {:else}
          {updates.checkedAt ? `Last checked ${since(updates.checkedAt)}.` : 'Not checked yet.'}
        {/if}
      </div>
      {#if updates.phase === 'downloading'}
        <div class="bar mt-2" role="progressbar" aria-label="Download progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pct === null ? undefined : Math.round(pct)}>
          <span class:indeterminate={pct === null} style={pct === null ? '' : `width: ${pct}%`}></span>
        </div>
      {/if}
    </div>
  </div>
  <div class="flex flex-wrap items-center gap-1.5 border-t border-border px-4 py-3">
    {#if updates.phase === 'available' || (updates.phase === 'error' && updates.info)}
      <button class="btn btn-primary !h-7 text-xs" disabled={updateBusy()} onclick={() => void installUpdate()}><Download size={13} /> Update now</button>
      {#if !skipped && updates.phase === 'available'}
        <button class="btn !h-7 text-xs" onclick={skipVersion}>Skip this version</button>
      {/if}
    {/if}
    <button class="btn !h-7 text-xs" disabled={updateBusy() || !inTauri} onclick={() => void checkForUpdates()}><RefreshCw size={13} /> Check for updates</button>
  </div>
  {#if updates.phase === 'available' || updates.phase === 'downloading'}
    <p class="border-t border-border px-4 py-2.5 text-xs text-muted">
      Your data is backed up first. Taskboard closes for a moment, installs, and reopens on the new version. Connected AI assistants may need to reconnect.
    </p>
  {/if}
</section>

{#if updates.info && notesHtml}
  <section class="card">
    <h4 class="card-title">What's new in {updates.info.version}</h4>
    <!-- Sanitized by the app's Markdown renderer (ammonia). -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="notes px-4 pb-3" onclick={onNotesClick}>{@html notesHtml}</div>
  </section>
{/if}

<section class="card">
  <div class="set-row">
    <div class="min-w-0 flex-1">
      <div class="text-[13px] font-medium">Check for updates automatically</div>
      <div class="mt-0.5 text-xs text-muted">At startup and every few hours. Only the latest version number is fetched from GitHub; nothing about your data is sent.</div>
    </div>
    <Switch label="Check for updates automatically" checked={updates.auto} onchange={setAutoCheck} />
  </div>
  <div class="set-row">
    <div class="min-w-0 flex-1">
      <div class="text-[13px] font-medium">Release notes</div>
      <div class="mt-0.5 text-xs text-muted">Every version and what changed.</div>
    </div>
    <div class="flex flex-wrap justify-end gap-1.5">
      <button class="btn !h-7 text-xs" onclick={() => void openLink(CHANGELOG_URL)}><ExternalLink size={13} /> Changelog</button>
      <button class="btn !h-7 text-xs" onclick={() => void openLink(RELEASES_URL)}>All releases</button>
    </div>
  </div>
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
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    min-height: 56px;
    padding: 10px 16px;
  }
  .set-row + .set-row {
    border-top: 1px solid var(--border);
  }
  .bar {
    height: 6px;
    border-radius: 99px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: 99px;
    background: var(--accent);
    transition: width 120ms linear;
  }
  .bar span.indeterminate {
    width: 35%;
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(300%);
    }
  }
  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .notes {
    max-height: 220px;
    overflow-y: auto;
    font-size: 13px;
    line-height: 1.5;
  }
  .notes :global(h1),
  .notes :global(h2),
  .notes :global(h3) {
    margin: 8px 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  .notes :global(ul) {
    padding-left: 18px;
    list-style: disc;
  }
  .notes :global(li) {
    margin: 2px 0;
  }
  .notes :global(a) {
    color: var(--accent-soft-text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .notes :global(code) {
    padding: 0 3px;
    border-radius: 3px;
    background: var(--surface-3);
    font-size: 12px;
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.spin),
    .bar span.indeterminate {
      animation: none;
    }
  }
</style>
