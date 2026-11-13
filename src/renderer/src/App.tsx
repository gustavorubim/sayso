import { useCallback, useEffect, useRef, useState } from 'react'
import type { DownloadProgress, HistoryItem, ModelInfo, Settings, Status, UpdateState } from '../../shared/types'
import { errorText } from './format'
import { DictionaryPage } from './pages/Dictionary'
import { HistoryPage } from './pages/History'
import { HomePage } from './pages/Home'
import { ModelsPage } from './pages/Models'
import { SettingsPage } from './pages/SettingsPage'

type Page = 'home' | 'models' | 'dictionary' | 'history' | 'settings'

const NAV: { id: Page; label: string }[] = [
  { id: 'home', label: 'Dictate' },
  { id: 'models', label: 'Speech models' },
  { id: 'dictionary', label: 'Dictionary' },
  { id: 'history', label: 'History' },
  { id: 'settings', label: 'Settings' }
]

export interface AppState {
  settings: Settings
  models: ModelInfo[]
  history: HistoryItem[]
  status: Status
  progress: Record<string, DownloadProgress>
  engine: 'loading' | 'ready' | 'failed' | null
  appUpdate: UpdateState
  /** Saves a settings change. Shows a notice and returns false if it is rejected. */
  update: (patch: Partial<Settings>) => Promise<boolean>
  refreshModels: () => Promise<void>
  notify: (message: string) => void
  go: (page: Page) => void
}

const RELEASES_URL = 'https://github.com/gustavorubim/shortcut-to-text/releases/latest'

function UpdateNotice({ update }: { update: UpdateState }): React.JSX.Element | null {
  if (update.state === 'none' || !update.version) return null
  return (
    <div className="update" role="status">
      {update.state === 'ready' && (
        <>
          <p>Version {update.version} is ready to install.</p>
          <button type="button" className="btn btn-primary" onClick={() => window.stt.installUpdate()}>
            Restart to update
          </button>
        </>
      )}
      {update.state === 'downloading' && <p>Downloading version {update.version}…</p>}
      {update.state === 'available' && (
        <>
          <p>Version {update.version} is available.</p>
          <button type="button" className="btn btn-primary" onClick={() => window.stt.openExternal(RELEASES_URL)}>
            Get the update
          </button>
        </>
      )}
      {update.state === 'error' && (
        <>
          <p>Version {update.version} could not be downloaded.</p>
          <button type="button" className="btn" onClick={() => window.stt.openExternal(RELEASES_URL)}>
            Download it by hand
          </button>
        </>
      )}
    </div>
  )
}

export function App(): React.JSX.Element {
  const [page, setPage] = useState<Page>('home')
  const [settings, setSettings] = useState<Settings | null>(null)
  const [models, setModels] = useState<ModelInfo[]>([])
  const [history, setHistory] = useState<HistoryItem[]>([])
  const [status, setStatus] = useState<Status>({ phase: 'idle' })
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({})
  const [engine, setEngine] = useState<AppState['engine']>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [version, setVersion] = useState('')
  const [appUpdate, setAppUpdate] = useState<UpdateState>({ state: 'none' })
  const noticeTimer = useRef<number | undefined>(undefined)
  const settingsRef = useRef<Settings | null>(null)
  settingsRef.current = settings

  const notify = useCallback((message: string) => {
    setNotice(message)
    window.clearTimeout(noticeTimer.current)
    noticeTimer.current = window.setTimeout(() => setNotice(null), 6000)
  }, [])

  const refreshModels = useCallback(async () => {
    setModels(await window.stt.call('list_models'))
  }, [])
  const refreshHistory = useCallback(async () => {
    setHistory(await window.stt.call('get_history'))
  }, [])

  useEffect(() => {
    const load = async (): Promise<void> => {
      try {
        setSettings(await window.stt.call('get_settings'))
        await Promise.all([refreshModels(), refreshHistory()])
        setStatus(await window.stt.call('get_status'))
        setVersion((await window.stt.info()).version)
        setAppUpdate(await window.stt.updateState())
      } catch (err) {
        notify(errorText(err))
      }
    }
    void load()

    const offs = [
      window.stt.on('status', (s) => {
        setStatus(s)
        if (s.phase === 'error' && s.message) notify(s.message)
      }),
      window.stt.on('settings-changed', setSettings),
      window.stt.on('history-changed', () => void refreshHistory()),
      window.stt.on('hotkey-error', notify),
      window.stt.on('engine', setEngine),
      window.stt.on('update', setAppUpdate),
      window.stt.on('core-exit', (message) => {
        notify(message)
        // The core restarts by itself; reload state once it is back.
        window.setTimeout(() => void load(), 2000)
      }),
      window.stt.on('download-progress', (p) => {
        setProgress((prev) => {
          if (!p.done) return { ...prev, [p.id]: p }
          const next = { ...prev }
          delete next[p.id]
          return next
        })
        if (p.done) {
          if (p.error && p.error !== 'Download cancelled') notify(p.error)
          void refreshModels()
        }
      })
    ]
    return () => offs.forEach((off) => off())
  }, [notify, refreshModels, refreshHistory])

  const update = useCallback(
    async (patch: Partial<Settings>): Promise<boolean> => {
      const current = settingsRef.current
      if (!current) return false
      const next = { ...current, ...patch }
      setSettings(next) // optimistic
      try {
        setSettings(await window.stt.call('save_settings', next))
        return true
      } catch (err) {
        setSettings(current)
        notify(errorText(err))
        return false
      }
    },
    [notify]
  )

  if (!settings) {
    return (
      <div className="boot">
        <p>{notice ?? 'Starting…'}</p>
      </div>
    )
  }

  const state: AppState = {
    settings,
    models,
    history,
    status,
    progress,
    engine,
    update,
    appUpdate,
    refreshModels,
    notify,
    go: setPage
  }

  return (
    <div className="shell">
      <nav className="nav" aria-label="Sections">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            <i />
            <i />
            <i />
            <i />
            <b />
          </span>
          Shortcut to Text
        </div>
        <ul>
          {NAV.map((item) => (
            <li key={item.id}>
              <button
                type="button"
                className={page === item.id ? 'nav-item is-active' : 'nav-item'}
                aria-current={page === item.id ? 'page' : undefined}
                onClick={() => setPage(item.id)}
              >
                {item.label}
              </button>
            </li>
          ))}
        </ul>
        <UpdateNotice update={appUpdate} />
        <p className="nav-foot">
          Audio never leaves this computer.
          {version && <span>Version {version}</span>}
        </p>
      </nav>

      <main className="main">
        {page === 'home' && <HomePage {...state} />}
        {page === 'models' && <ModelsPage {...state} />}
        {page === 'dictionary' && <DictionaryPage {...state} />}
        {page === 'history' && <HistoryPage {...state} />}
        {page === 'settings' && <SettingsPage {...state} />}
      </main>

      {notice && (
        <div className="notice" role="alert">
          <span>{notice}</span>
          <button type="button" onClick={() => setNotice(null)} aria-label="Dismiss">
            ×
          </button>
        </div>
      )}
    </div>
  )
}
