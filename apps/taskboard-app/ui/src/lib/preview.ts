// M0 ONLY: static sample data to show the shell's look. Removed in M2 when
// the UI reads real data from the core.

export interface PreviewProject {
  name: string
  hue: number
  open: number
}

export interface PreviewClient {
  name: string
  hue: number
  projects: PreviewProject[]
}

export type Priority = 'urgent' | 'high' | 'med' | 'low' | 'none'
export type Category = 'todo' | 'active' | 'done'

export interface PreviewTask {
  title: string
  client: string | null
  project: string
  hue: number
  status: string
  category: Category
  priority: Priority
  due: string
  overdue?: boolean
  estimate: string
}

export const clients: PreviewClient[] = [
  {
    name: 'Acme Corp',
    hue: 262,
    projects: [
      { name: 'Website redesign', hue: 262, open: 12 },
      { name: 'SEO audit', hue: 300, open: 4 },
    ],
  },
  {
    name: 'Northwind',
    hue: 190,
    projects: [
      { name: 'Mobile app MVP', hue: 190, open: 21 },
      { name: 'API integration', hue: 150, open: 7 },
    ],
  },
  {
    name: 'Bluebird Studio',
    hue: 45,
    projects: [{ name: 'Brand refresh', hue: 45, open: 3 }],
  },
]

export const personal: PreviewProject[] = [
  { name: 'Portfolio', hue: 10, open: 2 },
  { name: 'Admin & invoices', hue: 100, open: 5 },
]

export const tasks: PreviewTask[] = [
  { title: 'Send homepage wireframes for review', client: 'Acme Corp', project: 'Website redesign', hue: 262, status: 'Review', category: 'active', priority: 'high', due: 'Today', estimate: '1h' },
  { title: 'Fix login crash on Android 15', client: 'Northwind', project: 'Mobile app MVP', hue: 190, status: 'In Progress', category: 'active', priority: 'urgent', due: 'Yesterday', overdue: true, estimate: '2h' },
  { title: 'Map Stripe webhooks to order states', client: 'Northwind', project: 'API integration', hue: 150, status: 'To Do', category: 'todo', priority: 'med', due: 'Thu', estimate: '3h' },
  { title: 'Keyword research for service pages', client: 'Acme Corp', project: 'SEO audit', hue: 300, status: 'To Do', category: 'todo', priority: 'low', due: 'Fri', estimate: '2h 30m' },
  { title: 'Logo variations — round 2', client: 'Bluebird Studio', project: 'Brand refresh', hue: 45, status: 'In Progress', category: 'active', priority: 'med', due: 'Oct 22', estimate: '4h' },
  { title: 'Invoice October hours', client: null, project: 'Admin & invoices', hue: 100, status: 'To Do', category: 'todo', priority: 'high', due: 'Oct 31', estimate: '30m' },
  { title: 'Write case study for Northwind', client: null, project: 'Portfolio', hue: 10, status: 'Backlog', category: 'todo', priority: 'none', due: '—', estimate: '—' },
  { title: 'Set up staging environment', client: 'Acme Corp', project: 'Website redesign', hue: 262, status: 'Done', category: 'done', priority: 'med', due: 'Oct 12', estimate: '1h 30m' },
]
