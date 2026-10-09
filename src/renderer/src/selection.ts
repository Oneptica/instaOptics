// The surface selected in the Lens Data editor, shared with windows that act on it (e.g. the glass catalog).
import { useSyncExternalStore } from 'react'

let selected: number | null = null
const listeners = new Set<() => void>()

export function setSelectedSurface(index: number | null) {
  if (index === selected) return
  selected = index
  listeners.forEach(listener => listener())
}

export const useSelectedSurface = () => useSyncExternalStore(callback => { listeners.add(callback); return () => { listeners.delete(callback) } }, () => selected)
