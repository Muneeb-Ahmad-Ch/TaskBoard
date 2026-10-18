// Pinned projects live in the database; versions before 0.5 kept them per
// device. Those old pins move in once, but only into the database they came
// from: a different file (a demo, QA, a restored copy) leaves them alone.

/** The pin list after taking in `legacy` pins, or null when none apply here. */
export function pinsToMigrate(legacy: string[], current: string[], projectIds: Set<string>): string[] | null {
  const known = legacy.filter((id) => projectIds.has(id))
  if (!known.length) return null
  return [...current, ...known.filter((id, i) => !current.includes(id) && known.indexOf(id) === i)]
}
