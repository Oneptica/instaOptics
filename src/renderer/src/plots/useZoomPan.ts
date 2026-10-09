import { useEffect, useRef, useState, type PointerEvent } from 'react'

export interface ZoomPan { k: number; ox: number; oy: number }
const HOME: ZoomPan = { k: 1, ox: 0, oy: 0 }

/**
 * Wheel zoom about the cursor, drag to pan and double-click to reset, for a plot drawn around its centre.
 * A point at centre-relative pixel (x, y) is drawn at (x·k + ox, y·k + oy).
 */
export function useZoomPan<T extends SVGSVGElement>() {
  const ref = useRef<T>(null)
  const [view, setView] = useState<ZoomPan>(HOME)
  const current = useRef(view)
  current.current = view
  const drag = useRef<{ x: number; y: number; view: ZoomPan } | null>(null)

  useEffect(() => {
    const element = ref.current
    if (!element) return
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      const rect = element.getBoundingClientRect()
      const px = event.clientX - rect.left - rect.width / 2, py = event.clientY - rect.top - rect.height / 2
      const { k, ox, oy } = current.current
      const next = Math.max(1, Math.min(200, k * Math.exp(-Math.max(-200, Math.min(200, event.deltaY)) * 0.0015)))
      setView(next === 1 ? HOME : { k: next, ox: px - (px - ox) / k * next, oy: py - (py - oy) / k * next })
    }
    element.addEventListener('wheel', onWheel, { passive: false })
    return () => element.removeEventListener('wheel', onWheel)
  }, [])

  const handlers = {
    onPointerDown(event: PointerEvent<T>) {
      if (event.button !== 0) return
      event.currentTarget.setPointerCapture(event.pointerId)
      drag.current = { x: event.clientX, y: event.clientY, view: current.current }
    },
    onPointerMove(event: PointerEvent<T>) {
      const start = drag.current
      if (start) setView({ ...start.view, ox: start.view.ox + event.clientX - start.x, oy: start.view.oy + event.clientY - start.y })
    },
    onPointerUp() { drag.current = null },
    onDoubleClick() { setView(HOME) },
  }
  return { ref, view, handlers, zoomed: view.k !== 1, reset: () => setView(HOME) }
}
