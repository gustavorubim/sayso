import type { AppState } from '../App'
import { Keys } from '../components'
import { timeAgo } from '../format'

export function HomePage(s: AppState): React.JSX.Element {
  const active = s.models.find((m) => m.id === s.settings.model_id && m.installed)
  const liveWriting = s.settings.timing === 'live'
  // Live writing is always press-to-toggle (see Settings).
  const hold = s.settings.mode === 'hold' && !liveWriting
  const words = s.history.reduce((sum, h) => sum + h.words, 0)
  const live = s.status.phase === 'recording' || s.status.phase === 'transcribing' || s.status.phase === 'rewriting'

  return (
    <div className="page">
      <section className={`hero ${live ? 'is-live' : ''}`}>
        <Keys shortcut={s.settings.shortcut} size="xl" />
        <h1>
          {liveWriting
            ? 'Press the shortcut and speak. Your words are typed at each pause. Press it again to stop.'
            : hold
              ? 'Hold the shortcut and speak. Let go, and your words are typed where the cursor is.'
              : 'Press the shortcut and speak. Press it again, and your words are typed where the cursor is.'}
        </h1>
        <p className="hero-state" aria-live="polite">
          {s.status.phase === 'recording' && 'Listening…'}
          {s.status.phase === 'transcribing' && 'Writing…'}
          {s.status.phase === 'rewriting' && 'Polishing the wording…'}
          {!live && active && s.engine === 'loading' && `Loading ${active.name}…`}
          {!live && active && s.engine !== 'loading' && `Ready with ${active.name}.`}
          {!live && !active && 'One step left: choose a speech model.'}
        </p>
        {!active && (
          <button type="button" className="btn btn-primary" onClick={() => s.go('models')}>
            Choose a speech model
          </button>
        )}
        {active && s.settings.capture_enabled && (
          <p className="hero-second">
            <Keys shortcut={s.settings.capture_shortcut} /> writes down what the computer is playing. Press it again to
            stop.
          </p>
        )}
        {active && s.settings.rewrite_enabled && (
          <p className="hero-second">
            <Keys shortcut={s.settings.rewrite_shortcut} /> does the same, then tidies the wording before writing it.
          </p>
        )}
      </section>

      {active && (
        <section className="block">
          <h2>Try it here</h2>
          <textarea
            className="tryout"
            rows={4}
            placeholder={
              hold
                ? 'Click here, hold the shortcut, say something, then let go.'
                : 'Click here, press the shortcut, say something, then press it again.'
            }
          />
          <p className="quiet">
            It works the same in any app: email, chat, documents, code editors.
            {s.settings.voice_commands && ' Say “new line” or “new paragraph” to break the text.'}
          </p>
        </section>
      )}

      <section className="block">
        <h2>Recent</h2>
        {s.history.length === 0 ? (
          <p className="quiet">Nothing dictated yet. Your last few dictations will show up here.</p>
        ) : (
          <>
            <ul className="recent">
              {s.history.slice(0, 4).map((h) => (
                <li key={h.id}>
                  <p>{h.text}</p>
                  <time>{timeAgo(h.id)}</time>
                </li>
              ))}
            </ul>
            <p className="quiet">
              {words.toLocaleString()} words dictated in {s.history.length.toLocaleString()} takes.{' '}
              <button type="button" className="link" onClick={() => s.go('history')}>
                See all history
              </button>
            </p>
          </>
        )}
      </section>
    </div>
  )
}
