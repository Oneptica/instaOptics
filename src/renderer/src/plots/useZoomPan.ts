import { useEffect, useRef, useState, type PointerEvent, type RefObject } from 'react'

export interface ZoomPan { k: number; ox: number; oy: number }
const HOME: ZoomPan = { k: 1, ox: 0, oy: 0 }

/**
 * Wheel zoom about the cursor, drag to pan and double-click to reset, for a plot drawn around its centre.
 * A point at centre-relative pixel (x, y) is drawn at (x·k + ox, y·k + oy).
 */
export function useZoomPan<T extends HTMLElement | SVGElement>(external?: RefObject<T | null>) {
  const own = useRef<T>(null)
  const ref = external ?? own
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
    element.addEventListener('wheel', onWheel as EventListener, { passive: false })
    return () => element.removeEventListener('wheel', onWheel as EventListener)
  }, [ref])

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

export interface Box { h: [number, number]; v: [number, number] }

/**
 * Domain zoom for a plot with axes: the wheel zooms about the cursor (over a margin, only that axis), dragging pans and
 * double-click resets. `h` runs left to right and `v` bottom to top, in data units; the plot frame sits at
 * (left, top) with the given size inside the element.
 */
export function useDomainZoom<T extends HTMLElement>(fit: Box, frame: { left: number; top: number; width: number; height: number }, external?: RefObject<T | null>) {
  const own = useRef<T>(null)
  const ref = external ?? own
  const [view, setView] = useState<Box | null>(null)
  const shown = view ?? fit
  const latest = useRef({ shown, frame })
  latest.current = { shown, frame }
  const drag = useRef<{ x: number; y: number; box: Box } | null>(null)

  useEffect(() => {
    const element = ref.current
    if (!element) return
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      const { shown: box, frame: f } = latest.current
      const rect = element.getBoundingClientRect()
      const fx = (event.clientX - rect.left - f.left) / f.width, fy = 1 - (event.clientY - rect.top - f.top) / f.height
      const k = Math.exp(Math.max(-200, Math.min(200, event.deltaY)) * 0.0015)
      const scale = (range: [number, number], t: number): [number, number] => {
        const c = range[0] + Math.max(0, Math.min(1, t)) * (range[1] - range[0])
        return [c - (c - range[0]) * k, c + (range[1] - c) * k]
      }
      setView({ h: fx >= 0 ? scale(box.h, fx) : box.h, v: fy >= 0 ? scale(box.v, fy) : box.v })
    }
    element.addEventListener('wheel', onWheel as EventListener, { passive: false })
    return () => element.removeEventListener('wheel', onWheel as EventListener)
  }, [ref])

  const dragging = useRef(false)
  const handlers = {
    onPointerDown(event: PointerEvent<T>) {
      if (event.button !== 0) return
      dragging.current = false
      drag.current = { x: event.clientX, y: event.clientY, box: latest.current.shown }
    },
    onPointerMove(event: PointerEvent<T>) {
      const start = drag.current
      if (!start) return
      // Pointer capture starts only once the pointer has moved, so a plain click still reaches the elements underneath.
      if (!dragging.current) {
        if (Math.hypot(event.clientX - start.x, event.clientY - start.y) < 4) return
        dragging.current = true
        event.currentTarget.setPointerCapture(event.pointerId)
      }
      const f = latest.current.frame
      const dh = -(event.clientX - start.x) / f.width * (start.box.h[1] - start.box.h[0])
      const dv = (event.clientY - start.y) / f.height * (start.box.v[1] - start.box.v[0])
      setView({ h: [start.box.h[0] + dh, start.box.h[1] + dh], v: [start.box.v[0] + dv, start.box.v[1] + dv] })
    },
    onPointerUp() { drag.current = null },
    onDoubleClick() { setView(null) },
  }
  return { ref, shown, handlers, zoomed: view !== null, reset: () => setView(null) }
}
