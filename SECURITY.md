# Security

## Reporting a vulnerability

Please report security problems privately through
[GitHub's private vulnerability reporting](https://github.com/gustavorubim/sayso/security/advisories/new),
not in a public issue. Include what you found, how to reproduce it, and the app version and operating system.

You can expect an acknowledgement within a week. Fixes are released as a new version and credited in the release
notes unless you prefer otherwise.

## Supported versions

Only the latest release receives security fixes.

## What the app has access to

Sayso listens for a global shortcut, records from the microphone while that shortcut is active, reads and
writes the clipboard, and sends keystrokes to the focused application. It connects to the network to download
models from Hugging Face and to check GitHub Releases for updates. If you turn on the rewrite shortcut, the text of
those dictations is sent to OpenRouter with your API key; audio is never sent. Reports about any behaviour beyond this are
especially welcome.
