// Settings search: every setting with the words people use for it, and the
// place on its page to jump to (`data-setting` attributes).

export type SettingsPage =
  | 'general'
  | 'appearance'
  | 'sidebar'
  | 'sections'
  | 'views'
  | 'columns'
  | 'calendar'
  | 'reminders'
  | 'data'
  | 'ai'
  | 'updates'
  | 'about'

export interface SettingEntry {
  page: SettingsPage
  label: string
  /** `data-setting` value of the row or card to scroll to. */
  anchor: string
  /** Other words for it (lower case). */
  words: string
}

const S = (page: SettingsPage, label: string, anchor: string, words = ''): SettingEntry => ({ page, label, anchor, words })

export const SETTINGS: SettingEntry[] = [
  S('general', 'Start with Windows', 'autostart', 'autostart login sign in boot startup launch'),
  S('general', 'Keep running in the tray', 'tray', 'tray close minimize background notification area exit quit'),
  S('general', 'Quick add from anywhere', 'quick-add-shortcut', 'shortcut hotkey global ctrl shift space new task'),
  S('general', 'Search and commands', 'palette', 'ctrl k command palette search jump'),
  S('general', 'All keyboard shortcuts', 'shortcuts', 'keys hotkeys keyboard'),
  S('appearance', 'Theme', 'theme', 'dark light system mode night'),
  S('appearance', 'Accent color', 'accent', 'color colour highlight'),
  S('appearance', 'Zoom', 'zoom', 'scale size text size bigger smaller font magnify'),
  S('sidebar', 'Sidebar items', 'sidebar-items', 'today upcoming inbox meetings all work dashboard pinned saved views hide show order navigation'),
  S('sidebar', 'Counts and color dots', 'sidebar-details', 'count counts badge numbers overdue dot dots section project header'),
  S('sidebar', 'Sidebar width', 'sidebar-width', 'resize width narrow wide'),
  S('sections', 'Sections', 'sections', 'clients personal teams groups rename section new section delete reorder area categories'),
  S('views', 'View tabs', 'view-tabs', 'table board calendar tabs views'),
  S('views', 'Add-a-task box position', 'quick-add-position', 'quick add input top bottom new task box'),
  S('views', 'Leave out names a list already shows', 'repeated-context', 'repeated redundant repeat same company client group project name column hide'),
  S('views', 'Table columns', 'table-columns', 'columns hide show reorder priority due progress estimate tags group project added created date'),
  S('views', 'Task panel width', 'panel-width', 'panel details width resize'),
  S('columns', 'Board columns for new projects', 'columns-default', 'status statuses waiting backlog to do in progress review done cancelled workflow kanban stage'),
  S('columns', 'Add missing board columns to your projects', 'columns-apply', 'status statuses waiting apply all projects existing'),
  S('calendar', 'Week starts on', 'week-start', 'monday sunday saturday first day week'),
  S('calendar', 'Working hours and days off', 'working-hours', 'hours work schedule holidays vacation leave capacity time off days off availability'),
  S('reminders', 'Show reminders', 'reminders', 'notifications alerts notify toast windows'),
  S('reminders', 'Meeting reminders', 'meeting-reminder', 'minutes before meeting notification alert'),
  S('reminders', 'Timed task reminders', 'task-reminder', 'due time task notification alert'),
  S('reminders', 'Morning summary', 'digest', 'digest daily summary morning agenda'),
  S('reminders', 'Send a test notification', 'test-notification', 'test notification'),
  S('data', 'Back up now', 'backups', 'backup backups restore copy safe recover'),
  S('data', 'Export your data', 'export', 'export json csv ics save copy download'),
  S('data', 'Import data', 'import', 'import todoist trello notion csv ics calendar file'),
  S('data', 'Archived tasks', 'archived-tasks', 'archive archived restore hidden tasks'),
  S('data', 'Archived groups and projects', 'archived-projects', 'archive archived restore projects groups clients'),
  S('data', 'Sample workspace', 'sample', 'demo example sample'),
  S('ai', 'Let AI assistants read and change your data', 'ai-access', 'ai mcp claude codex cursor chatgpt assistant access tb'),
  S('ai', 'Connect an assistant', 'ai-connect', 'ai mcp claude codex cursor config setup connect tb'),
  S('ai', 'Check that the assistant connection works', 'ai-test', 'ai mcp test connection'),
  S('updates', 'Check for updates', 'update-check', 'update updates upgrade new version download install'),
  S('updates', 'Check for updates automatically', 'update-auto', 'update automatic auto upgrade'),
  S('updates', 'Release notes', 'release-notes', 'changelog what is new whats new releases'),
  S('about', 'About Taskboard', 'about', 'version license developer contact source code github website'),
]

const norm = (s: string) => s.toLowerCase().normalize('NFKD').replace(/[̀-ͯ]/g, '').replace(/\s+/g, ' ').trim()

/** Settings matching every word of `query`, best first: name matches above
 *  other-word matches, and earlier matches above later ones. */
export function searchSettings(query: string): SettingEntry[] {
  const words = norm(query).split(' ').filter(Boolean)
  if (!words.length) return []
  const scored: { e: SettingEntry; score: number }[] = []
  for (const e of SETTINGS) {
    const label = norm(e.label)
    const extra = norm(e.words)
    let score = 0
    let all = true
    for (const w of words) {
      const inLabel = label.indexOf(w)
      if (inLabel >= 0) score += 10 - Math.min(inLabel, 9) / 10 + (label.split(' ').includes(w) ? 2 : 0)
      else if (extra.split(' ').some((x) => x.startsWith(w)) || extra.includes(w)) score += 3
      else {
        all = false
        break
      }
    }
    if (all) scored.push({ e, score })
  }
  return scored.sort((a, b) => b.score - a.score).map((s) => s.e)
}
