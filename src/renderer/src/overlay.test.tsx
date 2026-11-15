// @vitest-environment jsdom
import { act } from '@testing-library/react'
import { beforeAll, describe, expect, it } from 'vitest'
import { type FakeCore, fakeCore } from './test/fake-core'

// The listening indicator is its own small page. It mounts itself when imported, so the
// simulated engine and the root element have to exist first.
let core: FakeCore

beforeAll(async () => {
  core = fakeCore()
  window.stt = core.api
  document.body.innerHTML = '<div id="root"></div>'
  await act(async () => {
    await import('./overlay')
  })
})

const text = (): string => document.getElementById('root')!.textContent ?? ''
const pill = (): Element => document.querySelector('.pill')!

describe('listening indicator', () => {
  it('says what is happening in each phase', () => {
    act(() => core.emit('status', { phase: 'recording' }))
    expect(text()).toBe('Listening')
    expect(pill().className).toContain('pill-recording')

    act(() => core.emit('status', { phase: 'recording', message: 'computer audio' }))
    expect(text()).toBe('Hearing')

    act(() => core.emit('status', { phase: 'transcribing' }))
    expect(text()).toBe('Writing')

    act(() => core.emit('status', { phase: 'rewriting' }))
    expect(text()).toBe('Polishing')

    act(() => core.emit('status', { phase: 'done' }))
    expect(text()).toBe('Done')
  })

  it('shows the reason when something went wrong', () => {
    act(() => core.emit('status', { phase: 'error', message: 'Download a speech model first' }))
    expect(text()).toBe('Download a speech model first')
    act(() => core.emit('status', { phase: 'error' }))
    expect(text()).toBe('Something went wrong')
  })

  it('draws the voice level as bars that never vanish and never overflow', () => {
    act(() => core.emit('status', { phase: 'recording' }))
    const bars = (): string[] => [...document.querySelectorAll<HTMLElement>('.pill-bars i')].map((b) => b.style.height)
    expect(bars()).toHaveLength(13)
    expect(new Set(bars())).toEqual(new Set(['12%']))

    act(() => core.emit('level', 0.05))
    act(() => core.emit('level', 5))
    const heights = bars().map((h) => Number.parseFloat(h))
    expect(heights.at(-1)).toBe(100)
    expect(heights.at(-2)!).toBeGreaterThan(12)
    expect(heights.at(-2)!).toBeLessThan(100)
    expect(Math.min(...heights)).toBe(12)
  })

  it('starts the bars from nothing on each new recording', () => {
    act(() => core.emit('level', 1))
    act(() => core.emit('status', { phase: 'recording' }))
    const heights = [...document.querySelectorAll<HTMLElement>('.pill-bars i')].map((b) => b.style.height)
    expect(new Set(heights)).toEqual(new Set(['12%']))
  })

  it('is empty when idle', () => {
    act(() => core.emit('status', { phase: 'idle' }))
    expect(text()).toBe('')
  })
})
