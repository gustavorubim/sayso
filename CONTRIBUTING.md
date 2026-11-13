# Contributing

Thanks for helping. This page covers setup, how changes get merged, and how releases are cut.

## Setup

You need Node.js 22+, Rust (stable), and on Linux:
`sudo apt install pkg-config libasound2-dev libx11-dev libxi-dev libxtst-dev libxdo-dev`.

```bash
npm install
npm run dev
```

| Command | What it does |
| --- | --- |
| `npm run dev` | Builds the Rust core (debug) and starts the app with hot reload |
| `npm run typecheck` | Type-checks the Electron and React code |
| `npm run core:test` | Runs the Rust unit tests |
| `npm run build` | Builds the Electron bundles into `out/` |
| `npm run dist` | Builds an installer for your system into `dist/` |
| `npm run icons` | Regenerates the app icons |

For Rust formatting and lints, from `core/`: `cargo fmt` and `cargo clippy --all-targets -- -D warnings`.

**Windows:** if C dependencies fail to compile with "Cannot open include file: 'stddef.h'", you have a global `CC`
environment variable pointing at `cl.exe`. Remove it, or unset it for the session.

**"Electron uninstall" on `npm run dev`:** npm skipped Electron's download step. Run
`node node_modules/electron/install.js` once.

## Where things live

| Path | Contents |
| --- | --- |
| `core/src/` | Rust core: hotkey, audio, speech engine, downloads, paste |
| `core/src/models.rs` | The model catalog |
| `src/main/` | Electron main process: windows, tray, core process |
| `src/renderer/` | React interface and the listening indicator |
| `src/shared/types.ts` | Types for the core protocol, shared by all TypeScript code |

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how the pieces talk to each other.

## Proposing a change

1. **Open an issue first** for anything larger than a small fix, so the approach can be agreed before you write code.
   Bug fixes, typo fixes and documentation improvements can go straight to a pull request.
2. **Branch from `main`** using `type/short-description`, for example `feat/streaming-preview` or
   `fix/paste-on-wayland`.
3. **Keep the pull request focused** on one change. Two unrelated fixes are two pull requests.
4. **Add or update tests** for Rust logic that can be tested without hardware (text processing, catalog, parsing).
5. **Note user-facing changes** under "Unreleased" in [CHANGELOG.md](CHANGELOG.md).

## Pull request policy

- **`main` is protected.** Nobody pushes to it directly; every change arrives through a pull request.
- **CI must pass.** The "CI passed" check covers formatting, lints and tests on Windows, macOS and Linux, plus the
  TypeScript type-check and build.
- **One approving review** from a code owner is required. Authors don't approve their own work.
- **Squash merge only.** The pull request title becomes the single commit on `main`.
- **The title must be a [Conventional Commit](https://www.conventionalcommits.org)**; a check enforces this.
  It ends up in the release notes, so write it for users:

  | Prefix | Use for | Example |
  | --- | --- | --- |
  | `feat:` | A new capability | `feat: add Parakeet v3 model` |
  | `fix:` | A bug fix | `fix: restore clipboard after paste on macOS` |
  | `perf:` | Faster or lighter, same behaviour | `perf: load models off the hotkey thread` |
  | `docs:` | Documentation only | `docs: explain Wayland limitation` |
  | `refactor:` `test:` `build:` `ci:` `chore:` | Internal work | `ci: cache cargo registry` |

  Add `!` for a breaking change: `feat!: move settings to a new location`.
- **Say how you tested it** in the description: which system, which model, which app you dictated into. Audio and
  paste behaviour differs per platform and CI cannot check it.
- **New dependencies need a reason** in the description. Prefer the standard library and what is already in the tree.
- Dependabot pull requests follow the same rules.

### Recommended repository settings

These are set in GitHub, not in the repository, so a maintainer applies them once
(Settings → Rules → Rulesets → New branch ruleset for `main`):

- Require a pull request before merging, with 1 approval and review from code owners
- Require status checks to pass: `CI passed` and `Conventional Commit title`
- Require linear history; block force pushes
- Under Settings → General: allow squash merging only, and default the commit message to the pull request title

## Adding a speech model

1. Find an ONNX export that [sherpa-onnx](https://k2-fsa.github.io/sherpa/onnx/pretrained_models/index.html) supports,
   hosted on Hugging Face. Prefer `int8` quantized files.
2. Add a `ModelDef` to `CATALOG` in `core/src/models.rs` with the exact file names and byte sizes from the repository's
   Files tab, and the matching `Kind`.
3. If it is a new architecture, add a `Kind` variant and wire it up in `Engine::load` in `core/src/engine.rs`.
4. Run `npm run core:test`, then download and dictate with it in `npm run dev`.
5. Add it to the table in `README.md` and state the model's license in the pull request.

## Releases

Maintainers cut releases; the pipeline does the building.

1. `npm run version:set -- 0.2.0` updates `package.json`, the lock files and `core/Cargo.toml`.
2. Move the "Unreleased" notes in `CHANGELOG.md` under a `0.2.0` heading.
3. Open a pull request titled `chore: release 0.2.0` and merge it.
4. Tag the merge commit: `git tag v0.2.0 && git push origin v0.2.0`.
5. The **Release** workflow builds installers for Windows, macOS (Apple silicon and Intel) and Linux and attaches them
   to a draft release with generated notes.
6. Try an installer, edit the notes if needed, and publish the draft.

Versions follow [Semantic Versioning](https://semver.org). Signing is optional and switched on by adding the
`CSC_LINK`, `CSC_KEY_PASSWORD`, `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD` and `APPLE_TEAM_ID` repository secrets.

## Conduct and security

Be kind; see the [Code of Conduct](CODE_OF_CONDUCT.md). Report vulnerabilities privately as described in
[SECURITY.md](SECURITY.md).
