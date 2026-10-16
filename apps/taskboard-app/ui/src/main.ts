import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { inTauri, ipc } from './lib/ipc'
import { applyTheme } from './lib/theme.svelte'

async function boot() {
  applyTheme()
  if (inTauri) {
    const mica = await ipc.hasMica().catch(() => false)
    document.documentElement.dataset.mica = String(mica)
  }

  const target = document.getElementById('app')
  if (!target) throw new Error('missing #app root')
  mount(App, { target })

  // Show the hidden window only after the first themed frame has painted.
  if (inTauri) {
    requestAnimationFrame(() => requestAnimationFrame(() => void ipc.showMainWindow()))
  }
}

void boot()
