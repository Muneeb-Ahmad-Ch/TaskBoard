// Release builds behave like a desktop app, not a web page: no browser
// context menu (Back, Refresh, Save as, Print, Share…) and no browser
// shortcuts that would reload, navigate away from, print or save the page.
// Taskboard's own right-click menus call preventDefault themselves and are
// unaffected. Development builds keep the browser menu for Inspect.

/** Keys the WebView would otherwise act on. */
function isBrowserShortcut(e: KeyboardEvent): boolean {
  const key = e.key.toLowerCase()
  const ctrl = e.ctrlKey || e.metaKey
  if (key === 'f5' || key === 'browserback' || key === 'browserforward' || key === 'browserrefresh') return true
  // Reload, print, save page, view source.
  if (ctrl && ['r', 'p', 's', 'u'].includes(key)) return true
  // Back / forward.
  return e.altKey && (key === 'arrowleft' || key === 'arrowright')
}

/** The edit menu (cut, copy, paste, spelling) still makes sense here. */
function wantsEditMenu(target: EventTarget | null): boolean {
  const el = target instanceof Element ? target : null
  if (el?.closest('input, textarea, [contenteditable=""], [contenteditable="true"]')) return true
  return !!window.getSelection()?.toString().trim()
}

export function installAppChrome(): void {
  window.addEventListener('contextmenu', (e) => {
    if (!wantsEditMenu(e.target)) e.preventDefault()
  })
  // Capture phase: runs before the app's own handlers, which still see the key.
  window.addEventListener(
    'keydown',
    (e) => {
      if (isBrowserShortcut(e)) e.preventDefault()
    },
    true,
  )
  // Mouse back / forward buttons.
  const noHistoryButtons = (e: MouseEvent) => {
    if (e.button === 3 || e.button === 4) e.preventDefault()
  }
  window.addEventListener('mousedown', noHistoryButtons, true)
  window.addEventListener('mouseup', noHistoryButtons, true)
  window.addEventListener('auxclick', noHistoryButtons, true)
}

export const __test = { isBrowserShortcut, wantsEditMenu }
