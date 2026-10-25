// File-based data actions shared by Settings and the welcome screen.
import { open, save } from '@tauri-apps/plugin-dialog'
import { isoDate } from './format'
import { errorMessage, ipc } from './ipc'
import { toast } from './toast.svelte'
import type { TaskQuery } from './bindings/TaskQuery'

export type DataKind = 'json' | 'csv' | 'ics'

const FILTERS: Record<DataKind, { name: string; extensions: string[] }> = {
  json: { name: 'Taskboard export (JSON)', extensions: ['json'] },
  csv: { name: 'Spreadsheet (CSV)', extensions: ['csv'] },
  ics: { name: 'Calendar (ICS)', extensions: ['ics'] },
}

const NAMES: Record<DataKind, string> = { json: 'export', csv: 'tasks', ics: 'calendar' }

export async function exportTo(kind: DataKind) {
  const path = await save({
    defaultPath: `taskboard-${NAMES[kind]}-${isoDate(new Date())}.${kind}`,
    filters: [FILTERS[kind]],
  })
  if (!path) return
  try {
    await ipc.exportData(kind, path)
    toast(`Exported to ${path.split(/[\\/]/).pop()}`)
  } catch (err) {
    toast(errorMessage(err), { kind: 'error' })
  }
}

/** The list on screen (scope, filters, sort) as a spreadsheet file. */
export async function exportList(query: TaskQuery, name: string) {
  const safe = name.replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'tasks'
  const path = await save({ defaultPath: `${safe} ${isoDate(new Date())}.csv`, filters: [FILTERS.csv] })
  if (!path) return
  try {
    // Every row of the list, not just the loaded pages.
    await ipc.exportListCsv({ ...query, offset: 0, limit: 0 }, path)
    toast(`Exported to ${path.split(/[\\/]/).pop()}`)
  } catch (err) {
    toast(errorMessage(err), { kind: 'error' })
  }
}

export async function importFrom(kind: DataKind) {
  const path = await open({ multiple: false, directory: false, filters: [FILTERS[kind]] })
  if (!path || Array.isArray(path)) return
  try {
    const r = await ipc.importData(kind, path)
    const extra = r.projects_created ? `, ${r.projects_created} new project${r.projects_created === 1 ? '' : 's'}` : ''
    const warn = r.warnings.length ? ` · ${r.warnings.length} warning${r.warnings.length === 1 ? '' : 's'}` : ''
    toast(`Imported ${r.created}, skipped ${r.skipped}${extra}${warn}`, {
      action: { label: 'Undo', run: () => void ipc.undo() },
    })
    if (r.warnings.length) console.warn('Import warnings:', r.warnings)
  } catch (err) {
    toast(errorMessage(err), { kind: 'error' })
  }
}
