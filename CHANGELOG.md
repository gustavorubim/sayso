# Changelog

Notable changes to Shortcut to Text. The format follows [Keep a Changelog](https://keepachangelog.com), and versions
follow [Semantic Versioning](https://semver.org).

## Unreleased

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

First version.

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
