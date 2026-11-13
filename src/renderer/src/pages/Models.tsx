import { useEffect, useState } from 'react'
import type { ModelInfo } from '../../../shared/types'
import type { AppState } from '../App'
import { Field } from '../components'
import { bytes, errorText, languageName } from '../format'

export function ModelsPage(s: AppState): React.JSX.Element {
  // A model picked in the dropdown before it is downloaded: it becomes active once it arrives.
  const [pending, setPending] = useState<string | null>(null)
  const active = s.models.find((m) => m.id === s.settings.model_id)
  const { update } = s

  useEffect(() => {
    if (!pending) return
    const model = s.models.find((m) => m.id === pending)
    if (model?.installed) {
      setPending(null)
      void update({ model_id: model.id })
    } else if (model && !model.downloading && !s.progress[pending]) {
      setPending(null) // cancelled or failed
    }
  }, [pending, s.models, s.progress, update])

  const download = async (id: string): Promise<void> => {
    try {
      await window.stt.call('download_model', { id })
      await s.refreshModels()
    } catch (err) {
      s.notify(errorText(err))
    }
  }

  const choose = async (id: string): Promise<void> => {
    const model = s.models.find((m) => m.id === id)
    if (!model) return
    if (model.installed) {
      setPending(null)
      await update({ model_id: id })
    } else {
      // Start the download before marking it pending, so the effect above sees it as downloading.
      if (!model.downloading) await download(id)
      setPending(id)
    }
  }

  const remove = async (model: ModelInfo): Promise<void> => {
    try {
      await window.stt.call('delete_model', { id: model.id })
      await s.refreshModels()
    } catch (err) {
      s.notify(errorText(err))
    }
  }

  const cancel = async (id: string): Promise<void> => {
    await window.stt.call('cancel_download', { id })
  }

  const selected = pending ?? s.settings.model_id ?? ''
  const languages = active ? [...(active.auto_language ? ['auto'] : []), ...active.language_choices] : []

  return (
    <div className="page">
      <header className="page-head">
        <h1>Speech models</h1>
        <p>
          Models are downloaded from Hugging Face once, then run on this computer. Pick one below; you can switch at
          any time.
        </p>
      </header>

      <section className="block panel">
        <Field label="Model in use" hint="Choosing a model you don't have yet downloads it first.">
          <select value={selected} onChange={(e) => void choose(e.target.value)} aria-label="Model in use">
            {!selected && <option value="">Choose a model…</option>}
            {s.models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
                {m.installed ? '' : ` — download ${bytes(m.size_bytes)}`}
              </option>
            ))}
          </select>
        </Field>
        {active && languages.length > 1 && (
          <Field label="Spoken language" hint="Picking your language is a little faster and more accurate than detecting it.">
            <select
              value={languages.includes(s.settings.language) ? s.settings.language : languages[0]}
              onChange={(e) => void update({ language: e.target.value })}
              aria-label="Spoken language"
            >
              {languages.map((code) => (
                <option key={code} value={code}>
                  {languageName(code)}
                </option>
              ))}
            </select>
          </Field>
        )}
        {active && languages.length <= 1 && (
          <Field label="Spoken language">
            <span className="quiet">{active.languages}</span>
          </Field>
        )}
        {pending && <p className="quiet">This model becomes active as soon as its download finishes.</p>}
      </section>

      <section className="block">
        <h2>All models</h2>
        <ul className="models">
          {s.models.map((m) => {
            const p = s.progress[m.id]
            const busy = m.downloading || Boolean(p)
            const pct = p && p.total > 0 ? Math.min(100, (p.downloaded / p.total) * 100) : 0
            const isActive = m.id === s.settings.model_id && m.installed
            return (
              <li key={m.id} className={isActive ? 'model is-active' : 'model'}>
                <div className="model-main">
                  <h3>
                    {m.name}
                    {isActive && <span className="tag tag-on">In use</span>}
                    {m.recommended && !isActive && <span className="tag">Good first choice</span>}
                  </h3>
                  <p>{m.description}</p>
                  <p className="model-meta">
                    <span>{m.languages}</span>
                    <span>{bytes(m.size_bytes)}</span>
                    <button
                      type="button"
                      className="link"
                      onClick={() => window.stt.openExternal(`https://huggingface.co/${m.source}`)}
                    >
                      {m.source} on Hugging Face
                    </button>
                  </p>
                  {busy && (
                    <div className="progress" aria-label={`Downloading ${m.name}`}>
                      <div
                        className="progress-bar"
                        role="progressbar"
                        aria-valuemin={0}
                        aria-valuemax={100}
                        aria-valuenow={Math.round(pct)}
                      >
                        <i style={{ width: `${pct}%` }} />
                      </div>
                      <span>{p ? `${bytes(p.downloaded)} of ${bytes(p.total)}` : 'Starting…'}</span>
                    </div>
                  )}
                </div>
                <div className="model-actions">
                  {busy ? (
                    <button type="button" className="btn" onClick={() => void cancel(m.id)}>
                      Cancel
                    </button>
                  ) : m.installed ? (
                    <>
                      {!isActive && (
                        <button type="button" className="btn btn-primary" onClick={() => void choose(m.id)}>
                          Use
                        </button>
                      )}
                      <button type="button" className="btn btn-quiet" onClick={() => void remove(m)}>
                        Delete
                      </button>
                    </>
                  ) : (
                    <button type="button" className="btn" onClick={() => void download(m.id)}>
                      Download
                    </button>
                  )}
                </div>
              </li>
            )
          })}
        </ul>
      </section>
    </div>
  )
}
