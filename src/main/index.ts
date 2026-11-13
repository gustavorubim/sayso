import { join } from 'node:path'
import { BrowserWindow, Menu, Tray, app, ipcMain, nativeImage, screen, shell } from 'electron'
import trayIconPath from '../../resources/tray.png?asset'
import appIconPath from '../../resources/icon.png?asset'
import type { AppInfo, Settings, Status } from '../shared/types'
import { Core } from './core'
import { checkForUpdates, getUpdateState, installUpdate, startUpdates } from './updates'

const OVERLAY_WIDTH = 380
const OVERLAY_HEIGHT = 72

const core = new Core()
let mainWindow: BrowserWindow | null = null
let overlay: BrowserWindow | null = null
let tray: Tray | null = null
let quitting = false
let settings: Settings | null = null
let overlayTimer: NodeJS.Timeout | null = null

const startHidden = process.argv.includes('--hidden')

function loadPage(win: BrowserWindow, page: 'index' | 'overlay'): void {
  const devUrl = process.env['ELECTRON_RENDERER_URL']
  if (!app.isPackaged && devUrl) void win.loadURL(`${devUrl}/${page}.html`)
  else void win.loadFile(join(__dirname, `../renderer/${page}.html`))
}

const webPreferences = {
  preload: join(__dirname, '../preload/index.js'),
  contextIsolation: true,
  nodeIntegration: false,
  sandbox: true
}

function createMainWindow(): BrowserWindow {
  const win = new BrowserWindow({
    width: 1000,
    height: 700,
    minWidth: 760,
    minHeight: 520,
    show: false,
    title: 'Shortcut to Text',
    icon: appIconPath,
    autoHideMenuBar: true,
    backgroundColor: '#eef1ec',
    webPreferences
  })
  win.once('ready-to-show', () => {
    if (!startHidden) win.show()
  })
  // Closing the window keeps the app listening in the tray.
  win.on('close', (event) => {
    if (!quitting) {
      event.preventDefault()
      win.hide()
    }
  })
  win.webContents.setWindowOpenHandler(({ url }) => {
    openExternal(url)
    return { action: 'deny' }
  })
  loadPage(win, 'index')
  return win
}

function createOverlay(): BrowserWindow {
  const win = new BrowserWindow({
    width: OVERLAY_WIDTH,
    height: OVERLAY_HEIGHT,
    show: false,
    frame: false,
    transparent: true,
    resizable: false,
    movable: false,
    focusable: false,
    skipTaskbar: true,
    alwaysOnTop: true,
    hasShadow: false,
    webPreferences
  })
  win.setAlwaysOnTop(true, 'screen-saver')
  win.setIgnoreMouseEvents(true)
  win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true })
  loadPage(win, 'overlay')
  return win
}

function showMainWindow(): void {
  if (!mainWindow) return
  if (mainWindow.isMinimized()) mainWindow.restore()
  mainWindow.show()
  mainWindow.focus()
}

function showOverlay(): void {
  if (!overlay) return
  // Bottom centre of the display the pointer is on.
  const display = screen.getDisplayNearestPoint(screen.getCursorScreenPoint())
  const { x, y, width, height } = display.workArea
  overlay.setBounds({
    x: Math.round(x + (width - OVERLAY_WIDTH) / 2),
    y: Math.round(y + height - OVERLAY_HEIGHT - 28),
    width: OVERLAY_WIDTH,
    height: OVERLAY_HEIGHT
  })
  overlay.showInactive()
}

function hideOverlayAfter(ms: number): void {
  if (overlayTimer) clearTimeout(overlayTimer)
  overlayTimer = setTimeout(() => overlay?.hide(), ms)
}

function onStatus(status: Status): void {
  if (overlayTimer) clearTimeout(overlayTimer)
  const wanted = settings?.show_overlay !== false
  switch (status.phase) {
    case 'recording':
    case 'transcribing':
      if (wanted) showOverlay()
      break
    case 'done':
      hideOverlayAfter(wanted ? 700 : 0)
      break
    case 'error':
      // Errors are always shown: otherwise a failed dictation looks like nothing happened.
      showOverlay()
      hideOverlayAfter(3200)
      break
    default:
      overlay?.hide()
  }
  tray?.setToolTip(
    status.phase === 'recording'
      ? 'Shortcut to Text — listening'
      : status.phase === 'transcribing'
        ? 'Shortcut to Text — writing'
        : 'Shortcut to Text'
  )
}

function applySettings(next: Settings): void {
  const loginChanged = settings?.launch_at_login !== next.launch_at_login
  settings = next
  if (loginChanged && app.isPackaged && process.platform !== 'linux') {
    app.setLoginItemSettings({ openAtLogin: next.launch_at_login, args: ['--hidden'] })
  }
}

function broadcast(event: string, payload: unknown): void {
  for (const win of BrowserWindow.getAllWindows()) {
    if (!win.isDestroyed()) win.webContents.send('core:event', event, payload)
  }
}

function openExternal(url: string): void {
  try {
    const { protocol, hostname } = new URL(url)
    if (protocol === 'https:' && (hostname === 'huggingface.co' || hostname === 'github.com')) {
      void shell.openExternal(url)
    }
  } catch {
    // not a URL: ignore
  }
}

function createTray(): Tray {
  let image = nativeImage.createFromPath(trayIconPath)
  if (image.isEmpty()) console.error(`tray icon could not be loaded from ${trayIconPath}`)
  if (process.platform === 'darwin') image = image.resize({ width: 18, height: 18 })
  const t = new Tray(image)
  t.setToolTip('Shortcut to Text')
  t.setContextMenu(
    Menu.buildFromTemplate([
      { label: 'Open Shortcut to Text', click: showMainWindow },
      { label: 'Start or stop dictation', click: () => void core.call('toggle_recording').catch(() => {}) },
      { type: 'separator' },
      {
        label: 'Quit',
        click: () => {
          quitting = true
          app.quit()
        }
      }
    ])
  )
  t.on('click', showMainWindow)
  return t
}

// Development runs get their own data folder, and with it their own single-instance lock,
// so `npm run dev` works while the installed app is running. (Both want the same global
// shortcut; whichever started second reports that it is taken.)
if (!app.isPackaged) {
  app.setPath('userData', `${app.getPath('userData')}-dev`)
}

if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', showMainWindow)

  app.whenReady().then(async () => {
    app.setAppUserModelId('com.shortcuttotext.app')

    core.on('event', (event: string, payload: unknown) => {
      if (event === 'status') onStatus(payload as Status)
      if (event === 'settings-changed') applySettings(payload as Settings)
      broadcast(event, payload)
    })
    core.start()

    ipcMain.handle('core:call', async (_e, method: string, params: unknown) => {
      const result = await core.call(method, params)
      if (method === 'save_settings' || method === 'get_settings') applySettings(result as Settings)
      return result
    })
    ipcMain.handle('app:info', (): AppInfo => ({ version: app.getVersion(), platform: process.platform }))
    ipcMain.on('app:open-external', (_e, url: string) => openExternal(url))
    ipcMain.handle('app:update-state', () => getUpdateState())
    ipcMain.handle('app:check-updates', () => checkForUpdates())
    ipcMain.on('app:install-update', () => {
      quitting = true
      installUpdate()
    })

    mainWindow = createMainWindow()
    overlay = createOverlay()
    tray = createTray()

    try {
      applySettings(await core.call<Settings>('get_settings'))
    } catch (err) {
      console.error('could not read settings:', err)
    }

    startUpdates((state) => broadcast('update', state))
  })

  app.on('activate', showMainWindow)
  app.on('before-quit', () => {
    quitting = true
    core.stop()
  })
  // The app lives in the tray; closing windows never quits it.
  app.on('window-all-closed', () => {})
}
