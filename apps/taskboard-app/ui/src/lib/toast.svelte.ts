// Small toast queue. Errors stay longer; an optional action (e.g. Undo) runs once.

export interface Toast {
  id: number
  message: string
  kind: 'info' | 'error'
  action?: { label: string; run: () => void }
}

let nextId = 1
export const toasts = $state<Toast[]>([])

export function dismiss(id: number) {
  const i = toasts.findIndex((t) => t.id === id)
  if (i >= 0) toasts.splice(i, 1)
}

export function toast(message: string, options: Partial<Omit<Toast, 'id' | 'message'>> = {}) {
  const t: Toast = { id: nextId++, message, kind: options.kind ?? 'info', action: options.action }
  toasts.push(t)
  if (toasts.length > 4) toasts.shift()
  setTimeout(() => dismiss(t.id), t.kind === 'error' ? 7000 : 4500)
}
