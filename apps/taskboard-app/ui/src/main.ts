import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { inTauri, ipc } from './lib/ipc'
import { initStore } from './lib/store.svelte'
import { applyTheme } from './lib/theme.svelte'
import { installAppChrome } from './lib/appChrome'

async function boot() {
  performance.mark('boot')
  if (!import.meta.env.DEV) installAppChrome()
  applyTheme()
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
