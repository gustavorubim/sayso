// Starts the packaged app in self-check mode and reports what it found.
//
//   npm run core:release && npm run build && node scripts/package.mjs --dir --publish never
//   node scripts/smoke-test.mjs
//
// The checks themselves live in src/main/smoke.ts. This script finds the unpacked app that
// electron-builder produced, runs it with a throwaway data folder, and fails if the app
// crashes, hangs, or reports a failed check.
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const TIMEOUT_MS = 180_000

function findApp() {
  const dist = 'dist'
  if (!existsSync(dist)) return null
  const dirs = readdirSync(dist)
  if (process.platform === 'win32') {
    const exe = join(dist, 'win-unpacked', 'Sayso.exe')
    return existsSync(exe) ? exe : null
  }
  if (process.platform === 'darwin') {
    for (const dir of dirs.filter((d) => d.startsWith('mac'))) {
      const exe = join(dist, dir, 'Sayso.app', 'Contents', 'MacOS', 'Sayso')
      if (existsSync(exe)) return exe
    }
    return null
  }
  for (const dir of dirs.filter((d) => d.startsWith('linux') && d.endsWith('unpacked'))) {
    const exe = join(dist, dir, 'sayso')
    if (existsSync(exe)) return exe
  }
  return null
}

const exe = findApp()
if (!exe) {
  console.error('No unpacked app found in dist/. Build it first: node scripts/package.mjs --dir --publish never')
  process.exit(2)
}

// With SAYSO_SMOKE_DATA pointing at a folder prepared by `npm run models:fetch`, the app also
// transcribes a real recording with a real model. Without it, that one check is skipped.
const modelData = process.env.SAYSO_SMOKE_DATA
const SMOKE_MODEL = 'canary-180m-flash'
const recording = resolve('core/tests/audio/clips/clip-02.wav')
if (modelData && !existsSync(join(modelData, 'models', SMOKE_MODEL))) {
  console.error(`SAYSO_SMOKE_DATA is set, but ${SMOKE_MODEL} is not in ${join(modelData, 'models')}.`)
  process.exit(2)
}
const dataDir = modelData ?? mkdtempSync(join(tmpdir(), 'sayso-smoke-'))
const audio = modelData ? { SAYSO_SMOKE_AUDIO: recording, SAYSO_SMOKE_MODEL: SMOKE_MODEL } : {}
const resultFile = join(mkdtempSync(join(tmpdir(), 'sayso-smoke-result-')), 'result.json')
// Chromium's sandbox cannot start on CI's Linux containers; everything else runs as shipped.
const args = process.platform === 'linux' && process.env.CI ? ['--no-sandbox'] : []

console.log(`Starting ${exe}`)
const child = spawn(exe, args, {
  env: { ...process.env, ...audio, SAYSO_SMOKE_TEST: '1', SAYSO_USER_DATA: dataDir, SAYSO_SMOKE_RESULT: resultFile },
  stdio: ['ignore', 'pipe', 'pipe']
})
let output = ''
child.stdout.on('data', (d) => (output += d))
child.stderr.on('data', (d) => (output += d))

const timer = setTimeout(() => {
  console.error(`The app did not finish its self-check within ${TIMEOUT_MS / 1000} s.`)
  child.kill('SIGKILL')
}, TIMEOUT_MS)

child.on('error', (err) => {
  clearTimeout(timer)
  console.error(`The app could not be started: ${err.message}`)
  process.exit(1)
})

child.on('exit', (code, signal) => {
  clearTimeout(timer)
  let report = null
  try {
    report = JSON.parse(readFileSync(resultFile, 'utf8'))
  } catch {
    // no report: the app died before finishing
  }
  if (!modelData) rmSync(dataDir, { recursive: true, force: true })

  if (!report) {
    console.error(`The app exited (code ${code}, signal ${signal}) without reporting. Its output:`)
    console.error(output.trim().split('\n').slice(-40).join('\n') || '(none)')
    process.exit(1)
  }
  console.log(`Sayso ${report.version} on ${report.platform}`)
  for (const r of report.results) {
    const mark = !r.ok ? 'FAIL' : r.detail ? 'skip' : 'ok  '
    console.log(`  ${mark}  ${r.name}${r.detail ? ` — ${r.detail}` : ''}`)
  }
  if (!report.ok || code !== 0) {
    console.error('\nSmoke test failed.')
    process.exit(1)
  }
  console.log('\nSmoke test passed.')
})
