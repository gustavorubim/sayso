// A stand-in for the engine, for interface tests. It keeps settings, models and history in
// memory, answers the same requests the real core does, and records every call so a test can
// assert on what the interface asked for.
import type { CoreEvents, HistoryItem, ModelInfo, Settings, SttApi, UpdateState } from '../../../shared/types'

export const DEFAULT_SETTINGS: Settings = {
  shortcut: 'Ctrl+Shift+Space',
  mode: 'hold',
  model_id: null,
  language: 'auto',
  input_device: null,
  output: 'paste',
  restore_clipboard: true,
  show_overlay: true,
  launch_at_login: false,
  remove_fillers: false,
  voice_commands: true,
  trailing_space: true,
  save_history: true,
  replacements: [],
  timing: 'after',
  rewrite_enabled: false,
  rewrite_shortcut: 'Ctrl+Shift+Alt+Space',
  rewrite_model: 'anthropic/claude-haiku-5.5',
  rewrite_prompt: 'Fix typos.',
  capture_enabled: false,
  capture_shortcut: 'Ctrl+Shift+Alt+KeyH'
}

export function model(id: string, name: string, extra: Partial<ModelInfo> = {}): ModelInfo {
  return {
    id,
    name,
    family: 'Test',
    repo: `test/${id}`,
    source: `source/${id}`,
    description: `${name} description.`,
    languages: 'English',
    language_choices: [],
    auto_language: false,
    size_bytes: 300_000_000,
    recommended: false,
    installed: false,
    downloading: false,
    ...extra
  }
}

export interface Call {
  method: string
  params: unknown
}

export interface FakeCore {
  api: SttApi
  calls: Call[]
  settings: Settings
  models: ModelInfo[]
  history: HistoryItem[]
  hasKey: boolean
  update: UpdateState
  /** Makes the next `save_settings` fail with this message. */
  failNextSave: string | null
  opened: string[]
  installedUpdate: number
  /** Sends an event to the interface, as the engine would. */
  emit<E extends keyof CoreEvents>(event: E, payload: CoreEvents[E]): void
  /** The parameters of every call to `method`, in order. */
  callsTo(method: string): unknown[]
}

/** What a test may preset. Settings are merged over the defaults. */
export interface Initial {
  settings?: Partial<Settings>
  models?: readonly ModelInfo[]
  history?: readonly HistoryItem[]
  hasKey?: boolean
}

export function fakeCore(initial: Initial = {}): FakeCore {
  const listeners = new Map<string, Set<(payload: never) => void>>()
  const core: FakeCore = {
    calls: [],
    settings: { ...DEFAULT_SETTINGS, ...initial.settings },
    models: initial.models
      ? [...initial.models]
      : [
          model('fast', 'Fast Model', { recommended: true }),
          model('big', 'Big Model', { size_bytes: 1_040_000_000, language_choices: ['en', 'pt'], auto_language: true })
        ],
    history: initial.history ? [...initial.history] : [],
    hasKey: initial.hasKey ?? false,
    update: { state: 'none' },
    failNextSave: null,
    opened: [],
    installedUpdate: 0,
    emit(event, payload) {
      for (const listener of listeners.get(event) ?? []) (listener as (p: unknown) => void)(payload)
    },
    callsTo(method) {
      return core.calls.filter((c) => c.method === method).map((c) => c.params)
    },
    api: {
      call: (async (method: string, params?: unknown) => {
        core.calls.push({ method, params })
        switch (method) {
          case 'get_settings':
          case 'get_default_settings':
            return method === 'get_settings' ? core.settings : DEFAULT_SETTINGS
          case 'save_settings':
            if (core.failNextSave) {
              const message = core.failNextSave
              core.failNextSave = null
              throw new Error(`Error invoking remote method 'core:call': Error: ${message}`)
            }
            core.settings = params as Settings
            return core.settings
          case 'get_status':
            return { phase: 'idle' }
          case 'list_models':
            return core.models
          case 'download_model': {
            const { id } = params as { id: string }
            core.models = core.models.map((m) => (m.id === id ? { ...m, downloading: true } : m))
            return null
          }
          case 'delete_model': {
            const { id } = params as { id: string }
            core.models = core.models.map((m) => (m.id === id ? { ...m, installed: false } : m))
            return null
          }
          case 'list_input_devices':
            return ['Built-in Microphone', 'USB Microphone']
          case 'get_history':
            return core.history
          case 'delete_history_item': {
            const { id } = params as { id: number }
            core.history = core.history.filter((h) => h.id !== id)
            core.emit('history-changed', null)
            return null
          }
          case 'clear_history':
            core.history = []
            core.emit('history-changed', null)
            return null
          case 'list_rewrite_models':
            return ['anthropic/claude-haiku-5.5', 'openai/gpt-6-luna']
          case 'rewrite_text':
            return 'A tidied sentence.'
          default:
            return null
        }
      }) as SttApi['call'],
      on: ((event: string, listener: (payload: never) => void) => {
        if (!listeners.has(event)) listeners.set(event, new Set())
        listeners.get(event)!.add(listener)
        return () => listeners.get(event)!.delete(listener)
      }) as SttApi['on'],
      info: async () => ({ version: '9.9.9', platform: 'win32' }),
      updateState: async () => core.update,
      checkForUpdates: async () => core.update,
      installUpdate: () => {
        core.installedUpdate += 1
      },
      hasRewriteKey: async () => core.hasKey,
      setRewriteKey: async (key: string) => {
        core.calls.push({ method: 'setRewriteKey', params: key })
        core.hasKey = key.trim().length > 0
        return { persisted: true }
      },
      openExternal: (url: string) => {
        core.opened.push(url)
      }
    }
  }
  return core
}

/** Marks a model as downloaded and tells the interface the download finished. */
export function finishDownload(core: FakeCore, id: string): void {
  core.models = core.models.map((m) => (m.id === id ? { ...m, installed: true, downloading: false } : m))
  core.emit('download-progress', { id, downloaded: 1, total: 1, done: true, error: null })
}
