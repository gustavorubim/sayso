import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { type BrowserWindow, app, nativeImage } from 'electron'
import type { ModelInfo, Settings } from '../shared/types'
import type { Core } from './core'

// A self-check of the packaged app, run by CI (scripts/smoke-test.mjs).
//
// Unit tests cannot see packaging mistakes: a file left out of the installer, a binary the
// operating system refuses to run, a preload script that fails to load. This starts the
// real app, checks the pieces that have broken before, writes the result to a file and
// exits. It is active only when SAYSO_SMOKE_TEST is set, and it uses a throwaway data
// folder, so it never touches a user's settings or opens the microphone.

export const SMOKE_TEST = Boolean(process.env['SAYSO_SMOKE_TEST'])

interface Context {
  core: Core
  mainWindow: BrowserWindow
  overlay: BrowserWindow
  iconPaths: Record<string, string>
}

type Check = [name: string, run: () => Promise<boolean | string>]

/** Runs `script` in a page, retrying until it returns true or time runs out. */
async function waitFor(win: BrowserWindow, script: string, timeoutMs = 20_000): Promise<boolean> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    try {
      if (await win.webContents.executeJavaScript(script, true)) return true
    } catch {
      // the page is still loading
    }
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  return false
}

export async function runSmokeTest({ core, mainWindow, overlay, iconPaths }: Context): Promise<void> {
  const checks: Check[] = [
    [
      'the speech engine starts and answers',
      async () => {
        const settings = await core.call<Settings>('get_settings')
        return typeof settings.shortcut === 'string' && settings.shortcut.length > 0
      }
    ],
    [
      'the model catalog is available',
      async () => {
        const models = await core.call<ModelInfo[]>('list_models')
        return models.length >= 7 || `only ${models.length} models`
      }
    ],
    ...Object.entries(iconPaths).map(([name, path]): Check => [
      `the ${name} icon is packaged`,
      async () => !nativeImage.createFromPath(path).isEmpty() || `empty image at ${path}`
    ]),
    ['the main window renders', () => waitFor(mainWindow, "document.querySelector('.shell') !== null")],
    ['the listening indicator loads', () => waitFor(overlay, "document.querySelector('#root') !== null")],
    [
      'the interface can reach the engine',
      () => waitFor(mainWindow, "window.stt.call('list_models').then((m) => m.length >= 7, () => false)")
    ],
    [
      'every screen opens',
      () =>
        waitFor(
          mainWindow,
          `(async () => {
            const names = [...document.querySelectorAll('.nav-item')].map((b) => b.textContent)
            if (names.length < 6) return false
            for (const name of names) {
              const item = [...document.querySelectorAll('.nav-item')].find((b) => b.textContent === name)
              if (!item) return false
              item.click()
              await new Promise((resolve) => setTimeout(resolve, 300))
              if (!document.querySelector('.page') || document.querySelector('.page-error')) return false
            }
            return true
          })()`
        )
    ],
    [
      'the packaged engine transcribes a recording',
      async () => {
        // Needs a downloaded model, so it only runs when the script provides one.
        const wav = process.env['SAYSO_SMOKE_AUDIO']
        const modelId = process.env['SAYSO_SMOKE_MODEL']
        if (!wav || !modelId) return 'skipped: no speech model was provided'
        if (!existsSync(wav)) return `recording not found: ${wav}`
        const settings = await core.call<Settings>('get_settings')
        await core.call('save_settings', { ...settings, model_id: modelId, language: 'en' })
        const transcript = await core.call<string>('transcribe_file', { path: wav })
        const normalise = (text: string): string[] =>
          text
            .toLowerCase()
            .replace(/[^a-z0-9' ]+/g, ' ')
            .split(/\s+/)
            .filter(Boolean)
        const expected = normalise(readFileSync(wav.replace(/\.wav$/, '.txt'), 'utf8'))
        const got = new Set(normalise(transcript))
        const found = expected.filter((word) => got.has(word)).length
        return found >= expected.length * 0.8 || `expected "${expected.join(' ')}", got "${transcript.trim()}"`
      }
    ],
    [
      'the interface cannot start a recording',
      () => waitFor(mainWindow, "window.stt.call('toggle_recording').then(() => false, () => true)", 5_000)
    ],
    [
      // Last, because it kills the engine: the app must bring it back by itself, with the
      // settings it had, and the interface must be able to use it again.
      'the engine comes back after a crash',
      async () => {
        const before = await core.call<Settings>('get_settings')
        const restarted = new Promise<void>((resolve) => core.once('started', () => resolve()))
        core.crashForTest()
        const inTime = await Promise.race([
          restarted.then(() => true),
          new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 15_000))
        ])
        if (!inTime) return 'the engine was not restarted within 15 s'
        const after = await core.call<Settings>('get_settings')
        if (JSON.stringify(after) !== JSON.stringify(before)) return 'settings changed across the restart'
        const reachable = await waitFor(
          mainWindow,
          "window.stt.call('list_models').then((m) => m.length >= 7, () => false)",
          10_000
        )
        return reachable || 'the interface could not reach the restarted engine'
      }
    ]
  ]

  const results: { name: string; ok: boolean; detail?: string }[] = []
  for (const [name, run] of checks) {
    try {
      const outcome = await run()
      if (outcome === true) results.push({ name, ok: true })
      else if (typeof outcome === 'string' && outcome.startsWith('skipped')) {
        results.push({ name, ok: true, detail: outcome })
      } else results.push({ name, ok: false, detail: String(outcome) })
    } catch (err) {
      results.push({ name, ok: false, detail: err instanceof Error ? err.message : String(err) })
    }
  }

  const ok = results.every((r) => r.ok)
  const report = JSON.stringify({ ok, version: app.getVersion(), platform: process.platform, results }, null, 2)
  const out = process.env['SAYSO_SMOKE_RESULT']
  if (out) writeFileSync(out, report)
  console.log(report)
  core.stop()
  app.exit(ok ? 0 : 1)
}
