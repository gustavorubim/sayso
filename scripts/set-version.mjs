// Sets the app version everywhere it is recorded.
//   npm run version:set -- 0.2.0
import { execSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'

const version = process.argv[2]
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version ?? '')) {
  console.error('Usage: npm run version:set -- <major.minor.patch>')
  process.exit(1)
}

execSync(`npm version ${version} --no-git-tag-version --allow-same-version`, { stdio: 'inherit' })

const cargoPath = 'core/Cargo.toml'
const cargo = readFileSync(cargoPath, 'utf8')
// Only the first `version = ` line, which belongs to [package].
writeFileSync(cargoPath, cargo.replace(/^version = ".*"$/m, `version = "${version}"`))
execSync('cargo update --manifest-path core/Cargo.toml --package shortcut-to-text-core', { stdio: 'inherit' })

console.log(`\nVersion set to ${version}. Next:`)
console.log(`  1. Add a "${version}" section to CHANGELOG.md`)
console.log(`  2. Open a pull request titled "chore: release ${version}"`)
console.log(`  3. After it merges: git tag v${version} && git push origin v${version}`)
