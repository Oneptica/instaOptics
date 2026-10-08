import { useState } from 'react'
import { formatShort } from '../format'
import { parseNumber } from '../lensEdit'

/** A text input that commits a number on Enter or blur and reverts on Escape or invalid input. */
export function NumberField({ value, onCommit, min, allowInfinity, optional, ariaLabel, placeholder }: {
  value: number | null; onCommit: (value: number | null) => void; min?: number; allowInfinity?: boolean
  /** Blank text commits null ("not set"); otherwise null displays as Infinity. */
  optional?: boolean
  ariaLabel: string; placeholder?: string
}) {
  const shown = value === null ? (optional ? '' : 'Infinity') : formatShort(value)
  const [text, setText] = useState(shown)
  const [synced, setSynced] = useState(shown)
  if (synced !== shown) { setSynced(shown); setText(shown) }
  const commit = () => {
    if (optional && text.trim() === '') { if (value !== null) onCommit(null); return }
    const parsed = parseNumber(text)
    const valid = parsed !== null && (parsed !== Infinity || allowInfinity) && (min === undefined || parsed > min)
    if (!valid) { setText(shown); return }
    onCommit(parsed === Infinity ? null : parsed)
  }
  return (
    <input
      className="field-input mono"
      aria-label={ariaLabel}
      value={text}
      placeholder={placeholder}
      spellCheck={false}
      onChange={event => setText(event.target.value)}
      onBlur={commit}
      onKeyDown={event => {
        if (event.key === 'Enter') event.currentTarget.blur()
        if (event.key === 'Escape') { setText(shown); event.currentTarget.blur() }
      }}
    />
  )
}
