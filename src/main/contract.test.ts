import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { DEFAULT_SETTINGS } from '../renderer/src/test/fake-core'

// The engine (Rust) and the interface (TypeScript) each have their own definition of the
// settings. tests/contract/settings-keys.json is the list both must match: the Rust side
// checks it in core/src/settings.rs, this file checks the TypeScript side. Add a setting
// to one side only and one of the two tests fails.

const contract: string[] = JSON.parse(readFileSync('tests/contract/settings-keys.json', 'utf8'))

function interfaceKeys(name: string): string[] {
  const source = readFileSync('src/shared/types.ts', 'utf8')
  const body = source.match(new RegExp(`export interface ${name} \\{([\\s\\S]*?)\\n\\}`))?.[1]
  if (!body) throw new Error(`interface ${name} not found in src/shared/types.ts`)
  return [...body.matchAll(/^ {2}(\w+)\??:/gm)].map((m) => m[1]).sort()
}

describe('settings contract', () => {
  it('the Settings type has exactly the fields the engine sends', () => {
    expect(interfaceKeys('Settings')).toEqual(contract)
  })

  it('the test double has them too', () => {
    expect(Object.keys(DEFAULT_SETTINGS).sort()).toEqual(contract)
  })

  it('the contract is sorted and has no duplicates', () => {
    expect([...new Set(contract)].sort()).toEqual(contract)
  })
})

describe('protocol types', () => {
  it('every engine method the interface may call is typed', () => {
    const main = readFileSync('src/main/index.ts', 'utf8')
    const allowed = [
      ...(main.match(/const RENDERER_METHODS = new Set\(\[([\s\S]*?)\]\)/)?.[1] ?? '').matchAll(/'(\w+)'/g)
    ]
      .map((m) => m[1])
      .sort()
    expect(allowed.length).toBeGreaterThan(10)
    const typed = interfaceKeys('CoreMethods')
    for (const method of allowed) expect(typed, `${method} is allowed but not typed`).toContain(method)
  })

  it('methods that start a recording or read files stay out of reach of the interface', () => {
    const main = readFileSync('src/main/index.ts', 'utf8')
    const allowed = main.match(/const RENDERER_METHODS = new Set\(\[([\s\S]*?)\]\)/)?.[1] ?? ''
    for (const method of ['toggle_recording', 'toggle_capture', 'transcribe_file', 'set_api_key']) {
      expect(allowed, `${method} must not be callable from the interface`).not.toContain(`'${method}'`)
    }
  })
})
