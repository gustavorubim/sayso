import { app } from 'electron'
import { autoUpdater } from 'electron-updater'
import type { UpdateState } from '../shared/types'

const CHECK_EVERY_MS = 6 * 60 * 60 * 1000

/**
 * Whether this build can replace itself. Windows installers and Linux AppImages can.
 * macOS needs a signed build, and a .deb is upgraded by the package manager, so on those
 * we only announce the new version and link to the download.
 */
const canSelfUpdate = process.platform === 'win32' || (process.platform === 'linux' && Boolean(process.env.APPIMAGE))

let state: UpdateState = { state: 'none' }
let onChange: (state: UpdateState) => void = () => {}

function set(next: UpdateState): void {
  state = next
  onChange(next)
}

export function getUpdateState(): UpdateState {
  return state
}

export async function checkForUpdates(): Promise<UpdateState> {
  if (!app.isPackaged) {
    return { state: 'none', message: 'Updates are only checked in the installed app.' }
  }
  // Don't restart a download that is under way or already finished.
  if (state.state === 'downloading' || state.state === 'ready') return state
  try {
    await autoUpdater.checkForUpdates()
    return state
  } catch (err) {
    console.error('update check failed:', err)
    return { state: 'error', message: 'Could not check for updates. Check your internet connection.' }
  }
}

export function installUpdate(): void {
  if (state.state === 'ready') autoUpdater.quitAndInstall(true, true)
}

/** Starts background update checks. `listener` is called whenever the update state changes. */
export function startUpdates(listener: (state: UpdateState) => void): void {
  onChange = listener
  if (!app.isPackaged) return

  autoUpdater.autoDownload = canSelfUpdate
  autoUpdater.autoInstallOnAppQuit = canSelfUpdate

  autoUpdater.on('update-available', (info) => {
    set({ state: canSelfUpdate ? 'downloading' : 'available', version: info.version })
  })
  autoUpdater.on('update-not-available', () => set({ state: 'none' }))
  autoUpdater.on('update-downloaded', (info) => set({ state: 'ready', version: info.version }))
  autoUpdater.on('error', (err) => {
    console.error('updater error:', err)
    // A failed background check stays quiet; a failed download is worth showing.
    if (state.state === 'downloading') {
      set({ state: 'error', version: state.version, message: 'The update could not be downloaded.' })
    }
  })

  void checkForUpdates()
  // The app sits in the tray for days, so keep checking.
  setInterval(() => void checkForUpdates(), CHECK_EVERY_MS)
}
