// Runs electron-builder, after deciding how the macOS app gets signed.
//
//   node scripts/package.mjs --publish never
//
// Everything after the script name is passed to electron-builder unchanged.
//
// Packaging breaks the signature Electron ships with: the app is renamed, its Info.plist is
// rewritten and its fuses are flipped. macOS will not open a downloaded app in that state and
// reports it as "damaged". With a Developer ID certificate electron-builder signs the app
// again and notarizes it. Without one it skips signing and leaves the broken signature in
// place, so this script asks for an ad hoc signature instead. That needs no certificate, and
// macOS then lets people allow the app under Privacy & Security rather than refusing it.
import { execFileSync, spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'

const ADHOC_ENTITLEMENTS = 'build/entitlements.mac.adhoc.plist'

// The same places electron-builder looks: a certificate handed over in the environment, or
// one already in the keychain.
function hasDeveloperIdCertificate() {
  if (process.env.CSC_LINK || process.env.CSC_NAME) return true
  try {
    const identities = execFileSync('security', ['find-identity', '-v', '-p', 'codesigning'], { encoding: 'utf8' })
    return identities.includes('Developer ID Application')
  } catch {
    return false
  }
}

const args = process.argv.slice(2)
if (process.platform === 'darwin' && !hasDeveloperIdCertificate()) {
  console.log('No Developer ID certificate found: the macOS app will be signed ad hoc.')
  args.push(
    '-c.mac.identity=-',
    `-c.mac.entitlements=${ADHOC_ENTITLEMENTS}`,
    `-c.mac.entitlementsInherit=${ADHOC_ENTITLEMENTS}`
  )
}

const cli = createRequire(import.meta.url).resolve('electron-builder/cli.js')
const result = spawnSync(process.execPath, [cli, ...args], { stdio: 'inherit' })
if (result.error) throw result.error
process.exit(result.status ?? 1)
