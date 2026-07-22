# Talk Smart Routing and Information Preservation Design

Date: 2026-07-22

## Status

Approved design baseline. Implementation has not started.

## Problem

Talk's default `RightAlt` action uses Smart mode. The current Smart router scans the
entire transcript for action keywords. Any occurrence of words such as `open`,
`launch`, or their Chinese equivalents immediately routes the complete transcript to
Command mode.

This is incorrect for long-form transcription. A meeting, livestream, video, or
multi-speaker conversation can quote a command without asking Talk to execute it.

The production failure is recorded in:

`release/Talk/talk-single-exe-20260722-r8/.runtime/talk-desktop/logs/418cb09a-3880-48ec-99f5-6b719bc391a1.json`

The session retained a 5,703-character transcript. The first command keyword occurred
hundreds of characters into quoted dialogue, but the final output was reduced to a
28-character assistant answer. The ASR input was not lost. Information was discarded
after Smart incorrectly routed the full transcript to Command processing.

## Goals

1. Preserve Smart mode as the default `RightAlt` behavior.
2. Distinguish a direct instruction to Talk from quoted or transcribed instructions.
3. Keep live streaming text faithful while the user's complete intent is still unknown.
4. Allow explicit leading meta-instructions to transform a long trailing body.
5. Prevent catastrophic information loss when Smart resolves to faithful transcription.
6. Ensure stop-time assembly is at least as complete as the text visible in the live HUD.
7. Preserve the behavior of explicit Command, Generate, Document, Translate, and
   Transcribe shortcuts.
8. Avoid adding another provider round trip to the stop path.

## Non-Goals

1. Replacing the provider model.
2. Adding a second cloud classification request.
3. Redesigning the desktop mode picker or shortcut map.
4. Solving speaker diarization or identifying individual speakers.
5. Broadly rewriting the streaming ASR segmentation system.

## Design Principles

### Intent scope matters more than keyword presence

An action keyword is evidence only when it appears in a direct leading intent span.
Keywords inside quoted speech, examples, questions, conditionals, or a long-form body
must not change the route by themselves.

### Incomplete streaming segments cannot safely determine final intent

Live speculative correction operates on partial clauses. It must not execute or
generate content before the complete utterance is available. Live correction therefore
uses faithful Transcribe processing regardless of the final requested mode.

### Smart transformation must be fail-safe

When Smart resolves to Transcribe, information preservation is more important than a
provider rewrite. A provider result that fails the full-document preservation policy is
rejected in favor of the complete faithful aggregate.

## Processing Stages

### Stage 1: Live streaming correction

Each committed local ASR segment is sent to the text processor using Transcribe mode.
The request may add punctuation, fix obvious recognition errors, and lightly normalize
spacing. It must not use Smart, Command, Generate, Document, or Translate prompts.

This stage updates the HUD and optional live insertion only. It does not select the
session's final Smart route.

### Stage 2: Stop-time transcript assembly

On stop, Talk drains completed live correction jobs and builds the complete session
transcript from the tracker snapshot, ordered HUD-only pending segments, and any
uncommitted final ASR tail. Existing source offsets remain authoritative so synthetic
punctuation cannot duplicate ASR text.

The stop aggregate must contain every ordered nonblank segment represented in the live
HUD. A final ASR event that revises an already committed source segment is merged as a
revision, not silently discarded merely because it is not a prefix append. The merge
must preserve unaffected committed segments and replace or extend only the affected
source segment.

The assembled transcript is the authoritative preservation baseline. It is retained
until final processing succeeds and passes the applicable safety policy.

### Stage 3: Full-session Smart routing

Smart routing evaluates the complete assembled transcript once. Routing follows this
precedence order:

1. Explicit leading meta-instruction.
2. Short direct instruction.
3. Long-form or multi-sentence transcription evidence.
4. Transcribe fallback.

The first nonblank clause, capped at 96 non-whitespace characters, is the leading intent
span. Command keywords outside that span cannot influence intent.

#### Explicit leading meta-instructions

An explicit leading instruction may transform a long trailing body:

- `Summarize the following content:` -> Document
- `Polish the following transcript:` -> Document
- `Translate the following content into English:` -> Translate
- `Write an article based on the following material:` -> Generate

Chinese equivalents and established English command forms are supported. Politeness
and direct-address prefixes such as `please`, `help me`, and their Chinese equivalents
remain valid leading intent evidence.

#### Short direct instructions

A transcript of at most 80 non-whitespace characters may route to Command when its
leading intent span begins with a supported action or direct-address command form. For
example, `Open Notepad` routes to Command. A question, quotation, conditional, or
descriptive prefix prevents this short-command rule from matching.

#### Quoted and descriptive content

The following do not constitute direct intent merely because they contain an action
word:

- `He said to open Notepad and then continued the demonstration.`
- `Why can this channel not be opened?`
- `If you open the application, this dialog appears.`
- A long meeting or video transcript containing any of those sentences.

#### Long-form evidence

Long character count, multiple sentence boundaries, and multiple streaming segments
are strong Transcribe evidence unless an explicit leading meta-instruction already
matched. The initial routing constants are:

- at least 160 non-whitespace characters; or
- at least 3 sentence boundaries; or
- at least 3 committed streaming segments.

Any one of these is sufficient long-form evidence. These are implementation constants
covered by boundary tests, not user configuration in this change.

### Stage 4: Routed provider processing

The provider is called once with the routed concrete mode. Smart itself remains the
requested mode for reporting and desktop policy, while `smart_routed_mode` records the
resolved concrete mode.

Explicit non-Smart mode shortcuts bypass Smart inference and retain their current
processing behavior.

### Stage 5: Preservation validation

Preservation validation applies when the concrete processing mode is Transcribe or
Dictate, including Smart routed to Transcribe.

The existing segment patch ratio only measures the span between a common prefix and
suffix. It is intentionally not reused for full documents because a few punctuation
changes distributed across a long transcript would look like a full rewrite.

Full-document validation uses two independent checks after normalizing whitespace:

1. Catastrophic compression: for inputs of at least 120 non-whitespace characters, the
   output must retain at least 60 percent of the input character count.
2. Bounded sequence change: a banded character edit calculation, with punctuation and
   whitespace discounted, must keep normalized change at or below 35 percent.

The banded calculation must have bounded memory and an early-exit budget so long
sessions cannot create quadratic memory growth. Passing only one check is insufficient.
Short faithful inputs use the bounded sequence check without the long-input compression
floor.

If validation fails:

1. Keep the assembled faithful transcript as `output_text`.
2. Continue the session as completed rather than failed.
3. Log a non-sensitive preservation fallback reason.
4. Do not issue a second provider request.

Document, Generate, Translate, and Command outputs are intentionally exempt because
their valid output length and wording may differ substantially from the source.

## Data Model

Introduce a focused Smart route analysis value containing only derived evidence:

- non-whitespace character count;
- sentence boundary count;
- whether long-form evidence is present;
- matched leading intent category, if any;
- resolved concrete mode;
- a stable reason code for diagnostics.

The analysis value must not retain or log the full transcript.

Live correction jobs receive an explicit faithful correction mode rather than
inheriting the session's final `mode_override`.

## Observability

Add non-sensitive route and preservation diagnostics:

- requested mode;
- resolved mode;
- route reason code;
- input character count;
- sentence boundary count;
- streaming segment count when available;
- output character count;
- normalized full-document change ratio for faithful modes;
- input/output character retention ratio;
- preservation fallback reason.

Do not add transcript content, provider credentials, or foreground selected text to
these diagnostics.

## Error Handling

1. Ambiguous Smart routing defaults to Transcribe.
2. Provider errors continue to use the existing local fallback behavior.
3. A rejected faithful provider result is treated as a successful local fallback.
4. Failure to compute optional route metrics must not discard the assembled transcript.
5. Explicit transformation modes retain existing error behavior.

## Test Strategy

### Router unit tests

- `Open Notepad` routes to Command.
- `He said to open Notepad` routes to Transcribe.
- `Why can this channel not be opened?` routes to Transcribe.
- A long transcript with an action keyword hundreds of characters into the body routes
  to Transcribe.
- `Summarize the following content:` followed by a long body routes to Document.
- Leading Translate, Generate, and Command instructions keep their concrete routes.
- Threshold boundary cases are explicit and deterministic.

### Live correction tests

- A Smart session's live correction job uses Transcribe processing.
- A live segment containing an action keyword cannot produce a Command response.
- Explicit final Command mode still receives faithful live preview before stop.

### Preservation tests

- A 5,703-character faithful transcript cannot be replaced by a 28-character answer.
- Punctuation and small recognition corrections remain accepted.
- Distributed punctuation corrections across a long document are not mistaken for a
  full rewrite.
- Broad Document, Generate, Translate, and Command outputs remain accepted.
- Preservation fallback keeps session status completed and persists the faithful output.

### Desktop stop-path tests

- The complete tracker aggregate is supplied to Smart routing.
- Stop-time assembly contains every ordered nonblank segment visible in the live HUD.
- A non-prefix final ASR revision updates the affected source segment without dropping
  other committed content.
- `smart_routed_mode` is persisted and drives the existing desktop output policy.
- A long-form Smart session inserts or displays the faithful final transcript rather
  than an assistant command answer.

### Product smoke

Build a new isolated release using a unique hotkey, ASR port, provider endpoint, and
`LOCALAPPDATA`. Run at least these scenarios without touching the current live instance:

1. Long-form transcript containing a quoted `open` instruction -> Transcribe output
   preserves the full content.
2. Short `Open Notepad` utterance -> Command route and assistant result.
3. Leading summary instruction followed by a long body -> Document route.
4. Provider returns a catastrophically short faithful result -> local aggregate wins.

## Compatibility

No configuration migration is required. Existing explicit mode shortcuts and provider
configuration remain valid. The change intentionally alters only Smart inference and
live speculative correction semantics.

## Release Acceptance Criteria

1. The production failure fixture routes to Transcribe.
2. Its final output cannot be reduced to the observed 28-character command answer.
3. Existing short direct commands continue to route to Command.
4. Explicit leading meta-instructions can transform long trailing content.
5. All Rust workspace tests and PowerShell tests pass.
6. The two-file product validator passes.
7. Isolated product smokes pass while the current live Talk and ASR worker remain
   unchanged.
