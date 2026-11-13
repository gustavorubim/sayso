import { Component, type ReactNode, useEffect, useState } from 'react'
import { shortcutFromEvent, shortcutKeys } from './format'

/**
 * Keeps a failure on one screen from blanking the whole window. The sidebar stays usable,
 * so the person can go somewhere else or try again.
 */
export class PageBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null }

  static getDerivedStateFromError(error: Error): { error: Error } {
    return { error }
  }

  render(): ReactNode {
    if (!this.state.error) return this.props.children
    return (
      <div className="page page-error" role="alert">
        <header className="page-head">
          <h1>This screen could not be shown</h1>
          <p>
            Something went wrong while drawing it. Dictation keeps working. Try another screen, or quit Sayso from the
            tray and open it again.
          </p>
        </header>
        <p className="quiet fine-print">{this.state.error.message}</p>
      </div>
    )
  }
}

export function Keys({ shortcut, size = 'md' }: { shortcut: string; size?: 'md' | 'xl' }): React.JSX.Element {
  return (
    <span className={`keys keys-${size}`}>
      {shortcutKeys(shortcut).map((k, i) => (
        <kbd key={`${k}-${i}`}>{k}</kbd>
      ))}
    </span>
  )
}

export function Toggle(props: {
  label: string
  hint?: string
  checked: boolean
  onChange: (value: boolean) => void
}): React.JSX.Element {
  return (
    <label className="row">
      <span className="row-text">
        <span className="row-label">{props.label}</span>
        {props.hint && <span className="row-hint">{props.hint}</span>}
      </span>
      <input
        type="checkbox"
        className="switch"
        role="switch"
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
      />
    </label>
  )
}

export function Field(props: { label: string; hint?: string; children: React.ReactNode }): React.JSX.Element {
  return (
    <div className="row">
      <span className="row-text">
        <span className="row-label">{props.label}</span>
        {props.hint && <span className="row-hint">{props.hint}</span>}
      </span>
      <span className="row-control">{props.children}</span>
    </div>
  )
}

/** Shows the current shortcut and records a new one from the keyboard. */
export function ShortcutInput(props: {
  value: string
  onChange: (shortcut: string) => Promise<boolean>
}): React.JSX.Element {
  const [recording, setRecording] = useState(false)

  useEffect(() => {
    if (!recording) return
    const onKey = (e: KeyboardEvent): void => {
      e.preventDefault()
      e.stopPropagation()
      if (e.code === 'Escape') {
        setRecording(false)
        return
      }
      const shortcut = shortcutFromEvent(e)
      if (shortcut) {
        setRecording(false)
        void props.onChange(shortcut)
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [recording, props])

  return (
    <span className="shortcut-input">
      {recording ? <span className="shortcut-wait">Press the new shortcut…</span> : <Keys shortcut={props.value} />}
      <button type="button" className="btn" onClick={() => setRecording((r) => !r)}>
        {recording ? 'Cancel' : 'Change'}
      </button>
    </span>
  )
}
