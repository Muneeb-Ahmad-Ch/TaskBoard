import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { errorMessage, inTauri, ipc } from './lib/ipc'
import { initStore } from './lib/store.svelte'
import { applyTheme } from './lib/theme.svelte'
import { installAppChrome } from './lib/appChrome'
import { appVersion, openChangelog } from './lib/about'
import { toast } from './lib/toast.svelte'
import { startUpdateChecks } from './lib/updater.svelte'
import { installZoom } from './lib/zoom.svelte'

/**
 * Anything that slips past the code's own error handling: logged for the
 * troubleshooting log, and shown once in a while instead of failing silently.
 */
function installErrorReporting() {
  let shownAt = 0
  const report = (reason: unknown, detail?: string) => {
    const message = reason instanceof Error ? reason.message : errorMessage(reason)
    // Chromium reports this benign layout notice as an error; nothing is wrong.
    if (/ResizeObserver loop/.test(message)) return
    if (inTauri) void ipc.logUiError(message, detail ?? (reason instanceof Error ? reason.stack : undefined)).catch(() => {})
    if (Date.now() - shownAt < 5_000) return
    shownAt = Date.now()
    // Errors from the app's own commands are already in plain language.
    const known = typeof reason === 'object' && reason !== null && 'code' in reason && 'message' in reason
    toast(known ? message : 'Something went wrong. It was logged, and your data is safe.', { kind: 'error' })
  }
  window.addEventListener('error', (e) => report(e.error ?? e.message))
  window.addEventListener('unhandledrejection', (e) => report(e.reason))
}

async function boot() {
  performance.mark('boot')
  if (!import.meta.env.DEV) installAppChrome()
  installErrorReporting()
  applyTheme()
  await installZoom().catch(() => {})
  if (inTauri) {
    const mica = await ipc.hasMica().catch(() => false)
    document.documentElement.dataset.mica = String(mica)
  }

  // Load sidebar data before the first paint so the window opens complete.
  await initStore()
  performance.mark('store-ready')

  const target = document.getElementById('app')
  if (!target) throw new Error('missing #app root')
  mount(App, { target })
  performance.mark('mounted')

  if (inTauri) {
    startUpdateChecks()
    // First launch after an in-app update: say so, with a way to see what changed.
    const from = await ipc.updatedFrom().catch(() => null)
    if (from) toast(`Updated to Taskboard ${await appVersion()}`, { action: { label: "What's new", run: openChangelog } })
  }
}

async function reveal() {
  // Show the hidden window only after the first themed frame has painted.
  // Started by Windows login with --minimized: stay in the tray.
  if (inTauri && !(await ipc.startHidden().catch(() => false))) {
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        performance.mark('reveal')
        void ipc.showMainWindow()
      }),
    )
  }
}

// Reveal the window even if boot fails, so an error is never an invisible app.
boot()
  .catch((err) => {
    console.error(err)
    document.body.textContent = `Taskboard failed to start: ${err instanceof Error ? err.message : String(err)}`
  })
  .finally(() => void reveal())
