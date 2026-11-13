import { useState } from 'react'
import type { AppState } from '../App'
import { Toggle } from '../components'

export function DictionaryPage(s: AppState): React.JSX.Element {
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const list = s.settings.replacements

  const add = async (e: React.FormEvent): Promise<void> => {
    e.preventDefault()
    const spoken = from.trim()
    if (!spoken) return
    const rest = list.filter((r) => r.from.toLowerCase() !== spoken.toLowerCase())
    if (await s.update({ replacements: [...rest, { from: spoken, to }] })) {
      setFrom('')
      setTo('')
    }
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>Dictionary</h1>
        <p>
          Fix words the model keeps getting wrong, such as names and product terms, or set up spoken shortcuts that
          expand into longer text.
        </p>
      </header>

      <section className="block">
        <h2>Replacements</h2>
        <form className="replace-form" onSubmit={(e) => void add(e)}>
          <label>
            When I say
            <input value={from} onChange={(e) => setFrom(e.target.value)} placeholder="my address" />
          </label>
          <label>
            Write
            <input value={to} onChange={(e) => setTo(e.target.value)} placeholder="221B Baker Street, London" />
          </label>
          <button type="submit" className="btn btn-primary" disabled={!from.trim()}>
            Add replacement
          </button>
        </form>

        {list.length === 0 ? (
          <p className="quiet">No replacements yet. Matching ignores capitalization and only replaces whole words.</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th scope="col">When I say</th>
                <th scope="col">Write</th>
                <th scope="col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {list.map((r) => (
                <tr key={r.from}>
                  <td>{r.from}</td>
                  <td>{r.to || <span className="quiet">(nothing: the words are removed)</span>}</td>
                  <td className="cell-action">
                    <button
                      type="button"
                      className="btn btn-quiet"
                      onClick={() => void s.update({ replacements: list.filter((x) => x !== r) })}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="block panel">
        <Toggle
          label="Spoken line breaks"
          hint="Saying “new line” or “new paragraph” inserts a break instead of the words."
          checked={s.settings.voice_commands}
          onChange={(v) => void s.update({ voice_commands: v })}
        />
        <Toggle
          label="Remove filler sounds"
          hint="Drops “um”, “uh” and “erm” from the text."
          checked={s.settings.remove_fillers}
          onChange={(v) => void s.update({ remove_fillers: v })}
        />
      </section>
    </div>
  )
}
