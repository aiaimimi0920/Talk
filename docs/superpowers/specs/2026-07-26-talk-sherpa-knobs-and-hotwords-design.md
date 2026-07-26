# Talk Sherpa Accuracy Knobs & Hotwords Biasing — Design

Status: Implemented 2026-07-26.

## Problem

The local sherpa daemon (`tools/talk-local-asr-sherpa`) already supports
`--enable-endpoint`, `--decoding-method`, and `--hotwords-file`, but the desktop
launch path did not expose them:

- `SpeculativeLocalAsrDaemonConfig` had no `enable_endpoint` field and no
  user-vocabulary source.
- `append_desktop_local_asr_daemon_args` never emitted `--enable-endpoint`.
- There was no way to bias recognition toward domain terms / proper nouns.

## Design

### New config fields (`crates/talk-core/src/lib.rs`)
`SpeculativeLocalAsrDaemonConfig` gains (both `#[serde(default)]`, backward
compatible; `f32` deliberately avoided to keep the struct `Eq`):

- `enable_endpoint: Option<bool>` — `None` keeps the daemon default (`true`).
- `hotwords_words: Option<PathBuf>` — a plain vocabulary list (one phrase per
  line, `#` comments, optional trailing `:score`). Validated as a non-blank path.

### Desktop wiring (`crates/talk-desktop/src/lib.rs`)
- `append_desktop_local_asr_daemon_args` now emits `--enable-endpoint true|false`
  only when configured.
- `desktop_effective_decoding_method` **auto-upgrades** decoding to
  `modified_beam_search` whenever a `hotwords_file` is present — sherpa hotword
  biasing only takes effect under beam search, so leaving it on greedy would
  silently ignore the vocabulary. (Chosen default: greedy globally, upgrade only
  when hotwords are configured.)
- `desktop_render_sherpa_hotwords_lines` (pure) renders a raw vocabulary list,
  dropping blanks/comments and appending the default score (`1.5`) to unscored
  phrases. `desktop_generate_sherpa_hotwords_file` writes it to
  `<model_root>/talk-generated-hotwords.txt` at launch; the launch-plan builder
  substitutes the generated file into `hotwords_file` (explicit `hotwords_file`
  wins).

## Tests

- `crates/talk-core/tests/config_contract.rs`: new fields parse; `enable_endpoint`
  and `hotwords_words` default to `None`; blank `hotwords_words` rejected.
- `crates/talk-desktop/tests/desktop_contract.rs`: `--enable-endpoint` emitted
  when set / omitted when unset; a configured `hotwords_file` forces
  `--decoding-method modified_beam_search`; a `hotwords_words` list is rendered to
  a generated `--hotwords-file` (default score applied, explicit score preserved,
  comments dropped) and also forces beam search. The existing full-arg-vector test
  is unchanged (only the struct literal gained the two new `None` fields).
- `examples/desktop-streaming-service-speculative-config.toml` documents the new
  options.

## Verification

`cargo test -p talk-core -p talk-desktop` (all green). Accuracy impact is
measurable via `scripts/Invoke-TalkAsrCorpusBenchmark.ps1` with vs without a
domain-vocabulary hotwords list (expect lower in-vocab CER; watch for
out-of-vocab over-biasing).
