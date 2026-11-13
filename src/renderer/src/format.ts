const isMac = navigator.platform.toLowerCase().includes('mac')

const KEY_LABELS: Record<string, string> = {
  Ctrl: isMac ? '⌃' : 'Ctrl',
  Alt: isMac ? '⌥' : 'Alt',
  Shift: isMac ? '⇧' : 'Shift',
  Super: isMac ? '⌘' : 'Win',
  Space: 'Space',
  Backquote: '`',
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Comma: ',',
  Period: '.',
  Slash: '/',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→'
}

/** `Ctrl+Shift+KeyD` → `['Ctrl', 'Shift', 'D']` */
export function shortcutKeys(shortcut: string): string[] {
  return shortcut
    .split('+')
    .filter(Boolean)
    .map((part) => {
      if (KEY_LABELS[part]) return KEY_LABELS[part]
      if (part.startsWith('Key')) return part.slice(3)
      if (part.startsWith('Digit')) return part.slice(5)
      if (part.startsWith('Numpad')) return `Num ${part.slice(6)}`
      return part
    })
}

const MODIFIER_CODES = new Set([
  'ControlLeft',
  'ControlRight',
  'ShiftLeft',
  'ShiftRight',
  'AltLeft',
  'AltRight',
  'MetaLeft',
  'MetaRight'
])

/** Builds an accelerator from a keydown event, or `null` while only modifiers are held. */
export function shortcutFromEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code) || !e.code) return null
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Super')
  const isFunctionKey = /^F\d{1,2}$/.test(e.code)
  // A bare letter would fire on every keystroke, so require a modifier.
  if (parts.length === 0 && !isFunctionKey) return null
  parts.push(e.code)
  return parts.join('+')
}

export function bytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)} GB`
  return `${Math.round(n / 1e6)} MB`
}

const languageNames = new Intl.DisplayNames(['en'], { type: 'language' })
export function languageName(code: string): string {
  if (code === 'auto') return 'Detect automatically'
  try {
    return languageNames.of(code) ?? code
  } catch {
    return code
  }
}

export function timeAgo(ms: number): string {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000))
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.floor(s / 60)} min ago`
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`
  return new Date(ms).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

export function errorText(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err)
  // Electron prefixes errors thrown across IPC.
  return raw.replace(/^Error invoking remote method '[^']+': (Error: )?/, '')
}
