import { Menu, app, type MenuItemConstructorOptions } from 'electron'
import type { MenuCommand } from '../shared/protocol'
import { WINDOWS } from '../shared/windows'

export function buildMenu(
  send: (command: MenuCommand) => void,
  newWindow: () => void,
  rendering: { softwareRendering: boolean; setSoftwareRendering: (enabled: boolean) => void; scale: number; setScale: (scale: number) => void },
  checkUpdates: () => void,
) {
  const item = (label: string, command: MenuCommand, accelerator?: string): MenuItemConstructorOptions => ({ label, accelerator, click: () => send(command) })
  const mac = process.platform === 'darwin'
  const template: MenuItemConstructorOptions[] = [
    ...(mac ? [{ role: 'appMenu' } as MenuItemConstructorOptions] : []),
    {
      label: '&File',
      submenu: [
        item('New System', 'file:new', 'CmdOrCtrl+N'),
        { label: 'New Window', accelerator: 'CmdOrCtrl+Shift+N', click: newWindow },
        { type: 'separator' },
        item('Open…', 'file:open', 'CmdOrCtrl+O'),
        { type: 'separator' },
        item('Save', 'file:save', 'CmdOrCtrl+S'),
        item('Save As…', 'file:saveAs', 'CmdOrCtrl+Shift+S'),
        { type: 'separator' },
        item('Import Zemax (.zmx)…', 'file:importZmx'),
        item('Export Zemax (.zmx)…', 'file:exportZmx'),
        { type: 'separator' },
        mac ? { role: 'close' } : { role: 'quit', label: 'Exit' },
      ],
    },
    {
      label: '&Edit',
      submenu: [
        // Undo and redo go to the renderer, which applies them to the text field being edited or to the lens.
        item('Undo', 'edit:undo', 'CmdOrCtrl+Z'),
        item('Redo', 'edit:redo', mac ? 'Cmd+Shift+Z' : 'Ctrl+Y'),
        { type: 'separator' },
        { role: 'cut' },
        { role: 'copy' },
        { role: 'paste' },
        { role: 'selectAll' },
      ],
    },
    {
      label: '&View',
      submenu: [
        item('Toggle Light/Dark Theme', 'view:theme'),
        item('Reset Window Layout', 'view:resetLayout'),
        {
          label: 'Interface Scale (restarts)',
          submenu: [0, 1, 1.25, 1.5, 1.75, 2].map(scale => ({
            label: scale ? `${scale * 100}%` : 'Automatic',
            type: 'radio' as const,
            checked: rendering.scale === scale,
            click: () => { if (rendering.scale !== scale) rendering.setScale(scale) },
          })),
        },
        {
          label: 'Software 3D Rendering (restarts)',
          type: 'checkbox',
          checked: rendering.softwareRendering,
          click: menuItem => rendering.setSoftwareRendering(menuItem.checked),
        },
        { type: 'separator' },
        { role: 'resetZoom' },
        { role: 'zoomIn' },
        { role: 'zoomOut' },
        { type: 'separator' },
        { role: 'togglefullscreen' },
        ...(app.isPackaged ? [] : [{ type: 'separator' } as MenuItemConstructorOptions, { role: 'reload' } as MenuItemConstructorOptions, { role: 'toggleDevTools' } as MenuItemConstructorOptions]),
      ],
    },
    {
      label: '&Window',
      submenu: [
        ...WINDOWS.filter(window => window.group !== 'Help').flatMap((window, i, list) => [
          ...(i > 0 && list[i - 1].group !== window.group ? [{ type: 'separator' } as MenuItemConstructorOptions] : []),
          item(window.title, `window:${window.id}`, window.accelerator),
        ]),
        ...(mac ? [{ type: 'separator' } as MenuItemConstructorOptions, { role: 'minimize' } as MenuItemConstructorOptions, { role: 'front' } as MenuItemConstructorOptions] : []),
      ],
    },
    {
      role: 'help',
      submenu: [
        item('Welcome', 'window:welcome'),
        { label: 'Check for Updates…', click: checkUpdates },
        { type: 'separator' },
        { label: `instaOptics ${app.getVersion()}`, enabled: false },
      ],
    },
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}
