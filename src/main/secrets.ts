import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { app, safeStorage } from 'electron'

// The OpenRouter API key. It is encrypted with the operating system's credential store
// (DPAPI on Windows, Keychain on macOS, the desktop keyring on Linux) and never written to
// disk in plain text. The interface can set or remove it but cannot read it back.

let sessionKey: string | null = null

function keyFile(): string {
  return join(app.getPath('userData'), 'openrouter.key')
}

/** The stored key, or `null`. Call only after the app is ready. */
export function loadKey(): string | null {
  if (sessionKey) return sessionKey
  try {
    if (!existsSync(keyFile()) || !safeStorage.isEncryptionAvailable()) return null
    sessionKey = safeStorage.decryptString(readFileSync(keyFile()))
    return sessionKey
  } catch (err) {
    console.error('could not read the stored API key:', err)
    return null
  }
}

/**
 * Stores the key, or removes it when `key` is empty. Returns whether it will survive a
 * restart: without a system credential store it is kept for this session only.
 */
export function saveKey(key: string): { persisted: boolean } {
  const trimmed = key.trim()
  if (!trimmed) {
    sessionKey = null
    rmSync(keyFile(), { force: true })
    return { persisted: true }
  }
  sessionKey = trimmed
  if (!safeStorage.isEncryptionAvailable()) return { persisted: false }
  writeFileSync(keyFile(), safeStorage.encryptString(trimmed))
  return { persisted: true }
}
