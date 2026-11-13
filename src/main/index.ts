import { existsSync, mkdirSync, renameSync } from 'node:fs'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import {
  BrowserWindow,
  type IpcMainEvent,
  type IpcMainInvokeEvent,
  Menu,
  Tray,
  app,
  ipcMain,
  nativeImage,
  screen,
  session,
  shell
} from 'electron'
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

// Where the app's own pages live. Anything else is not allowed to load in a window or to
// call into the main process.
const devUrl = !app.isPackaged ? process.env['ELECTRON_RENDERER_URL'] : undefined
const APP_ORIGIN = devUrl ? `${devUrl}/` : pathToFileURL(join(__dirname, '../renderer/')).href

function isAppPage(url: string | undefined): boolean {
  return typeof url === 'string' && url.startsWith(APP_ORIGIN)
}

/** Refuses IPC that does not come from one of the app's own pages. */
function fromApp(event: IpcMainEvent | IpcMainInvokeEvent): boolean {
  return isAppPage(event.senderFrame?.url)
}

// The only core methods the interface may call. Everything else the core can do (start a
// recording, read a file from disk, receive the API key) is reachable from the main process
// only, so a page that somehow ran foreign script could not use it.
const RENDERER_METHODS = new Set([
  'get_settings',
  'save_settings',
  'get_default_settings',
  'get_status',
  'list_models',
  'download_model',
  'cancel_download',
  'delete_model',
  'list_input_devices',
  'get_history',
  'delete_history_item',
  'clear_history',
  'copy_text',
  'list_rewrite_models',
  'rewrite_text'
])

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
      {
        label: 'Start or stop transcribing computer audio',
        click: () => void core.call('toggle_capture').catch(() => {})
      },
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

  // Every window, present and future: never navigate away from the app's own pages (a
  // dropped file or link would otherwise load with the app's privileges), and never open
  // new windows. Links go to the system browser through the allowlist in openExternal.
  app.on('web-contents-created', (_event, contents) => {
    contents.on('will-navigate', (event, url) => {
      if (!isAppPage(url)) event.preventDefault()
    })
    contents.setWindowOpenHandler(({ url }) => {
      openExternal(url)
      return { action: 'deny' }
    })
  })

  app.whenReady().then(async () => {
    // The interface needs no browser permissions: the microphone is opened by the core.
    session.defaultSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false))

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

    ipcMain.handle('rewrite:has-key', (e) => fromApp(e) && loadKey() !== null)
    ipcMain.handle('rewrite:set-key', async (e, key: string) => {
      if (!fromApp(e)) throw new Error('Not available.')
      const result = saveKey(String(key ?? ''))
      await core.call('set_api_key', { key: loadKey() })
      return result
    })
    ipcMain.handle('core:call', async (e, method: string, params: unknown) => {
      if (!fromApp(e) || !RENDERER_METHODS.has(method)) throw new Error('Not available.')
      const result = await core.call(method, params)
      if (method === 'save_settings' || method === 'get_settings') applySettings(result as Settings)
      return result
    })
    ipcMain.handle('app:info', (): AppInfo => ({ version: app.getVersion(), platform: process.platform }))
    ipcMain.on('app:open-external', (e, url: string) => {
      if (fromApp(e)) openExternal(url)
    })
    ipcMain.handle('app:update-state', () => getUpdateState())
    ipcMain.handle('app:check-updates', (e) => (fromApp(e) ? checkForUpdates() : getUpdateState()))
    ipcMain.on('app:install-update', (e) => {
      // Only when an update is actually waiting: otherwise the app would be left in a
      // state where closing the window quits it.
      if (!fromApp(e) || getUpdateState().state !== 'ready') return
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
