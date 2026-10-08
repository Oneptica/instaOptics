/** Fixed-decimal display used in editors and reports; non-finite values read "Infinity". */
export function formatFixed(value: number | null | undefined, digits = 4): string {
  if (value === null || value === undefined) return 'Infinity'
  if (!Number.isFinite(value)) return value < 0 ? '-Infinity' : 'Infinity'
  const text = value.toFixed(digits)
  return text === `-${(0).toFixed(digits)}` ? (0).toFixed(digits) : text
}

/** Shortest readable form for values the user typed (fields, wavelengths). */
export function formatShort(value: number): string {
  return Number.isFinite(value) ? String(Number(value.toPrecision(10))) : 'Infinity'
}

export const fieldColor = (index: number) => `var(--field-${(index % 6) + 1})`

/** A field value with its unit: degrees for angle fields, mm for object or image heights. */
export function fieldLabel(value: number, fieldType: string | undefined): string {
  return fieldType && fieldType !== 'angle' ? `${formatShort(value)} mm` : `${formatShort(value)}°`
}
