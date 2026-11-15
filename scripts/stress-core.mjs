// Stresses the speech engine process the way the app talks to it: over its standard input
// and output, from outside. It uses the engine inside the packaged app when there is one in
// dist/, so what is stressed is what ships.
//
//   npm run stress:core
//
// What it does:
//   1. Floods the engine with thousands of requests mixed with lines that are not requests
//      at all, and checks every request gets exactly one answer.
//   2. Kills the engine in the middle of saving settings, over and over, and checks it
//      always starts again with a settings file it can read.
//   3. Transcribes the same recording repeatedly and checks the text never changes
//      (only when SAYSO_SMOKE_DATA points at a folder with the canary-180m-flash model).
//   4. Closes the engine's input and checks it exits by itself, as it must when the app quits.
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createInterface } from 'node:readline'

const BINARY = process.platform === 'win32' ? 'sayso-core.exe' : 'sayso-core'
const MODEL = 'canary-180m-flash'

function findCore() {
  // SAYSO_CORE picks a specific engine, for example a fresh debug build while developing.
  if (process.env.SAYSO_CORE) return existsSync(process.env.SAYSO_CORE) ? process.env.SAYSO_CORE : null
  const candidates = []
  if (existsSync('dist')) {
    for (const dir of readdirSync('dist')) {
      candidates.push(join('dist', dir, 'resources', 'bin', BINARY))
      candidates.push(join('dist', dir, 'Sayso.app', 'Contents', 'Resources', 'bin', BINARY))
    }
  }
  candidates.push(join('core', 'target', 'release', BINARY), join('core', 'target', 'debug', BINARY))
  return candidates.find((path) => existsSync(path)) ?? null
}

/** One running engine process. */
class Engine {
  constructor(binary, dataDir) {
    this.child = spawn(binary, ['--data-dir', dataDir], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true })
    this.answers = new Map() // id -> number of answers
    this.waiting = new Map() // id -> resolve
    this.nextId = 1
    this.stderr = ''
    this.exited = new Promise((done) => this.child.on('exit', (code, signal) => done({ code, signal })))
    this.child.stdin.on('error', () => {}) // writing to a killed engine is expected here
    this.child.stderr.on('data', (d) => (this.stderr = (this.stderr + d).slice(-4000)))
    createInterface({ input: this.child.stdout }).on('line', (line) => {
      let message
      try {
        message = JSON.parse(line)
      } catch {
        throw new Error(`The engine wrote a line that is not JSON: ${line.slice(0, 200)}`)
      }
      if (typeof message.id !== 'number') return // an event, or the answer to a junk request
      this.answers.set(message.id, (this.answers.get(message.id) ?? 0) + 1)
      this.waiting.get(message.id)?.(message)
      this.waiting.delete(message.id)
    })
  }

  write(text) {
    this.child.stdin.write(text)
  }

  /** Sends a request without waiting; returns its id. */
  send(method, params = null) {
    const id = this.nextId++
    this.write(`${JSON.stringify({ id, method, params })}\n`)
    return id
  }

  /** Sends a request and waits for its answer. */
  call(method, params = null, timeoutMs = 120_000) {
    const id = this.send(method, params)
    return new Promise((done, fail) => {
      const timer = setTimeout(
        () => fail(new Error(`${method} was not answered within ${timeoutMs / 1000} s`)),
        timeoutMs
      )
      this.waiting.set(id, (message) => {
        clearTimeout(timer)
        if (message.error !== undefined) fail(new Error(`${method}: ${message.error}`))
        else done(message.result)
      })
      this.exited.then(({ code }) => {
        clearTimeout(timer)
        fail(new Error(`the engine exited (code ${code}) before answering ${method}\n${this.stderr}`))
      })
    })
  }

  kill() {
    this.child.kill('SIGKILL')
    return this.exited
  }
}

function check(condition, message) {
  if (!condition) throw new Error(message)
}

async function flood(binary) {
  const dir = mkdtempSync(join(tmpdir(), 'sayso-stress-'))
  const engine = new Engine(binary, dir)
  const settings = await engine.call('get_settings')

  const junk = [
    'this is not json\n',
    '{"id": 1, "method": \n',
    '[]\n',
    '42\n',
    '"just a string"\n',
    '{}\n',
    '{"method": 7}\n',
    '{"id": {"nested": true}, "method": "get_settings"}\n',
    '\n\n\n',
    '\u0000\u0001\u0002 binary ￿\n',
    `{"id": "big", "method": "no_such_method", "params": "${'x'.repeat(2_000_000)}"}\n`,
    `${'['.repeat(100_000)}\n`
  ]
  const methods = ['get_settings', 'list_models', 'get_status', 'get_history', 'get_default_settings', 'no_such_method']
  const ids = []
  const total = 6000
  const started = Date.now()
  for (let i = 0; i < total; i++) {
    if (i % 10 === 0) ids.push(engine.send('save_settings', { ...settings, remove_fillers: i % 20 === 0 }))
    else ids.push(engine.send(methods[i % methods.length]))
    if (i % 250 === 0) engine.write(junk[(i / 250) % junk.length])
  }
  // A request sent after everything else: when it is answered, the rest must have been too.
  const after = await engine.call('get_settings', null, 180_000)
  check(typeof after.shortcut === 'string', 'the engine did not return settings after the flood')
  const unanswered = ids.filter((id) => engine.answers.get(id) !== 1)
  check(
    unanswered.length === 0,
    `${unanswered.length} of ${total} requests did not get exactly one answer (first: ${unanswered[0]})`
  )
  JSON.parse(readFileSync(join(dir, 'settings.json'), 'utf8'))
  const seconds = (Date.now() - started) / 1000

  // The app quits by closing the engine's input. It must go by itself, and promptly.
  engine.child.stdin.end()
  const exit = await Promise.race([engine.exited, new Promise((done) => setTimeout(() => done(null), 10_000))])
  if (!exit) await engine.kill()
  check(exit !== null, 'the engine did not exit within 10 s of its input being closed')
  check(exit.code === 0, `the engine exited with code ${exit.code} when its input was closed`)
  rmSync(dir, { recursive: true, force: true })
  return `${total} requests and ${Math.ceil(total / 250)} junk lines answered in ${seconds.toFixed(1)} s; exits when its input closes`
}

async function killLoop(binary) {
  const dir = mkdtempSync(join(tmpdir(), 'sayso-stress-'))
  const rounds = 25
  const padding = 'a custom instruction '.repeat(400) // makes the file big enough to tear
  for (let round = 0; round < rounds; round++) {
    const engine = new Engine(binary, dir)
    // Whatever the last round left behind must be readable, by the engine and as a file.
    const settings = await engine.call('get_settings', null, 30_000)
    if (round > 0) {
      const onDisk = JSON.parse(readFileSync(join(dir, 'settings.json'), 'utf8'))
      check(
        onDisk.rewrite_prompt?.startsWith('round '),
        `round ${round}: the settings were lost when the engine was killed (the file went back to defaults)`
      )
      check(
        settings.rewrite_prompt === onDisk.rewrite_prompt,
        `round ${round}: the engine did not load what was on disk`
      )
    }
    await engine.call('save_settings', { ...settings, rewrite_prompt: `round ${round} ${padding}` })
    // Keep saving, and pull the plug at a different moment each round.
    for (let i = 0; i < 300; i++) {
      engine.send('save_settings', {
        ...settings,
        rewrite_prompt: `round ${round} save ${i} ${padding.repeat(1 + (i % 3))}`
      })
    }
    await new Promise((done) => setTimeout(done, round * 4))
    await engine.kill()
  }
  const leftovers = readdirSync(dir).filter((name) => name.endsWith('.tmp'))
  check(leftovers.length <= rounds, `${leftovers.length} temporary files piled up`)
  rmSync(dir, { recursive: true, force: true })
  return `killed ${rounds} times while saving; the settings were readable and kept every time`
}

async function transcribeRepeatedly(binary, dataDir) {
  const recording = resolve('core/tests/audio/clips/clip-02.wav')
  const engine = new Engine(binary, dataDir)
  const settings = await engine.call('get_settings')
  await engine.call('save_settings', { ...settings, model_id: MODEL, language: 'en' })
  const first = await engine.call('transcribe_file', { path: recording })
  check(first.trim().split(/\s+/).length >= 5, `the transcript is too short: "${first}"`)
  const expected = { false: first.trim() }
  const rounds = 40
  const started = Date.now()
  for (let i = 0; i < rounds; i++) {
    // Other requests keep arriving while it works, as they do in the app.
    engine.send('get_status')
    engine.send('list_models')
    // Every fourth one goes through the write-as-you-speak path, which returns the text in
    // pieces. Those are capitalised piece by piece, so they are compared with each other.
    const live = i % 4 === 3
    const answer = await engine.call('transcribe_file', { path: recording, live })
    const text = (live ? answer.join('') : answer).trim()
    expected[live] ??= text
    check(text === expected[live], `transcription ${i + 1} differs:\n  before: ${expected[live]}\n  now:    ${text}`)
  }
  const seconds = (Date.now() - started) / 1000
  engine.child.stdin.end()
  await engine.exited
  return `${rounds} transcriptions in ${seconds.toFixed(1)} s, the same text every time`
}

const binary = findCore()
if (!binary) {
  console.error('No engine found. Build one first: npm run pack:dir (or npm run core:build).')
  process.exit(2)
}
console.log(`Stressing ${binary}`)

const modelData = process.env.SAYSO_SMOKE_DATA
const haveModel = modelData && existsSync(join(modelData, 'models', MODEL))
const stages = [
  ['a flood of requests', () => flood(binary)],
  ['being killed while saving', () => killLoop(binary)],
  [
    'transcribing over and over',
    () => (haveModel ? transcribeRepeatedly(binary, modelData) : 'skipped: SAYSO_SMOKE_DATA has no speech model')
  ]
]

let failed = false
for (const [name, run] of stages) {
  try {
    console.log(`  ok    ${name} — ${await run()}`)
  } catch (err) {
    failed = true
    console.log(`  FAIL  ${name} — ${err.message}`)
  }
}
if (failed) {
  console.error('\nEngine stress test failed.')
  process.exit(1)
}
console.log('\nEngine stress test passed.')
process.exit(0)
