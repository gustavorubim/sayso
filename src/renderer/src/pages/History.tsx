import { useState } from 'react'
import type { AppState } from '../App'
import { Toggle } from '../components'
import { errorText, timeAgo } from '../format'

export function HistoryPage(s: AppState): React.JSX.Element {
  const [query, setQuery] = useState('')
  const [copied, setCopied] = useState<number | null>(null)
  const [confirmClear, setConfirmClear] = useState(false)

  const q = query.trim().toLowerCase()
  const items = q ? s.history.filter((h) => h.text.toLowerCase().includes(q)) : s.history

  const copy = async (id: number, text: string): Promise<void> => {
    try {
      await window.stt.call('copy_text', { text })
      setCopied(id)
      window.setTimeout(() => setCopied((c) => (c === id ? null : c)), 1500)
    } catch (err) {
      s.notify(errorText(err))
    }
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>History</h1>
        <p>Your last 300 dictations, stored only on this computer.</p>
      </header>

      <section className="block panel">
        <Toggle
          label="Keep history"
          hint="Turn off to stop saving dictations. Existing entries stay until you delete them."
          checked={s.settings.save_history}
          onChange={(v) => void s.update({ save_history: v })}
        />
      </section>

      <section className="block">
        {s.history.length > 0 && (
          <div className="toolbar">
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search history"
              aria-label="Search history"
            />
            {confirmClear ? (
              <>
                <button
                  type="button"
                  className="btn btn-danger"
                  onClick={() => {
                    setConfirmClear(false)
                    void window.stt.call('clear_history')
                  }}
                >
                  Delete all {s.history.length}
                </button>
                <button type="button" className="btn" onClick={() => setConfirmClear(false)}>
                  Keep
                </button>
              </>
            ) : (
              <button type="button" className="btn btn-quiet" onClick={() => setConfirmClear(true)}>
                Delete all
              </button>
            )}
          </div>
        )}

        {s.history.length === 0 && <p className="quiet">Nothing here yet. Dictate something and it will appear.</p>}
        {s.history.length > 0 && items.length === 0 && <p className="quiet">No dictation contains “{query}”.</p>}

        <ul className="history">
          {items.map((h) => (
            <li key={h.id}>
              <p>{h.text}</p>
              <div className="history-meta">
                <time>{timeAgo(h.id)}</time>
                <span>{(h.duration_ms / 1000).toFixed(1)} s</span>
                <span>{h.model}</span>
                <span className="history-actions">
                  <button type="button" className="btn btn-quiet" onClick={() => void copy(h.id, h.text)}>
                    {copied === h.id ? 'Copied' : 'Copy'}
                  </button>
                  <button
                    type="button"
                    className="btn btn-quiet"
                    onClick={() => void window.stt.call('delete_history_item', { id: h.id })}
                  >
                    Delete
                  </button>
                </span>
              </div>
            </li>
          ))}
        </ul>
      </section>
    </div>
  )
}
