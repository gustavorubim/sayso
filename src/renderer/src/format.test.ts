import { describe, expect, it } from 'vitest'
import { bytes, errorText, languageName, shortcutFromEvent, shortcutKeys, timeAgo } from './format'

/** The parts of a keydown event that `shortcutFromEvent` reads. */
function key(code: string, modifiers: Partial<Record<'ctrl' | 'alt' | 'shift' | 'meta', boolean>> = {}): KeyboardEvent {
  return {
    code,
    ctrlKey: Boolean(modifiers.ctrl),
    altKey: Boolean(modifiers.alt),
    shiftKey: Boolean(modifiers.shift),
    metaKey: Boolean(modifiers.meta)
  } as KeyboardEvent
}

describe('shortcutFromEvent', () => {
  it('builds an accelerator the core can parse', () => {
    expect(shortcutFromEvent(key('Space', { ctrl: true, shift: true }))).toBe('Ctrl+Shift+Space')
    expect(shortcutFromEvent(key('KeyH', { ctrl: true, alt: true, shift: true }))).toBe('Ctrl+Alt+Shift+KeyH')
    expect(shortcutFromEvent(key('Digit1', { meta: true }))).toBe('Super+Digit1')
  })

  it('waits while only modifier keys are held', () => {
    expect(shortcutFromEvent(key('ControlLeft', { ctrl: true }))).toBeNull()
    expect(shortcutFromEvent(key('ShiftRight', { ctrl: true, shift: true }))).toBeNull()
  })

  it('refuses a bare key, which would fire on every keystroke', () => {
    expect(shortcutFromEvent(key('KeyA'))).toBeNull()
    expect(shortcutFromEvent(key('Space'))).toBeNull()
  })

  it('allows a function key on its own', () => {
    expect(shortcutFromEvent(key('F8'))).toBe('F8')
    expect(shortcutFromEvent(key('F12', { shift: true }))).toBe('Shift+F12')
  })
})

describe('shortcutKeys', () => {
  it('turns key codes into what is printed on the key', () => {
    const keys = shortcutKeys('Ctrl+Shift+KeyD')
    expect(keys).toHaveLength(3)
    expect(keys[2]).toBe('D')
    expect(shortcutKeys('Ctrl+Digit7').at(-1)).toBe('7')
    expect(shortcutKeys('Alt+Space').at(-1)).toBe('Space')
    expect(shortcutKeys('F9')).toEqual(['F9'])
  })

  it('ignores empty parts', () => {
    expect(shortcutKeys('')).toEqual([])
  })
})

describe('bytes', () => {
  it('uses megabytes below a gigabyte and gigabytes above', () => {
    expect(bytes(208_977_568)).toBe('209 MB')
    expect(bytes(1_036_613_791)).toBe('1.04 GB')
  })
})

describe('languageName', () => {
  it('names languages and the automatic option', () => {
    expect(languageName('auto')).toBe('Detect automatically')
    expect(languageName('pt')).toBe('Portuguese')
  })
})

describe('timeAgo', () => {
  it('describes recent times relatively', () => {
    expect(timeAgo(Date.now() - 5_000)).toBe('just now')
    expect(timeAgo(Date.now() - 5 * 60_000)).toBe('5 min ago')
    expect(timeAgo(Date.now() - 3 * 3_600_000)).toBe('3 h ago')
  })
})

describe('errorText', () => {
  it("removes Electron's IPC prefix", () => {
    const err = new Error("Error invoking remote method 'core:call': Error: Could not register Ctrl+Alt+Space.")
    expect(errorText(err)).toBe('Could not register Ctrl+Alt+Space.')
  })

  it('passes other errors through', () => {
    expect(errorText(new Error('plain'))).toBe('plain')
    expect(errorText('text')).toBe('text')
  })
})
