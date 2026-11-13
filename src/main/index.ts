import { existsSync, mkdirSync, renameSync } from 'node:fs'
import { join } from 'node:path'
import { BrowserWindow, Menu, Tray, app, ipcMain, nativeImage, screen, shell } from 'electron'
import trayIconPath from '../../resources/tray.png?asset'
import appIconPath from '../../resources/icon.png?asset'
import type { AppInfo, Settings, Status } from '../shared/types'
import { Core } from './core'
import { loadKey, saveKey } from './secrets'
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
    title: 'Sayso',
    icon: appIconPath,
    autoHideMenuBar: true,
    backgroundColor: '#fff6ea',
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
    case 'rewriting':
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
      ? 'Sayso — listening'
      : status.phase === 'transcribing' || status.phase === 'rewriting'
        ? 'Sayso — writing'
        : 'Sayso'
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
    if (protocol === 'https:' && ['huggingface.co', 'github.com', 'openrouter.ai'].includes(hostname)) {
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
  t.setToolTip('Sayso')
  t.setContextMenu(
    Menu.buildFromTemplate([
      { label: 'Open Sayso', click: showMainWindow },
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

// Sayso was first released as "Shortcut to Text". Carry settings, history and downloaded
// models over from the old data folder the first time the renamed app starts.
function migrateOldData(): void {
  const oldDir = join(app.getPath('appData'), 'shortcut-to-text')
  const newDir = app.getPath('userData')
  if (!existsSync(oldDir) || existsSync(join(newDir, 'settings.json'))) return
  mkdirSync(newDir, { recursive: true })
  for (const name of ['settings.json', 'history.json', 'openrouter.key', 'models']) {
    try {
      if (existsSync(join(oldDir, name)) && !existsSync(join(newDir, name))) {
        renameSync(join(oldDir, name), join(newDir, name))
      }
    } catch (err) {
      console.error(`could not carry over ${name}:`, err)
    }
  }
}
if (app.isPackaged) migrateOldData()

if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', showMainWindow)

  app.whenReady().then(async () => {
    app.setAppUserModelId('com.sayso.app')

    core.on('event', (event: string, payload: unknown) => {
      if (event === 'status') onStatus(payload as Status)
      if (event === 'settings-changed') applySettings(payload as Settings)
      broadcast(event, payload)
    })
    // The core holds the API key in memory only, so hand it over on every (re)start.
    core.on('started', () => {
      const key = loadKey()
      if (key) void core.call('set_api_key', { key }).catch(() => {})
    })
    core.start()

    ipcMain.handle('rewrite:has-key', () => loadKey() !== null)
    ipcMain.handle('rewrite:set-key', async (_e, key: string) => {
      const result = saveKey(String(key ?? ''))
      await core.call('set_api_key', { key: loadKey() })
      return result
    })
    ipcMain.handle('core:call', async (_e, method: string, params: unknown) => {
      // The key goes through 'rewrite:set-key' so it is always stored encrypted.
      if (method === 'set_api_key') throw new Error('Not available.')
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
