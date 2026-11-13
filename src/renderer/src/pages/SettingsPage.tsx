import { useEffect, useState } from 'react'
import type { Settings } from '../../../shared/types'
import type { AppState } from '../App'
import { Field, ShortcutInput, Toggle } from '../components'

const isLinux = navigator.platform.toLowerCase().includes('linux')

export function SettingsPage(s: AppState): React.JSX.Element {
  const [devices, setDevices] = useState<string[]>([])
  const [checking, setChecking] = useState(false)
  const [checkResult, setCheckResult] = useState<string | null>(null)

  const check = async (): Promise<void> => {
    setChecking(true)
    setCheckResult(null)
    const result = await window.stt.checkForUpdates()
    setChecking(false)
    if (result.message) setCheckResult(result.message)
    else if (result.state === 'none') setCheckResult('You have the latest version.')
    else if (result.state === 'ready') setCheckResult(`Version ${result.version} is ready. Restart to install it.`)
    else if (result.state === 'downloading') setCheckResult(`Downloading version ${result.version}…`)
    else if (result.state === 'available') setCheckResult(`Version ${result.version} is available.`)
  }

  useEffect(() => {
    window.stt
      .call('list_input_devices')
      .then(setDevices)
      .catch(() => setDevices([]))
  }, [])

  const { settings, update } = s
  // Keep a saved device selectable even while it is unplugged.
  const deviceOptions =
    settings.input_device && !devices.includes(settings.input_device)
      ? [settings.input_device, ...devices]
      : devices

  return (
    <div className="page">
      <header className="page-head">
        <h1>Settings</h1>
      </header>

      <section className="block">
        <h2>Shortcut</h2>
        <div className="panel">
          <Field label="Dictation shortcut" hint="Works in every app while Shortcut to Text is running.">
            <ShortcutInput value={settings.shortcut} onChange={(shortcut) => update({ shortcut })} />
          </Field>
          <Field
            label="How it works"
            hint={
              settings.mode === 'hold'
                ? 'Records while the shortcut is held down.'
                : 'Press once to start, again to stop. Better for long dictation.'
            }
          >
            <select
              value={settings.mode}
              onChange={(e) => void update({ mode: e.target.value as Settings['mode'] })}
              aria-label="How the shortcut works"
            >
              <option value="hold">Hold to talk</option>
              <option value="toggle">Press to start and stop</option>
            </select>
          </Field>
        </div>
      </section>

      <section className="block">
        <h2>Microphone</h2>
        <div className="panel">
          <Field label="Input device">
            <select
              value={settings.input_device ?? ''}
              onChange={(e) => void update({ input_device: e.target.value || null })}
              aria-label="Input device"
            >
              <option value="">System default</option>
              {deviceOptions.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </Field>
        </div>
      </section>

      <section className="block">
        <h2>Output</h2>
        <div className="panel">
          <Field
            label="Deliver text by"
            hint={
              settings.output === 'paste'
                ? 'Fastest, and works with any length of text.'
                : settings.output === 'type'
                  ? 'For apps that block pasting. Slower on long text.'
                  : 'Nothing is inserted; paste it yourself.'
            }
          >
            <select
              value={settings.output}
              onChange={(e) => void update({ output: e.target.value as Settings['output'] })}
              aria-label="Deliver text by"
            >
              <option value="paste">Pasting at the cursor</option>
              <option value="type">Typing it out</option>
              <option value="copy">Copying to the clipboard only</option>
            </select>
          </Field>
          {settings.output === 'paste' && (
            <Toggle
              label="Put my clipboard back afterwards"
              hint="Restores whatever text you had copied before dictating."
              checked={settings.restore_clipboard}
              onChange={(v) => void update({ restore_clipboard: v })}
            />
          )}
          <Toggle
            label="Add a space after each dictation"
            hint="So the next sentence doesn't run into the last one."
            checked={settings.trailing_space}
            onChange={(v) => void update({ trailing_space: v })}
          />
        </div>
      </section>

      <section className="block">
        <h2>App</h2>
        <div className="panel">
          <Toggle
            label="Show the listening indicator"
            hint="A small pill at the bottom of the screen while recording and writing."
            checked={settings.show_overlay}
            onChange={(v) => void update({ show_overlay: v })}
          />
          {!isLinux && (
            <Toggle
              label="Open at login"
              hint="Starts quietly in the tray."
              checked={settings.launch_at_login}
              onChange={(v) => void update({ launch_at_login: v })}
            />
          )}
          <Field label="Updates" hint={checkResult ?? 'New versions are checked for automatically.'}>
            <button type="button" className="btn" disabled={checking} onClick={() => void check()}>
              {checking ? 'Checking…' : 'Check for updates'}
            </button>
          </Field>
        </div>
      </section>
    </div>
  )
}
