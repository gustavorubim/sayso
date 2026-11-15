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
| `npm run check` | Everything CI checks on the interface: formatting, lint, types, unit tests |
| `npm run format` | Reformats the TypeScript and config files with Prettier |
| `npm run typecheck` | Type-checks the Electron and React code |
| `npm run test` | Runs the interface tests |
| `npm run core:test` | Runs the Rust unit tests |
| `npm run core:test:audio` | Runs the audio regression tests; needs speech models, see [Tests](#tests) |
| `npm run models:fetch -- --data-dir <folder> <model id>` | Downloads a model outside the app, checksums verified |
| `npm run build` | Builds the Electron bundles into `out/` |
| `npm run dist` | Builds an installer for your system into `dist/` |
| `npm run pack:dir` then `npm run smoke` | Builds the unpacked app and runs its self-check, as CI does |
| `npm run stress:core` | Floods, kills and restarts the engine process; see [Tests](#tests) |
| `npm run icons` | Regenerates the app icons |

For Rust formatting and lints, from `core/`: `cargo fmt` and `cargo clippy --all-targets -- -D warnings`.

**Windows:** if C dependencies fail to compile with "Cannot open include file: 'stddef.h'", you have a global `CC`
environment variable pointing at `cl.exe`. Remove it, or unset it for the session.

**"Electron uninstall" on `npm run dev`:** npm skipped Electron's download step. Run
`node node_modules/electron/install.js` once.

## Tests

There are five kinds, and a change usually needs the first two.

**Core unit tests** (`npm run core:test`) cover everything in the engine that needs no microphone or model: text
clean-up, the request protocol, settings, the model catalog, download checksums, rewrite safety. They take a
second. Add one for any logic you change.

**Interface tests** (`npm run test`) render every screen against a simulated engine
(`src/renderer/src/test/fake-core.ts`) and check what the interface asks the engine to do. If you add a control,
add a test that uses it.

**Audio regression tests** run real recordings (`core/tests/audio`) through real speech models and check
accuracy, very short recordings, long recordings, other sample rates, silence, and writing as you speak. They
are the only tests that can tell whether dictation got worse, so run them for any change to the engine, audio
handling, or model settings:

```bash
npm run models:fetch -- --data-dir .speech-models canary-180m-flash whisper-large-v3-turbo
```

```bash
SAYSO_TEST_DATA="$PWD/.speech-models" npm run core:test:audio
```

That is about 1.25 GB of models, downloaded once. CI runs exactly this on every pull request. To test other
models, download them into the same folder; every model found there is tested unless `SAYSO_TEST_MODELS` names
a subset.

**Smoke test** (`npm run pack:dir`, then `npm run smoke`) builds the real app, starts it, and has it check
itself. With `SAYSO_SMOKE_DATA=.speech-models` it also transcribes a recording through the packaged engine. Its
last check kills the engine and expects the app to bring it back.

**Stress tests** ask what happens when things are done many times, at once, or go wrong. They come in three
parts:

- The ones that need no model run with every `npm run core:test` (`core/src/stress_tests.rs`): requests from
  many threads at once, hundreds of malformed requests, a settings file read while it is being rewritten, and
  downloads that are cut off, corrupted, oversized or cancelled, against a small web server inside the test.
- The ones that need models dictate hundreds of times in a row and watch memory, switch models back and
  forth, transcribe a ten-minute recording, and feed the engine audio no microphone should produce. They take
  a few minutes, so CI runs them on every push to `main` and before every release, not on pull requests:

  ```bash
  SAYSO_TEST_DATA="$PWD/.speech-models" cargo test --manifest-path core/Cargo.toml stress_tests -- --ignored --test-threads=1 --nocapture
  ```

- `npm run stress:core` works on the engine from outside, the way the app does. It uses the engine inside the
  packaged app in `dist/` when there is one (set `SAYSO_CORE` to point it at another build). It sends thousands
  of requests mixed with junk, kills the engine 25 times in the middle of saving settings and checks they
  survive, and with `SAYSO_SMOKE_DATA` set transcribes the same recording 40 times. CI runs it after the smoke
  test on each system.

When a stress test finds something, fix the cause and keep the test. The first run of these found four real
faults: settings lost when the engine was killed mid-save, a crash on recordings shorter than one audio frame, a
request that could hang the engine forever on Windows, and an engine that was not restarted after its third
crash of the day.

Two rules that come from experience:

- **Do not change a speech-model setting on the strength of a few clips.** Run `sayso-core bench --eval` on a
  proper test set and the audio tests first. Version 0.4.0 shipped a Whisper setting that looked fine on two
  clips and nearly doubled its error rate.
- **When you add a setting,** add it to `tests/contract/settings-keys.json`. A test on each side fails until the
  engine and the interface agree.

## Where things live

| Path | Contents |
| --- | --- |
| `core/src/` | Rust core: hotkey, audio, speech engine, downloads, paste |
| `core/src/models.rs` | The model catalog |
| `src/main/` | Electron main process: windows, tray, core process |
| `src/renderer/` | React interface and the listening indicator |
| `src/shared/types.ts` | Types for the core protocol, shared by all TypeScript code |
| `core/tests/audio/` | Labelled recordings for the audio tests |
| `tests/contract/` | Facts the engine and the interface must agree on |

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how the pieces talk to each other.

## Proposing a change

1. **Open an issue first** for anything larger than a small fix, so the approach can be agreed before you write code.
   Bug fixes, typo fixes and documentation improvements can go straight to a pull request.
2. **Branch from `main`** using `type/short-description`, for example `feat/streaming-preview` or
   `fix/paste-on-wayland`.
3. **Keep the pull request focused** on one change. Two unrelated fixes are two pull requests.
4. **Add or update tests.** See [Tests](#tests): unit tests for logic, an interface test for a new control, and
   the audio tests for anything that touches recognition.
5. **Note user-facing changes** under "Unreleased" in [CHANGELOG.md](CHANGELOG.md).

## Pull request policy

- **`main` is protected.** Nobody pushes to it directly; every change arrives through a pull request.
- **CI must pass.** The "CI passed" check covers, for the Rust core, formatting, lints and tests; for the
  interface, formatting, lint, type-check and tests; the audio regression tests against real speech models;
  and a packaged-app test that builds the real app, starts it, has it transcribe a recording, and stresses its
  engine. Pull requests run on Linux and Windows; `main` and releases add macOS and the slow stress tests.
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
2. Add a `ModelDef` to `CATALOG` in `core/src/models.rs` with the repository's current commit hash as `revision`,
   and for each file its exact name, byte size and SHA-256 (the Files tab shows all three), plus the matching `Kind`.
3. If it is a new architecture, add a `Kind` variant and wire it up in `Engine::load` in `core/src/engine.rs`.
4. Run `npm run core:test`, then download and dictate with it in `npm run dev`.
5. Add it to the table in `README.md` and state the model's license in the pull request.

## Releases

Maintainers cut releases; the pipeline does the building.

1. `npm run version:set -- 0.2.0` updates `package.json`, the lock files and `core/Cargo.toml`.
2. Move the "Unreleased" notes in `CHANGELOG.md` under a `0.2.0` heading.
3. Open a pull request titled `chore: release 0.2.0` and merge it.
4. Tag the merge commit: `git tag v0.2.0 && git push origin v0.2.0`.
5. The **Release** workflow first runs the unit, audio and stress tests on the tagged commit. Then each system
   builds its installers once, and the app from that same build is started, checks itself, transcribes a
   recording and has its engine stressed. Only installers that passed are attached to a draft release with
   generated notes, so what is published is what was tested.
6. Try an installer, edit the notes if needed, and publish the draft.

### Rehearsing a release

To check that a release would succeed without making one, run the Release workflow by hand: Actions, Release,
"Run workflow", or `gh workflow run Release`. Everything above happens on the chosen branch, nothing is
published, and the installers are kept as workflow artifacts for three days. Do this after any change to the
workflows, the packaging configuration or the speech engine version.

### Beta releases

To try a build before everyone gets it, release it as a beta: `npm run version:set -- 0.5.0-beta.1`, then tag
`v0.5.0-beta.1`. The workflow publishes it as a GitHub pre-release. It does not become the "latest" release, and
the app's updater only offers betas to people already running one. When it holds up, set the version to `0.5.0`
and tag that.

### Dependency audit

The **Dependency audit** workflow runs every Monday and can be started by hand from the Actions tab. If a
dependency that ships in the app has a known vulnerability, it opens an issue labelled `security-audit`.

Versions follow [Semantic Versioning](https://semver.org). Signing is optional and switched on by adding the
`CSC_LINK`, `CSC_KEY_PASSWORD`, `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD` and `APPLE_TEAM_ID` repository secrets.

Without those secrets the macOS app is signed ad hoc (`scripts/package.mjs`). An ad hoc signature needs no
certificate and is enough for macOS to open the app once the person allows it under Privacy & Security; with no
signature at all macOS calls the app "damaged" and refuses. Always package through `npm run dist` or
`scripts/package.mjs` rather than calling `electron-builder` directly, which skips this step.

## Conduct and security

Be kind; see the [Code of Conduct](CODE_OF_CONDUCT.md). Report vulnerabilities privately as described in
[SECURITY.md](SECURITY.md).
