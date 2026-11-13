import { useEffect, useState } from 'react'
import type { AppState } from '../App'
import { Field, ShortcutInput, Toggle } from '../components'
import { errorText } from '../format'

export function RewritePage(s: AppState): React.JSX.Element {
  const { settings, update } = s
  const [hasKey, setHasKey] = useState<boolean | null>(null)
  const [keyInput, setKeyInput] = useState('')
  const [keyNote, setKeyNote] = useState<string | null>(null)
  const [models, setModels] = useState<string[]>([])
  const [model, setModel] = useState(settings.rewrite_model)
  const [prompt, setPrompt] = useState(settings.rewrite_prompt)
  const [sample, setSample] = useState(
    'so um i was thinking we could maybe meet on on thursday instead if thats ok with every one'
  )
  const [result, setResult] = useState<string | null>(null)
  const [trying, setTrying] = useState(false)

  const tryIt = async (): Promise<void> => {
    setTrying(true)
    setResult(null)
    try {
      setResult(await window.stt.call('rewrite_text', { text: sample }))
    } catch (err) {
      s.notify(errorText(err))
    } finally {
      setTrying(false)
    }
  }

  useEffect(() => {
    void window.stt.hasRewriteKey().then(setHasKey)
    // The list only feeds suggestions; the field works without it.
    window.stt
      .call('list_rewrite_models')
      .then(setModels)
      .catch(() => setModels([]))
  }, [])

  useEffect(() => setModel(settings.rewrite_model), [settings.rewrite_model])
  useEffect(() => setPrompt(settings.rewrite_prompt), [settings.rewrite_prompt])

  const saveKey = async (e: React.FormEvent): Promise<void> => {
    e.preventDefault()
    if (!keyInput.trim()) return
    try {
      const { persisted } = await window.stt.setRewriteKey(keyInput)
      setKeyInput('')
      setHasKey(true)
      setKeyNote(
        persisted
          ? null
          : 'This computer has no secure credential store, so the key is kept only until you quit the app.'
      )
    } catch (err) {
      s.notify(errorText(err))
    }
  }

  const removeKey = async (): Promise<void> => {
    await window.stt.setRewriteKey('')
    setHasKey(false)
    setKeyNote(null)
    if (settings.rewrite_enabled) await update({ rewrite_enabled: false })
  }

  const resetPrompt = async (): Promise<void> => {
    const defaults = await window.stt.call('get_default_settings')
    await update({ rewrite_prompt: defaults.rewrite_prompt })
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>Rewrite</h1>
        <p>
          A second shortcut that dictates and then has a language model tidy the result: typos fixed, false starts
          removed, awkward phrasing smoothed out. Your regular shortcut keeps writing exactly what you said.
        </p>
      </header>

      <section className="block">
        <h2>OpenRouter account</h2>
        <div className="panel">
          {hasKey ? (
            <Field label="API key" hint={keyNote ?? 'Saved, encrypted by your operating system.'}>
              <button type="button" className="btn" onClick={() => void removeKey()}>
                Remove key
              </button>
            </Field>
          ) : (
            <form className="row key-form" onSubmit={(e) => void saveKey(e)}>
              <label className="row-text" htmlFor="openrouter-key">
                <span className="row-label">API key</span>
                <span className="row-hint">
                  Create one at{' '}
                  <button
                    type="button"
                    className="link"
                    onClick={() => window.stt.openExternal('https://openrouter.ai/keys')}
                  >
                    openrouter.ai/keys
                  </button>
                  . Rewriting is billed to that account.
                </span>
              </label>
              <span className="row-control key-entry">
                <input
                  id="openrouter-key"
                  type="password"
                  autoComplete="off"
                  spellCheck={false}
                  value={keyInput}
                  onChange={(e) => setKeyInput(e.target.value)}
                  placeholder="sk-or-…"
                />
                <button type="submit" className="btn btn-primary" disabled={!keyInput.trim()}>
                  Save key
                </button>
              </span>
            </form>
          )}
        </div>
        <p className="quiet fine-print">
          When you use the rewrite shortcut, the transcribed text is sent to OpenRouter and on to the model you choose.
          Your audio is never sent, and nothing is sent when you use the regular shortcut.
        </p>
      </section>

      <section className="block">
        <h2>Shortcut</h2>
        <div className="panel">
          <Toggle
            label="Turn on the rewrite shortcut"
            hint={hasKey ? 'Works alongside your regular dictation shortcut.' : 'Add an API key first.'}
            checked={settings.rewrite_enabled}
            onChange={(v) => {
              if (v && !hasKey) s.notify('Add your OpenRouter API key first.')
              else void update({ rewrite_enabled: v })
            }}
          />
          <Field
            label="Rewrite shortcut"
            hint={
              settings.mode === 'hold'
                ? 'Hold it, speak, and let go. The cleaned-up text is written a moment later.'
                : 'Press it, speak, and press again. The cleaned-up text is written a moment later.'
            }
          >
            <ShortcutInput
              value={settings.rewrite_shortcut}
              onChange={(shortcut) => update({ rewrite_shortcut: shortcut })}
            />
          </Field>
        </div>
      </section>

      <section className="block">
        <h2>How it rewrites</h2>
        <div className="panel">
          <Field label="Model" hint="Any OpenRouter model id. Small, fast models suit this best.">
            <input
              className="model-input"
              list="rewrite-models"
              value={model}
              spellCheck={false}
              onChange={(e) => setModel(e.target.value)}
              onBlur={() => {
                if (model.trim() && model !== settings.rewrite_model) void update({ rewrite_model: model.trim() })
              }}
              aria-label="Rewrite model"
            />
            <datalist id="rewrite-models">
              {models.map((id) => (
                <option key={id} value={id} />
              ))}
            </datalist>
          </Field>
          <div className="row row-stacked">
            <label className="row-text" htmlFor="rewrite-prompt">
              <span className="row-label">Instructions</span>
              <span className="row-hint">
                Tell the model how to rewrite. Add your own rules, such as “use British spelling” or “keep it casual”.
              </span>
            </label>
            <textarea
              id="rewrite-prompt"
              rows={5}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              onBlur={() => {
                if (prompt.trim() && prompt !== settings.rewrite_prompt) void update({ rewrite_prompt: prompt })
              }}
            />
            <button type="button" className="link" onClick={() => void resetPrompt()}>
              Restore the default instructions
            </button>
          </div>
        </div>
      </section>

      <section className="block">
        <h2>Try it</h2>
        <p className="quiet">Type or paste some rough text to see what the model does with it.</p>
        <textarea
          className="tryout"
          rows={3}
          value={sample}
          onChange={(e) => setSample(e.target.value)}
          aria-label="Text to rewrite"
        />
        <button
          type="button"
          className="btn btn-primary try-button"
          disabled={!hasKey || trying || !sample.trim()}
          onClick={() => void tryIt()}
        >
          {trying ? 'Rewriting…' : 'Rewrite this'}
        </button>
        {!hasKey && <span className="quiet try-note">Add an API key first.</span>}
        {result !== null && <p className="try-result">{result}</p>}
      </section>
    </div>
  )
}
