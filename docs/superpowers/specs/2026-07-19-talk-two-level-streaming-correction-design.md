# Talk Two-Level Streaming Correction Design

## Goal

Talk must optimize speech at two levels. Stable local clauses are sent to the remote text processor one at a time, corrected clauses become white and are inserted immediately in recognition order, and recording stop waits for every queued local correction before one final whole-text optimization.

For `"你好，今天我们去北京玩，明天我们去上海玩。"`, the intended provider calls are:

1. `你好`
2. `今天我们去北京玩`
3. `明天我们去上海玩`
4. The concatenation of the three corrected clauses

## Segmentation

The runtime segmenter treats Chinese and ASCII clause punctuation as semantic boundaries:

- Clause boundaries: `，`, `,`, `；`, `;`, `：`, `:`
- Sentence boundaries: `。`, `！`, `？`, `.`, `!`, `?`

A clause ending at clause punctuation may commit with two or more non-whitespace characters. Existing pause and 30-character thresholds remain fallbacks for speech without punctuation. The cumulative ASR splitter continues producing ordered virtual segments such as `seg`, `seg#2`, and `seg#3`.

## Local Correction Queue

Each recording owns a FIFO correction queue and a tracker. The tracker records segment id, local text, corrected text, insert anchor, pending job count, and stopping or cancelled state. The worker processes one provider request at a time, so corrections and insertions cannot be reordered.

Before each local request, `contextBefore` is rebuilt from preceding corrected clauses. Stopping closes the queue to new work and waits for the pending count to reach zero. Cancelling prevents queued results from inserting or opening stale popups.

## HUD And Insertion

An uncorrected clause is yellow. A successfully processed clause becomes white and is inserted into the current editable target. Once inserted, it is removed from the yellow accumulation so later speech cannot recolor completed text. The HUD remains in `Listening` state and keeps the existing double-buffered waveform path.

If no editable target is available, corrected text is still retained in the tracker, but no anchor is recorded and the copy-popup fallback may be used.

## Final Whole-Text Optimization

After the local queue drains, Talk builds the aggregate from corrected clauses in recognition order and appends any final ASR tail once.

Exactly one whole-text provider call is allowed:

- With no inserted local clauses, the normal processed transcript path performs the global call and inserts the result.
- With inserted local clauses, the local transcript path performs no AI call and no insertion; one final document correction job performs the global call.

Safe replacement compares the current target against the actual corrected clauses Talk inserted. If the target still matches, Talk replaces it with the final result. If the user edited the target, focus changed, or the target is unavailable, Talk preserves user content and opens the copy popup.

## Failure Rules

- Stale, cancelled, or duplicate local results are dropped.
- Failed local requests release their pending slot and cannot block stop.
- Failed final optimization leaves locally corrected text intact.
- Target mismatch never causes destructive replacement.

## Verification

Automated tests must prove short comma clauses, three ordered local requests for the approved example, ordered corrected context, queue drain, corrected aggregate selection, exactly one global pass, and replacement against the actual inserted baseline. The product release must remain exactly `Talk.exe` plus `talk.toml`.
