import { type ChildProcessWithoutNullStreams, spawn } from 'node:child_process'
import { EventEmitter } from 'node:events'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { createInterface } from 'node:readline'
import { app } from 'electron'

const BINARY = process.platform === 'win32' ? 'sayso-core.exe' : 'sayso-core'

function corePath(): string {
  if (app.isPackaged) return join(process.resourcesPath, 'bin', BINARY)
  const root = app.getAppPath()
  const release = join(root, 'core', 'target', 'release', BINARY)
  const debug = join(root, 'core', 'target', 'debug', BINARY)
  return existsSync(debug) ? debug : release
}

interface Pending {
  resolve: (value: unknown) => void
  reject: (reason: Error) => void
}

/**
 * The Rust core, running as a child process. Requests and events are JSON
 * lines over stdio (see core/src/rpc.rs).
 *
 * Emits `event` (name, payload) for every core event, and `started` each time the process starts.
 */
export class Core extends EventEmitter {
  private child: ChildProcessWithoutNullStreams | null = null
  private nextId = 1
  private pending = new Map<number, Pending>()
  private stopping = false
  private restarts = 0

  start(): void {
    const path = corePath()
    if (!existsSync(path)) {
      this.emit('event', 'core-exit', `Core binary not found at ${path}. Run "npm run core:build".`)
      return
    }
    const child = spawn(path, ['--data-dir', app.getPath('userData')], {
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe']
    })
    this.child = child
    // Lets the shell hand over state the core keeps only in memory.
    setImmediate(() => this.emit('started'))

    createInterface({ input: child.stdout }).on('line', (line) => this.onLine(line))
    createInterface({ input: child.stderr }).on('line', (line) => console.error(`[core] ${line}`))

    child.on('error', (err) => console.error('[core] failed to start:', err))
    child.on('exit', (code) => {
      this.child = null
      for (const p of this.pending.values()) p.reject(new Error('The speech engine stopped.'))
      this.pending.clear()
      if (this.stopping) return
      this.emit('event', 'core-exit', `The speech engine stopped unexpectedly (code ${code}).`)
      if (this.restarts++ < 3) setTimeout(() => this.start(), 1000)
    })
  }

  private onLine(line: string): void {
    let msg: { id?: number; result?: unknown; error?: string; event?: string; payload?: unknown }
    try {
      msg = JSON.parse(line)
    } catch {
      console.error('[core] unparseable line:', line)
      return
    }
    if (msg.event) {
      this.emit('event', msg.event, msg.payload)
      return
    }
    if (typeof msg.id !== 'number') return
    const p = this.pending.get(msg.id)
    if (!p) return
    this.pending.delete(msg.id)
    if (msg.error !== undefined) p.reject(new Error(msg.error))
    else p.resolve(msg.result)
  }

  call<T = unknown>(method: string, params?: unknown): Promise<T> {
    const child = this.child
    if (!child) return Promise.reject(new Error('The speech engine is not running.'))
    const id = this.nextId++
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject })
      child.stdin.write(`${JSON.stringify({ id, method, params: params ?? null })}\n`)
    })
  }

  stop(): void {
    this.stopping = true
    this.child?.stdin.end()
    this.child?.kill()
  }
}
