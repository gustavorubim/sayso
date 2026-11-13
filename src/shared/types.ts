// Types shared by the Electron main process, the preload script and the UI.
// They mirror the JSON the Rust core sends (see core/src).

export interface Replacement {
  from: string
  to: string
}

export interface Settings {
  shortcut: string
  mode: 'hold' | 'toggle'
  model_id: string | null
  language: string
  input_device: string | null
  output: 'paste' | 'type' | 'copy'
  restore_clipboard: boolean
  show_overlay: boolean
  launch_at_login: boolean
  remove_fillers: boolean
  voice_commands: boolean
  trailing_space: boolean
  save_history: boolean
  replacements: Replacement[]
  /** `after`: write when the recording stops. `live`: write at each pause while speaking. */
  timing: 'after' | 'live'
  rewrite_enabled: boolean
  rewrite_shortcut: string
  rewrite_model: string
  rewrite_prompt: string
  capture_enabled: boolean
  capture_shortcut: string
}

export interface ModelInfo {
  id: string
  name: string
  family: string
  repo: string
  source: string
  description: string
  languages: string
  language_choices: string[]
  auto_language: boolean
  size_bytes: number
  recommended: boolean
  installed: boolean
  downloading: boolean
}

export interface HistoryItem {
  id: number
  text: string
  model: string
  duration_ms: number
  words: number
  rewritten: boolean
}

export type Phase = 'idle' | 'recording' | 'transcribing' | 'rewriting' | 'done' | 'error'

export interface Status {
  phase: Phase
  message?: string
}

export interface DownloadProgress {
  id: string
  downloaded: number
  total: number
  done: boolean
  error: string | null
}

export interface UpdateState {
  /**
   * `none`: up to date (or not checked). `available`: a newer version exists but must be
   * downloaded by hand. `downloading`: fetching it. `ready`: downloaded, restart to install.
   * `error`: the check or download failed.
   */
  state: 'none' | 'available' | 'downloading' | 'ready' | 'error'
  version?: string
  message?: string
}

export interface CoreEvents {
  update: UpdateState
  status: Status
  level: number
  'download-progress': DownloadProgress
  'settings-changed': Settings
  'history-changed': null
  'hotkey-error': string
  engine: 'loading' | 'ready' | 'failed'
  'core-exit': string
}

export interface CoreMethods {
  get_settings: { params: void; result: Settings }
  save_settings: { params: Settings; result: Settings }
  get_status: { params: void; result: Status }
  list_models: { params: void; result: ModelInfo[] }
  download_model: { params: { id: string }; result: null }
  cancel_download: { params: { id: string }; result: null }
  delete_model: { params: { id: string }; result: null }
  list_input_devices: { params: void; result: string[] }
  get_history: { params: void; result: HistoryItem[] }
  delete_history_item: { params: { id: number }; result: null }
  clear_history: { params: void; result: null }
  get_default_settings: { params: void; result: Settings }
  list_rewrite_models: { params: void; result: string[] }
  rewrite_text: { params: { text: string }; result: string }
  toggle_recording: { params: void; result: null }
  toggle_capture: { params: void; result: null }
  copy_text: { params: { text: string }; result: null }
}

export interface AppInfo {
  version: string
  platform: string
}

export interface SttApi {
  call<M extends keyof CoreMethods>(
    method: M,
    ...params: CoreMethods[M]['params'] extends void ? [] : [CoreMethods[M]['params']]
  ): Promise<CoreMethods[M]['result']>
  on<E extends keyof CoreEvents>(event: E, listener: (payload: CoreEvents[E]) => void): () => void
  info(): Promise<AppInfo>
  /** The last known update state, without contacting the server. */
  updateState(): Promise<UpdateState>
  checkForUpdates(): Promise<UpdateState>
  /** Quits, installs the downloaded update, and reopens the app. */
  installUpdate(): void
  /** Whether an OpenRouter API key is stored. The key itself is never exposed to the interface. */
  hasRewriteKey(): Promise<boolean>
  /** Stores the OpenRouter API key, or removes it when empty. */
  setRewriteKey(key: string): Promise<{ persisted: boolean }>
  openExternal(url: string): void
}
