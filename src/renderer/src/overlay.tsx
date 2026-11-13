import '@fontsource-variable/fredoka'
import './styles.css'
import { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import type { Status } from '../../shared/types'

const BARS = 13

function Overlay(): React.JSX.Element {
  const [status, setStatus] = useState<Status>({ phase: 'idle' })
  const [levels, setLevels] = useState<number[]>(() => Array(BARS).fill(0))

  useEffect(() => {
    const offStatus = window.stt.on('status', (s) => {
      setStatus(s)
      if (s.phase === 'recording') setLevels(Array(BARS).fill(0))
    })
    const offLevel = window.stt.on('level', (level) => {
      // Speech RMS is small; lift it into a visible range.
      const shaped = Math.min(1, Math.pow(level * 9, 0.7))
      setLevels((prev) => [...prev.slice(1), shaped])
    })
    return () => {
      offStatus()
      offLevel()
    }
  }, [])

  return (
    <div className={`pill pill-${status.phase}`} role="status">
      {status.phase === 'recording' && (
        <>
          <span className="pill-dot" />
          <span className="pill-bars" aria-hidden="true">
            {levels.map((l, i) => (
              <i key={i} style={{ height: `${Math.max(12, l * 100)}%` }} />
            ))}
          </span>
          <span className="pill-text">Listening</span>
        </>
      )}
      {status.phase === 'transcribing' && (
        <>
          <span className="pill-spinner" aria-hidden="true" />
          <span className="pill-text">Writing</span>
        </>
      )}
      {status.phase === 'rewriting' && (
        <>
          <span className="pill-spinner" aria-hidden="true" />
          <span className="pill-text">Polishing</span>
        </>
      )}
      {status.phase === 'done' && <span className="pill-text">Done</span>}
      {status.phase === 'error' && <span className="pill-text">{status.message ?? 'Something went wrong'}</span>}
    </div>
  )
}

createRoot(document.getElementById('root')!).render(<Overlay />)
