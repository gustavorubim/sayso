# Changelog

Notable changes to Sayso. The format follows [Keep a Changelog](https://keepachangelog.com), and versions
follow [Semantic Versioning](https://semver.org).

## Unreleased

## 0.3.0

### Changed

- **The app is now called Sayso** (it was "Shortcut to Text"), with a new icon and colours. The repository moved to
  `gustavorubim/sayso`. Models, settings and history are carried over on first start. The old app is not removed
  automatically; uninstall it yourself.

### Added

- Write as you speak: an option in Settings to type each sentence at every pause while you keep talking
- Rewrite shortcut: a second, optional shortcut that dictates and then cleans up the text with a language model
  through OpenRouter, using your own API key, with a choice of model and editable instructions
- A "Try it" box on the Rewrite screen to test the key, model and instructions on typed text
- History marks dictations that were rewritten

## 0.2.1

### Fixed

- The tray icon and window icon were missing in the installed app

## 0.2.0

### Added

- Update notice in the sidebar: "Restart to update" once a new version has downloaded (Windows, Linux AppImage),
  or a download link where the app cannot update itself (macOS, Linux .deb)
- "Check for updates" in Settings
- The app checks for updates every six hours while it runs

### Changed

- Development runs use their own data folder, so `npm run dev` works while the installed app is open

## 0.1.1

### Fixed

- macOS installers now build when no code-signing secrets are configured

### Added

- Screenshots in the README

## 0.1.0

First version, released as "Shortcut to Text".

### Added

- Global dictation shortcut with hold-to-talk and press-to-toggle modes
- Text delivery by paste (with clipboard restore), simulated typing, or clipboard only
- Seven speech models downloadable from Hugging Face: Parakeet TDT 0.6B v3, Whisper Large v3 Turbo,
  Distil-Whisper Large v3.5, Whisper Small, Canary 180M Flash, SenseVoice Small, Moonshine Base
- Model downloads with progress, cancel and resume
- Dictionary replacements, spoken line breaks, and optional filler removal
- Local, searchable dictation history
- Listening indicator, tray icon, and open-at-login
- Installers for Windows, macOS and Linux built by GitHub Actions
