// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Settings } from '../../shared/types'
import { App } from './App'
import { PageBoundary } from './components'
import { type FakeCore, type Initial, fakeCore, finishDownload, model } from './test/fake-core'

afterEach(cleanup)

/** Renders the app against a simulated engine and waits until it has loaded. */
async function start(initial: Initial = {}): Promise<FakeCore> {
  const core = fakeCore(initial)
  window.stt = core.api
  render(<App />)
  await screen.findByRole('navigation')
  return core
}

async function open(name: string): Promise<void> {
  fireEvent.click(screen.getByRole('button', { name }))
  await screen.findByRole('heading', { level: 1 })
}

const lastSave = (core: FakeCore): Settings => core.callsTo('save_settings').at(-1) as Settings

const withModel: Initial = {
  settings: { model_id: 'fast' },
  models: [model('fast', 'Fast Model', { installed: true, recommended: true }), model('big', 'Big Model')]
}

describe('first run', () => {
  it('shows the shortcut and asks for a model', async () => {
    await start()
    expect(screen.getByText('Ctrl')).toBeTruthy()
    expect(screen.getByText('Space')).toBeTruthy()
    expect(screen.getByText('One step left: choose a speech model.')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Choose a speech model' }))
    expect(await screen.findByRole('heading', { name: 'Speech models' })).toBeTruthy()
  })

  it('shows the version', async () => {
    await start()
    expect(await screen.findByText('Version 9.9.9')).toBeTruthy()
  })
})

describe('navigation', () => {
  it('opens every screen', async () => {
    await start(withModel)
    for (const [item, heading] of [
      ['Speech models', 'Speech models'],
      ['Rewrite', 'Rewrite'],
      ['Dictionary', 'Dictionary'],
      ['History', 'History'],
      ['Settings', 'Settings']
    ]) {
      await open(item)
      expect(screen.getByRole('heading', { level: 1, name: heading })).toBeTruthy()
    }
    await open('Dictate')
    expect(screen.getByText('Ready with Fast Model.')).toBeTruthy()
  })
})

describe('dictate screen', () => {
  it('follows the recording state', async () => {
    const core = await start(withModel)
    act(() => core.emit('status', { phase: 'recording' }))
    expect(screen.getByText('Listening…')).toBeTruthy()
    act(() => core.emit('status', { phase: 'transcribing' }))
    expect(screen.getByText('Writing…')).toBeTruthy()
    act(() => core.emit('status', { phase: 'rewriting' }))
    expect(screen.getByText('Polishing the wording…')).toBeTruthy()
    act(() => core.emit('status', { phase: 'idle' }))
    expect(screen.getByText('Ready with Fast Model.')).toBeTruthy()
  })

  it('shows an error from the engine as a notice', async () => {
    const core = await start(withModel)
    act(() => core.emit('status', { phase: 'error', message: 'Heard only silence.' }))
    expect(screen.getByRole('alert').textContent).toContain('Heard only silence.')
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }))
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('explains each mode in its own words', async () => {
    await start({ ...withModel, settings: { model_id: 'fast', mode: 'toggle' } })
    expect(screen.getByRole('heading', { level: 1 }).textContent).toContain('Press it again')
    cleanup()
    await start({ ...withModel, settings: { model_id: 'fast', timing: 'live' } })
    expect(screen.getByRole('heading', { level: 1 }).textContent).toContain('at each pause')
  })

  it('lists recent dictations and the optional shortcuts', async () => {
    await start({
      ...withModel,
      settings: { model_id: 'fast', rewrite_enabled: true, capture_enabled: true },
      history: [{ id: Date.now(), text: 'Buy milk.', model: 'fast', duration_ms: 1200, words: 2, rewritten: false }]
    })
    expect(screen.getByText('Buy milk.')).toBeTruthy()
    expect(screen.getByText(/tidies the wording/)).toBeTruthy()
    expect(screen.getByText(/what the computer is playing/)).toBeTruthy()
  })

  it('tells the person when a shortcut could not be registered', async () => {
    const core = await start(withModel)
    act(() => core.emit('hotkey-error', 'Could not register Ctrl+Shift+Space.'))
    expect(screen.getByRole('alert').textContent).toContain('Could not register')
  })
})

describe('speech models', () => {
  it('downloads a model picked in the dropdown, then switches to it', async () => {
    const core = await start()
    await open('Speech models')
    fireEvent.change(screen.getByLabelText('Model in use'), { target: { value: 'big' } })
    await waitFor(() => expect(core.callsTo('download_model')).toEqual([{ id: 'big' }]))
    expect(await screen.findByText(/becomes active as soon as its download finishes/)).toBeTruthy()
    // Not selected until it has actually arrived.
    expect(core.callsTo('save_settings')).toEqual([])

    act(() => finishDownload(core, 'big'))
    await waitFor(() => expect(lastSave(core)?.model_id).toBe('big'))
  })

  it('shows progress while downloading and offers to cancel', async () => {
    const core = await start()
    await open('Speech models')
    fireEvent.click(screen.getAllByRole('button', { name: 'Download' })[0])
    await waitFor(() => expect(core.callsTo('download_model')).toEqual([{ id: 'fast' }]))
    act(() =>
      core.emit('download-progress', {
        id: 'fast',
        downloaded: 150_000_000,
        total: 300_000_000,
        done: false,
        error: null
      })
    )
    expect(screen.getByText('150 MB of 300 MB')).toBeTruthy()
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('50')
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(core.callsTo('cancel_download')).toEqual([{ id: 'fast' }]))
  })

  it('reports a failed download', async () => {
    const core = await start()
    await open('Speech models')
    act(() =>
      core.emit('download-progress', {
        id: 'fast',
        downloaded: 0,
        total: 1,
        done: true,
        error: 'The downloaded file did not match its checksum.'
      })
    )
    expect((await screen.findByRole('alert')).textContent).toContain('checksum')
  })

  it('switches to a downloaded model without downloading again', async () => {
    const core = await start({
      settings: { model_id: 'fast' },
      models: [model('fast', 'Fast Model', { installed: true }), model('big', 'Big Model', { installed: true })]
    })
    await open('Speech models')
    fireEvent.click(screen.getByRole('button', { name: 'Use' }))
    await waitFor(() => expect(lastSave(core)?.model_id).toBe('big'))
    expect(core.callsTo('download_model')).toEqual([])
  })

  it('deletes a model', async () => {
    const core = await start(withModel)
    await open('Speech models')
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))
    await waitFor(() => expect(core.callsTo('delete_model')).toEqual([{ id: 'fast' }]))
  })

  it('offers only the languages the model in use supports', async () => {
    const core = await start({
      settings: { model_id: 'big' },
      models: [model('big', 'Big Model', { installed: true, language_choices: ['en', 'pt'], auto_language: true })]
    })
    await open('Speech models')
    const language = screen.getByLabelText('Spoken language') as HTMLSelectElement
    expect([...language.options].map((o) => o.value)).toEqual(['auto', 'en', 'pt'])
    fireEvent.change(language, { target: { value: 'pt' } })
    await waitFor(() => expect(lastSave(core)?.language).toBe('pt'))
  })

  it('marks the model in use and the recommended one, and links to the model card', async () => {
    const core = await start({
      settings: { model_id: 'big' },
      models: [model('fast', 'Fast Model', { recommended: true }), model('big', 'Big Model', { installed: true })]
    })
    await open('Speech models')
    expect(screen.getByText('In use')).toBeTruthy()
    expect(screen.getByText('Good first choice')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'source/fast on Hugging Face' }))
    expect(core.opened).toEqual(['https://huggingface.co/source/fast'])
  })
})

describe('dictionary', () => {
  it('adds, replaces and removes a replacement', async () => {
    const core = await start(withModel)
    await open('Dictionary')
    const add = screen.getByRole('button', { name: 'Add replacement' }) as HTMLButtonElement
    expect(add.disabled).toBe(true)

    fireEvent.change(screen.getByLabelText('When I say'), { target: { value: '  my address ' } })
    fireEvent.change(screen.getByLabelText('Write'), { target: { value: '221B Baker Street' } })
    fireEvent.click(add)
    await waitFor(() => expect(lastSave(core)?.replacements).toEqual([{ from: 'my address', to: '221B Baker Street' }]))
    expect(await screen.findByText('221B Baker Street')).toBeTruthy()

    // The same phrase again replaces the old entry; it does not add a second one.
    fireEvent.change(screen.getByLabelText('When I say'), { target: { value: 'My Address' } })
    fireEvent.change(screen.getByLabelText('Write'), { target: { value: '10 Downing Street' } })
    fireEvent.click(add)
    await waitFor(() => expect(lastSave(core)?.replacements).toEqual([{ from: 'My Address', to: '10 Downing Street' }]))

    fireEvent.click(await screen.findByRole('button', { name: 'Remove' }))
    await waitFor(() => expect(lastSave(core)?.replacements).toEqual([]))
  })

  it('switches spoken line breaks and filler removal', async () => {
    const core = await start(withModel)
    await open('Dictionary')
    fireEvent.click(screen.getByRole('switch', { name: /Spoken line breaks/ }))
    await waitFor(() => expect(lastSave(core)?.voice_commands).toBe(false))
    fireEvent.click(screen.getByRole('switch', { name: /Remove filler sounds/ }))
    await waitFor(() => expect(lastSave(core)?.remove_fillers).toBe(true))
  })
})

describe('history', () => {
  const history = [
    { id: 3, text: 'Send the report today.', model: 'fast', duration_ms: 2000, words: 4, rewritten: true },
    { id: 2, text: 'Call the dentist.', model: 'fast', duration_ms: 1500, words: 3, rewritten: false },
    { id: 1, text: 'Report the outage.', model: 'fast', duration_ms: 1500, words: 3, rewritten: false }
  ]

  it('searches, copies and deletes', async () => {
    const core = await start({ ...withModel, history })
    await open('History')
    expect(screen.getByText('Rewritten')).toBeTruthy()

    fireEvent.change(screen.getByLabelText('Search history'), { target: { value: 'REPORT' } })
    expect(screen.queryByText('Call the dentist.')).toBeNull()
    expect(screen.getByText('Send the report today.')).toBeTruthy()
    expect(screen.getByText('Report the outage.')).toBeTruthy()

    fireEvent.change(screen.getByLabelText('Search history'), { target: { value: 'zebra' } })
    expect(screen.getByText(/No dictation contains/)).toBeTruthy()
    fireEvent.change(screen.getByLabelText('Search history'), { target: { value: '' } })

    const first = screen.getByText('Send the report today.').closest('li')!
    fireEvent.click(within(first).getByRole('button', { name: 'Copy' }))
    await waitFor(() => expect(core.callsTo('copy_text')).toEqual([{ text: 'Send the report today.' }]))
    expect(await within(first).findByRole('button', { name: 'Copied' })).toBeTruthy()

    fireEvent.click(within(first).getByRole('button', { name: 'Delete' }))
    await waitFor(() => expect(screen.queryByText('Send the report today.')).toBeNull())
    expect(core.callsTo('delete_history_item')).toEqual([{ id: 3 }])
  })

  it('asks before deleting everything', async () => {
    const core = await start({ ...withModel, history })
    await open('History')
    fireEvent.click(screen.getByRole('button', { name: 'Delete all' }))
    expect(core.callsTo('clear_history')).toEqual([])
    fireEvent.click(screen.getByRole('button', { name: 'Keep' }))
    expect(screen.getByText('Call the dentist.')).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'Delete all' }))
    fireEvent.click(screen.getByRole('button', { name: 'Delete all 3' }))
    await waitFor(() => expect(core.callsTo('clear_history')).toHaveLength(1))
    expect(await screen.findByText(/Nothing here yet/)).toBeTruthy()
  })

  it('can be switched off', async () => {
    const core = await start(withModel)
    await open('History')
    fireEvent.click(screen.getByRole('switch', { name: /Keep history/ }))
    await waitFor(() => expect(lastSave(core)?.save_history).toBe(false))
  })
})

describe('settings', () => {
  it('saves each choice', async () => {
    const core = await start(withModel)
    await open('Settings')

    fireEvent.change(screen.getByLabelText('How the shortcut works'), { target: { value: 'toggle' } })
    await waitFor(() => expect(lastSave(core)?.mode).toBe('toggle'))

    fireEvent.change(await screen.findByLabelText('Input device'), { target: { value: 'USB Microphone' } })
    await waitFor(() => expect(lastSave(core)?.input_device).toBe('USB Microphone'))
    fireEvent.change(screen.getByLabelText('Input device'), { target: { value: '' } })
    await waitFor(() => expect(lastSave(core)?.input_device).toBeNull())

    fireEvent.change(screen.getByLabelText('Deliver text by'), { target: { value: 'type' } })
    await waitFor(() => expect(lastSave(core)?.output).toBe('type'))

    fireEvent.click(screen.getByRole('switch', { name: /Add a space after each dictation/ }))
    await waitFor(() => expect(lastSave(core)?.trailing_space).toBe(false))

    fireEvent.click(screen.getByRole('switch', { name: /Transcribe what the computer is playing/ }))
    await waitFor(() => expect(lastSave(core)?.capture_enabled).toBe(true))

    fireEvent.click(screen.getByRole('switch', { name: /Open at login/ }))
    await waitFor(() => expect(lastSave(core)?.launch_at_login).toBe(true))
  })

  it('locks hold-or-toggle while writing as you speak', async () => {
    const core = await start(withModel)
    await open('Settings')
    fireEvent.change(screen.getByLabelText('When text is written'), { target: { value: 'live' } })
    await waitFor(() => expect(lastSave(core)?.timing).toBe('live'))
    const mode = (await screen.findByLabelText('How the shortcut works')) as HTMLSelectElement
    await waitFor(() => expect(mode.disabled).toBe(true))
    expect(mode.value).toBe('toggle')
  })

  it('only offers to restore the clipboard when pasting', async () => {
    await start({ ...withModel, settings: { model_id: 'fast', output: 'copy' } })
    await open('Settings')
    expect(screen.queryByRole('switch', { name: /Put my clipboard back/ })).toBeNull()
  })

  it('puts the old value back and says why when a change is refused', async () => {
    const core = await start(withModel)
    await open('Settings')
    core.failNextSave = 'Could not register Ctrl+Alt+KeyD. Another app may be using it.'
    fireEvent.change(screen.getByLabelText('How the shortcut works'), { target: { value: 'toggle' } })
    expect((await screen.findByRole('alert')).textContent).toContain('Another app may be using it.')
    // The error is shown without Electron's wrapper text.
    expect(screen.getByRole('alert').textContent).not.toContain('remote method')
    await waitFor(() =>
      expect((screen.getByLabelText('How the shortcut works') as HTMLSelectElement).value).toBe('hold')
    )
  })

  it('records a new shortcut from the keyboard', async () => {
    const core = await start(withModel)
    await open('Settings')
    fireEvent.click(screen.getAllByRole('button', { name: 'Change' })[0])
    expect(screen.getByText('Press the new shortcut…')).toBeTruthy()
    // Modifiers alone do nothing; the shortcut is complete when a real key arrives.
    fireEvent.keyDown(window, { code: 'ControlLeft', ctrlKey: true })
    expect(core.callsTo('save_settings')).toEqual([])
    fireEvent.keyDown(window, { code: 'KeyD', ctrlKey: true, altKey: true })
    await waitFor(() => expect(lastSave(core)?.shortcut).toBe('Ctrl+Alt+KeyD'))
  })

  it('cancels shortcut recording with Escape', async () => {
    const core = await start(withModel)
    await open('Settings')
    fireEvent.click(screen.getAllByRole('button', { name: 'Change' })[0])
    fireEvent.keyDown(window, { code: 'Escape' })
    expect(screen.queryByText('Press the new shortcut…')).toBeNull()
    expect(core.callsTo('save_settings')).toEqual([])
  })
})

describe('rewrite', () => {
  it('stores the key without ever showing it, and removes it', async () => {
    const core = await start(withModel)
    await open('Rewrite')
    const field = (await screen.findByPlaceholderText('sk-or-…')) as HTMLInputElement
    expect(field.type).toBe('password')
    fireEvent.change(field, { target: { value: 'sk-or-secret-value' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save key' }))

    expect(await screen.findByRole('button', { name: 'Remove key' })).toBeTruthy()
    expect(core.callsTo('setRewriteKey')).toEqual(['sk-or-secret-value'])
    expect(document.body.innerHTML).not.toContain('sk-or-secret-value')
    // The key travels its own channel, never through a general engine request.
    expect(JSON.stringify(core.calls.filter((c) => c.method !== 'setRewriteKey'))).not.toContain('sk-or-secret-value')

    fireEvent.click(screen.getByRole('button', { name: 'Remove key' }))
    await waitFor(() => expect(core.callsTo('setRewriteKey')).toEqual(['sk-or-secret-value', '']))
    expect(await screen.findByRole('button', { name: 'Save key' })).toBeTruthy()
  })

  it('will not switch on without a key', async () => {
    const core = await start(withModel)
    await open('Rewrite')
    fireEvent.click(await screen.findByRole('switch', { name: /Turn on the rewrite shortcut/ }))
    expect((await screen.findByRole('alert')).textContent).toContain('API key first')
    expect(core.callsTo('save_settings')).toEqual([])
  })

  it('switches on with a key, and saves the model and instructions', async () => {
    const core = await start({ ...withModel, hasKey: true })
    await open('Rewrite')
    await screen.findByRole('button', { name: 'Remove key' })
    fireEvent.click(screen.getByRole('switch', { name: /Turn on the rewrite shortcut/ }))
    await waitFor(() => expect(lastSave(core)?.rewrite_enabled).toBe(true))

    const modelField = screen.getByLabelText('Rewrite model')
    fireEvent.change(modelField, { target: { value: 'openai/gpt-6-luna' } })
    fireEvent.blur(modelField)
    await waitFor(() => expect(lastSave(core)?.rewrite_model).toBe('openai/gpt-6-luna'))

    const instructions = screen.getByLabelText(/Instructions/)
    fireEvent.change(instructions, { target: { value: 'Use British spelling.' } })
    fireEvent.blur(instructions)
    await waitFor(() => expect(lastSave(core)?.rewrite_prompt).toBe('Use British spelling.'))

    fireEvent.click(screen.getByRole('button', { name: 'Restore the default instructions' }))
    await waitFor(() => expect(lastSave(core)?.rewrite_prompt).toBe('Fix typos.'))
  })

  it('tries a rewrite on typed text', async () => {
    const core = await start({ ...withModel, hasKey: true })
    await open('Rewrite')
    await screen.findByRole('button', { name: 'Remove key' })
    fireEvent.change(screen.getByLabelText('Text to rewrite'), { target: { value: 'so um hello' } })
    fireEvent.click(screen.getByRole('button', { name: 'Rewrite this' }))
    expect(await screen.findByText('A tidied sentence.')).toBeTruthy()
    expect(core.callsTo('rewrite_text')).toEqual([{ text: 'so um hello' }])
  })
})

describe('updates', () => {
  it('offers to restart when an update has downloaded', async () => {
    const core = await start(withModel)
    act(() => core.emit('update', { state: 'downloading', version: '9.9.10' }))
    expect(screen.getByText('Downloading version 9.9.10…')).toBeTruthy()
    act(() => core.emit('update', { state: 'ready', version: '9.9.10' }))
    fireEvent.click(screen.getByRole('button', { name: 'Restart to update' }))
    expect(core.installedUpdate).toBe(1)
  })

  it('links to the download where the app cannot update itself', async () => {
    const core = await start(withModel)
    act(() => core.emit('update', { state: 'available', version: '9.9.10' }))
    fireEvent.click(screen.getByRole('button', { name: 'Get the update' }))
    expect(core.opened).toEqual(['https://github.com/gustavorubim/sayso/releases/latest'])
  })

  it('checks on demand from the sidebar', async () => {
    const core = await start(withModel)
    fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }))
    expect(await screen.findByText('You have the latest version.')).toBeTruthy()

    core.update = { state: 'ready', version: '9.9.10' }
    fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }))
    expect(await screen.findByRole('button', { name: 'Restart to update' })).toBeTruthy()
  })

  it('shows nothing when there is no update', async () => {
    await start(withModel)
    expect(screen.queryByRole('status')).toBeNull()
  })
})

describe('when something breaks', () => {
  it('a screen that fails to draw does not blank the window', () => {
    const Broken = (): React.JSX.Element => {
      throw new Error('boom in a screen')
    }
    const silence = vi.spyOn(console, 'error').mockImplementation(() => {})
    render(
      <PageBoundary>
        <Broken />
      </PageBoundary>
    )
    silence.mockRestore()
    expect(screen.getByRole('alert').textContent).toContain('This screen could not be shown')
    expect(screen.getByRole('alert').textContent).toContain('boom in a screen')
  })

  it('reloads after the engine restarts', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const core = await start(withModel)
      const before = core.callsTo('get_settings').length
      act(() => core.emit('core-exit', 'The speech engine stopped unexpectedly (code 101).'))
      expect(screen.getByRole('alert').textContent).toContain('stopped unexpectedly')
      await act(async () => {
        await vi.advanceTimersByTimeAsync(2500)
      })
      expect(core.callsTo('get_settings').length).toBeGreaterThan(before)
    } finally {
      vi.useRealTimers()
    }
  })
})
