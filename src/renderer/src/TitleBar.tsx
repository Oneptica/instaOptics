import { useEffect, useRef, useState } from 'react'
import logo from './assets/logo.png'

type MenuData = Awaited<ReturnType<typeof window.instaOptics.menu.get>>

const api = window.instaOptics
const mac = api.platform === 'darwin'

const shortcut = (accelerator?: string) =>
  accelerator?.replace(/CmdOrCtrl|CommandOrControl/g, mac ? '⌘' : 'Ctrl').replace(/\+/g, mac ? '' : '+') ?? ''

/** Single title bar: app menus on Windows and Linux (macOS keeps its system menu bar), and the document title. */
export function TitleBar({ title }: { title: string }) {
  const [menus, setMenus] = useState<MenuData>([])
  const [open, setOpen] = useState<number | null>(null)
  const barRef = useRef<HTMLDivElement>(null)

  const load = () => { if (!mac) void api.menu.get().then(setMenus) }
  useEffect(load, [])

  useEffect(() => {
    if (open === null) return
    const close = (event: MouseEvent) => { if (!barRef.current?.contains(event.target as Node)) setOpen(null) }
    const key = (event: KeyboardEvent) => { if (event.key === 'Escape') setOpen(null) }
    window.addEventListener('mousedown', close)
    window.addEventListener('keydown', key)
    return () => { window.removeEventListener('mousedown', close); window.removeEventListener('keydown', key) }
  }, [open])

  return (
    <header className={`title-bar${mac ? ' mac' : ''}`} ref={barRef}>
      {!mac && <img src={logo} alt="" className="title-logo" />}
      {!mac && (
        <nav className="menu-bar">
          {menus.map((menu, i) => (
            <div key={menu.label} className="menu-root">
              <button
                className={`menu-title${open === i ? ' open' : ''}`}
                onMouseDown={() => { if (open !== i) load(); setOpen(open === i ? null : i) }}
                onMouseEnter={() => { if (open !== null && open !== i) setOpen(i) }}
              >{menu.label.replace('&', '')}</button>
              {open === i && (
                <div className="menu-dropdown">
                  {menu.items.map(item => item.type === 'separator'
                    ? <div key={item.path.join('.')} className="menu-separator" />
                    : (
                      <button
                        key={item.path.join('.')}
                        className="menu-item"
                        disabled={!item.enabled}
                        onClick={() => { setOpen(null); api.menu.invoke(item.path) }}
                      >
                        <span className="menu-check">{(item.type === 'checkbox' || item.type === 'radio') && item.checked ? '✓' : ''}</span>
                        <span className="menu-label">{item.label}</span>
                        <span className="menu-shortcut">{shortcut(item.accelerator)}</span>
                      </button>
                    ))}
                </div>
              )}
            </div>
          ))}
        </nav>
      )}
      <div className="title-text">{title}</div>
    </header>
  )
}
