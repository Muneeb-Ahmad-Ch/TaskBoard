// Keyboard behaviour for a group of radio buttons or tabs (ARIA radiogroup /
// tablist): one Tab stop (the chosen one), arrow keys and Home/End move to
// and pick the next choice. Use as `<div role="radiogroup" use:segmented>`.

const CHOICE = '[role="radio"], [role="tab"]'
const isChosen = (el: Element) => el.getAttribute('aria-checked') === 'true' || el.getAttribute('aria-selected') === 'true'

export function segmented(node: HTMLElement) {
  const choices = () => [...node.querySelectorAll<HTMLElement>(CHOICE)].filter((el) => !el.hasAttribute('disabled'))

  /** Only the chosen one (or the first) is a Tab stop. */
  function rove() {
    const list = choices()
    const chosen = list.find(isChosen) ?? list[0]
    for (const el of list) el.tabIndex = el === chosen ? 0 : -1
  }

  function onkeydown(e: KeyboardEvent) {
    const list = choices()
    const at = list.indexOf(document.activeElement as HTMLElement)
    if (at < 0 || !list.length) return
    let next = -1
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (at + 1) % list.length
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = (at - 1 + list.length) % list.length
    else if (e.key === 'Home') next = 0
    else if (e.key === 'End') next = list.length - 1
    if (next < 0) return
    e.preventDefault()
    e.stopPropagation()
    list[next].focus()
    list[next].click()
  }

  rove()
  // The chosen one changes from outside too (a click, a shortcut, a saved view).
  const watch = new MutationObserver(rove)
  watch.observe(node, { subtree: true, childList: true, attributes: true, attributeFilter: ['aria-checked', 'aria-selected'] })
  node.addEventListener('keydown', onkeydown)
  return {
    destroy() {
      watch.disconnect()
      node.removeEventListener('keydown', onkeydown)
    },
  }
}
