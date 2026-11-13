import { writeFileSync } from 'node:fs'
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
      'the interface cannot start a recording',
      () => waitFor(mainWindow, "window.stt.call('toggle_recording').then(() => false, () => true)", 5_000)
    ]
  ]

  const results: { name: string; ok: boolean; detail?: string }[] = []
  for (const [name, run] of checks) {
    try {
      const outcome = await run()
      results.push(outcome === true ? { name, ok: true } : { name, ok: false, detail: String(outcome) })
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
