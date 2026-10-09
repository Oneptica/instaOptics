import { describe, expect, it } from 'vitest'
import type { LensSystem } from '../../../shared/lens'
import { documentReducer, initialDocument, isDirty, parseDocument, serializeDocument } from '../document'

const system = (name: string): LensSystem => ({
  name, objectDistance: null, stopIndex: 0, entrancePupilDiameter: 10, fields: [0], wavelengths: [0.55], primaryWavelength: 0, rayAiming: false,
  surfaces: [{ radius: 50, thickness: 5, material: 'N-BK7' }, { radius: 0, thickness: 40, material: 'AIR' }],
})

describe('document history', () => {
  it('edits, undoes and redoes', () => {
    let state = initialDocument(system('a'))
    state = documentReducer(state, { type: 'edit', update: s => ({ ...s, name: 'b' }) })
    expect(state.system.name).toBe('b')
    expect(isDirty(state)).toBe(true)
    state = documentReducer(state, { type: 'undo' })
    expect(state.system.name).toBe('a')
    expect(isDirty(state)).toBe(false)
    state = documentReducer(state, { type: 'redo' })
    expect(state.system.name).toBe('b')
  })

  it('ignores edits that change nothing and clears redo on a new edit', () => {
    let state = initialDocument(system('a'))
    expect(documentReducer(state, { type: 'edit', update: s => s })).toBe(state)
    state = documentReducer(state, { type: 'edit', update: s => ({ ...s, name: 'b' }) })
    state = documentReducer(state, { type: 'undo' })
    state = documentReducer(state, { type: 'edit', update: s => ({ ...s, name: 'c' }) })
    expect(state.future).toHaveLength(0)
  })

  it('limits the history to 200 steps', () => {
    let state = initialDocument(system('0'))
    for (let i = 1; i <= 250; i++) state = documentReducer(state, { type: 'edit', update: s => ({ ...s, name: String(i) }) })
    expect(state.past).toHaveLength(200)
  })

  it('records an undo point for previewed optimizer results', () => {
    const start = initialDocument(system('a'))
    let state = documentReducer(start, { type: 'preview', system: system('previewed') })
    expect(state.past).toHaveLength(0)
    state = documentReducer(state, { type: 'commit', base: start.system })
    expect(state.past).toEqual([start.system])
    expect(documentReducer(state, { type: 'undo' }).system).toBe(start.system)
  })

  it('marks the saved state', () => {
    let state = documentReducer(initialDocument(system('a')), { type: 'edit', update: s => ({ ...s, name: 'b' }) })
    state = documentReducer(state, { type: 'saved', path: '/tmp/x.json' })
    expect(isDirty(state)).toBe(false)
    expect(state.path).toBe('/tmp/x.json')
  })
})

describe('document files', () => {
  it('round-trips and also accepts a bare system', () => {
    const original = system('a')
    expect(parseDocument(serializeDocument(original))).toEqual(original)
    expect(parseDocument(JSON.stringify(original))).toEqual(original)
  })
})
