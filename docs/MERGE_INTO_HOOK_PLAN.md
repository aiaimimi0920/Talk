# Superseded HookLess Into Hook Merge Plan

This plan is superseded.

`HookLess` is now officially `Talk`: an independent Neuro voice input app and
local capability provider. Talk should not be merged into `Hook/` as an
internal subsystem.

The current architecture decision is documented in:

- `../../docs/architecture/neuro-local-app-structure.md`
- `../../docs/talk/README.md`
- `../README.md`

## Current direction

```text
Talk remains independently runnable.
Hook calls Talk when Talk is installed.
Loom may mediate complex Talk/Hook workflows.
```

Hook may keep a minimal connector, UI affordance, or compatibility layer for
voice features, but Talk owns:

- hotkey and push-to-talk/toggle state;
- audio capture and audio artifacts;
- ASR/transcription and optional text processing;
- insertion strategy and safety fallbacks;
- voice session evidence and local API surface.

## Historical note

The old merge idea mapped crates into `Hook/src-tauri/src/voice`:

- `talk-core` -> `Hook/src-tauri/src/voice/core`
- `talk-audio` -> `Hook/src-tauri/src/voice/audio`
- `talk-client` -> `Hook/src-tauri/src/voice/client`
- `talk-insert` -> `Hook/src-tauri/src/voice/insert`
- `talk-hotkey` -> existing Tauri global shortcut registration

That mapping is retained only as historical context for existing Hook voice code
and migration audits. It is no longer the product direction.
