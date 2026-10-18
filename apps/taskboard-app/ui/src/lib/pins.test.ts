import { describe, expect, it } from 'vitest'
import { pinsToMigrate } from './pins'

describe('moving pins saved by older versions into the database', () => {
  const projects = new Set(['a', 'b', 'c'])
  it('adds them after the pins already there, without repeats', () => {
    expect(pinsToMigrate(['b', 'a'], ['a'], projects)).toEqual(['a', 'b'])
  })
  it('skips projects this database does not have', () => {
    expect(pinsToMigrate(['x', 'c'], [], projects)).toEqual(['c'])
  })
  it('leaves pins from another database alone', () => {
    expect(pinsToMigrate(['x', 'y'], ['a'], projects)).toBeNull()
  })
  it('has nothing to do without old pins', () => {
    expect(pinsToMigrate([], ['a'], projects)).toBeNull()
  })
})
