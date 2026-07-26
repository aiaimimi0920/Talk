use std::collections::{HashMap, HashSet};

use crate::segmenter::{
    evaluate_segment_readiness, SegmentReadiness, SegmenterConfig, SegmenterInput,
};
use talk_client::StreamingAsrEvent;
use talk_core::{SpeculativeSegment, TalkError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeculativeCorrectionRequest {
    pub segment_id: String,
    pub local_text: String,
    pub context_before: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeculativeRuntimeEvent {
    DraftUpdated {
        segment_id: String,
        text: String,
    },
    LocalSegmentCommitted {
        segment_id: String,
        text: String,
    },
    CorrectionRequested {
        segment_id: String,
        local_text: String,
        context_before: String,
    },
    /// Sub-segments rolled back by a revision that were NOT re-committed (a
    /// revised hypothesis segmented into fewer pieces). Consumers should drop
    /// these ids from any per-segment view (e.g. the streaming HUD).
    LocalSegmentsInvalidated {
        segment_ids: Vec<String>,
    },
}

#[derive(Debug, Default)]
pub struct SpeculativeRuntimeState {
    segments: HashMap<String, SpeculativeSegment>,
    committed_segment_ids: Vec<String>,
    correction_requested_segment_ids: HashSet<String>,
    cumulative_source_committed_text: HashMap<String, String>,
    cumulative_source_commit_counts: HashMap<String, usize>,
    // Per ASR source id: the cumulative committed prefix length (bytes, in the
    // trimmed source) after each committed sub-segment, in order. Used to roll
    // back sub-segments when a later ASR hypothesis revises earlier words.
    cumulative_source_segment_boundaries: HashMap<String, Vec<usize>>,
}

impl SpeculativeRuntimeState {
    pub fn accept_asr_event(
        &mut self,
        event: StreamingAsrEvent,
    ) -> Result<SpeculativeRuntimeEvent, TalkError> {
        let segment_id = event.segment_id().to_string();
        let text = event.text().to_string();

        if event.is_final() {
            if let Some(segment) = self.segments.get_mut(&segment_id) {
                segment.mark_local_final(text.clone())?;
            } else {
                let mut segment = SpeculativeSegment::new(segment_id.clone(), text.clone())?;
                segment.mark_local_final(text.clone())?;
                self.segments.insert(segment_id.clone(), segment);
            }
            Ok(SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text })
        } else {
            self.segments.insert(
                segment_id.clone(),
                SpeculativeSegment::new(segment_id.clone(), text.clone())?,
            );
            Ok(SpeculativeRuntimeEvent::DraftUpdated { segment_id, text })
        }
    }

    pub fn accept_asr_event_with_segmentation(
        &mut self,
        event: StreamingAsrEvent,
        trailing_silence_ms: u64,
        config: &SegmenterConfig,
    ) -> Result<Vec<SpeculativeRuntimeEvent>, TalkError> {
        let source_segment_id = event.segment_id().to_string();
        let source_text = event.text().trim().to_string();
        let Some((tail_start_byte, invalidated_ids)) =
            self.reconcile_source_for_segmentation(&source_segment_id, &source_text)
        else {
            return Ok(Vec::new());
        };
        let text = source_text[tail_start_byte..].to_string();
        let candidates =
            runtime_segment_candidates(&text, trailing_silence_ms, event.is_final(), config);
        let mut events = Vec::new();
        let mut reemitted_ids: HashSet<String> = HashSet::new();

        for candidate in candidates {
            let segment_id = self.next_runtime_segment_id(&source_segment_id);
            reemitted_ids.insert(segment_id.clone());
            if !candidate.commit {
                self.segments.insert(
                    segment_id.clone(),
                    SpeculativeSegment::new(segment_id.clone(), candidate.text.clone())?,
                );
                events.push(SpeculativeRuntimeEvent::DraftUpdated {
                    segment_id,
                    text: candidate.text,
                });
                continue;
            }

            let committed_source_end = tail_start_byte.saturating_add(candidate.end_byte);
            let committed_source_text = source_text
                .get(..committed_source_end)
                .unwrap_or(source_text.as_str())
                .trim_end();
            let already_committed = self.is_segment_committed(&segment_id);
            self.commit_local_segment_from_asr_source(
                &source_segment_id,
                committed_source_text,
                &segment_id,
                &candidate.text,
            )?;
            if already_committed {
                continue;
            }

            events.push(SpeculativeRuntimeEvent::LocalSegmentCommitted {
                segment_id: segment_id.clone(),
                text: candidate.text.clone(),
            });
            if let Some(request) = self.request_correction_once(
                &segment_id,
                &candidate.text,
                config.correction_context_chars,
            ) {
                events.push(SpeculativeRuntimeEvent::CorrectionRequested {
                    segment_id: request.segment_id,
                    local_text: request.local_text,
                    context_before: request.context_before,
                });
            }
        }

        // Report rolled-back sub-segments that were not re-committed so consumers
        // can drop their now-stale per-segment view (leading/kept segments stay
        // in place, so HUD ordering is preserved).
        let orphan_ids: Vec<String> = invalidated_ids
            .into_iter()
            .filter(|segment_id| !reemitted_ids.contains(segment_id))
            .collect();
        if !orphan_ids.is_empty() {
            events.push(SpeculativeRuntimeEvent::LocalSegmentsInvalidated {
                segment_ids: orphan_ids,
            });
        }

        Ok(events)
    }

    /// Decide where new segmentation should start for an incoming ASR
    /// hypothesis, returning the byte offset in `source_text` from which the
    /// uncommitted tail begins (or `None` when there is nothing new to commit).
    ///
    /// - First hypothesis for a source, or a pure prefix-extension (append):
    ///   returns the committed prefix length, matching the previous behaviour.
    /// - Revision (the new hypothesis diverges from the already-committed text
    ///   after some common prefix): rolls back the diverged sub-segments so the
    ///   revised text is re-committed under the same reused ids instead of being
    ///   silently dropped. This mutates state, so it takes `&mut self`.
    fn reconcile_source_for_segmentation(
        &mut self,
        source_segment_id: &str,
        source_text: &str,
    ) -> Option<(usize, Vec<String>)> {
        if source_text.is_empty() {
            return None;
        }

        let Some(committed_text) = self.cumulative_source_committed_text.get(source_segment_id)
        else {
            return Some((0, Vec::new()));
        };
        let committed_text = committed_text.trim().to_string();
        if source_text == committed_text {
            return None;
        }
        // A shorter hypothesis that is a prefix of the committed text is stale
        // (the committed text already contains it) — keep the committed text.
        if committed_text.starts_with(source_text) {
            return None;
        }
        if let Some(tail) = source_text.strip_prefix(committed_text.as_str()) {
            if tail.trim().is_empty() {
                return None;
            }
            return Some((source_text.len() - tail.len(), Vec::new()));
        }

        // Revision: the ASR revised earlier words. Roll back every sub-segment
        // beyond the retained common prefix, then re-segment from there.
        let (retained_end, invalidated_ids) = self.invalidate_revised_source_segments(
            source_segment_id,
            &committed_text,
            source_text,
        );
        if source_text[retained_end..].trim().is_empty() {
            return None;
        }
        Some((retained_end, invalidated_ids))
    }

    /// Drop the committed sub-segments of `source_segment_id` that fall beyond
    /// the longest common prefix between the previously committed text and the
    /// revised `source_text`, returning the retained prefix length (a char
    /// boundary in `source_text`) and the ids of the rolled-back sub-segments.
    /// Reused sub-segment ids are freed so the caller re-commits the revised
    /// tail under the same ids.
    fn invalidate_revised_source_segments(
        &mut self,
        source_segment_id: &str,
        committed_text: &str,
        source_text: &str,
    ) -> (usize, Vec<String>) {
        let common_prefix_len = longest_common_prefix_len(committed_text, source_text);
        let boundaries = self
            .cumulative_source_segment_boundaries
            .get(source_segment_id)
            .cloned()
            .unwrap_or_default();
        let retained_count = boundaries
            .iter()
            .take_while(|&&end| end <= common_prefix_len)
            .count();
        let retained_end = retained_count
            .checked_sub(1)
            .map_or(0, |last| boundaries[last]);

        let mut invalidated_ids = Vec::new();
        for index in retained_count..boundaries.len() {
            let sub_segment_id = source_segment_sub_id(source_segment_id, index);
            self.committed_segment_ids
                .retain(|committed_id| committed_id != &sub_segment_id);
            self.segments.remove(&sub_segment_id);
            self.correction_requested_segment_ids
                .remove(&sub_segment_id);
            invalidated_ids.push(sub_segment_id);
        }

        self.cumulative_source_segment_boundaries
            .entry(source_segment_id.to_string())
            .or_default()
            .truncate(retained_count);
        self.cumulative_source_commit_counts
            .insert(source_segment_id.to_string(), retained_count);
        self.cumulative_source_committed_text.insert(
            source_segment_id.to_string(),
            source_text[..retained_end].trim().to_string(),
        );
        (retained_end, invalidated_ids)
    }

    fn next_runtime_segment_id(&self, source_segment_id: &str) -> String {
        let next_index = self
            .cumulative_source_commit_counts
            .get(source_segment_id)
            .copied()
            .unwrap_or(0)
            + 1;
        if next_index == 1 {
            source_segment_id.to_string()
        } else {
            format!("{source_segment_id}#{next_index}")
        }
    }

    fn commit_local_segment_from_asr_source(
        &mut self,
        source_segment_id: &str,
        source_text: &str,
        segment_id: &str,
        text: &str,
    ) -> Result<(), TalkError> {
        let already_committed = self.is_segment_committed(segment_id);
        self.commit_local_segment(segment_id, text)?;
        if !already_committed {
            let next_count = self
                .cumulative_source_commit_counts
                .get(source_segment_id)
                .copied()
                .unwrap_or(0)
                + 1;
            self.cumulative_source_commit_counts
                .insert(source_segment_id.to_string(), next_count);
            // Record this sub-segment's cumulative committed prefix length so a
            // later revision can identify which sub-segments to roll back.
            self.cumulative_source_segment_boundaries
                .entry(source_segment_id.to_string())
                .or_default()
                .push(source_text.trim().len());
        }
        self.cumulative_source_committed_text.insert(
            source_segment_id.to_string(),
            source_text.trim().to_string(),
        );
        Ok(())
    }

    fn commit_local_segment(&mut self, segment_id: &str, text: &str) -> Result<(), TalkError> {
        if let Some(segment) = self.segments.get_mut(segment_id) {
            segment.mark_local_final(text.to_string())?;
        } else {
            let mut segment = SpeculativeSegment::new(segment_id.to_string(), text.to_string())?;
            segment.mark_local_final(text.to_string())?;
            self.segments.insert(segment_id.to_string(), segment);
        }
        if !self
            .committed_segment_ids
            .iter()
            .any(|committed_id| committed_id == segment_id)
        {
            self.committed_segment_ids.push(segment_id.to_string());
        }
        Ok(())
    }

    fn is_segment_committed(&self, segment_id: &str) -> bool {
        self.committed_segment_ids
            .iter()
            .any(|committed_id| committed_id == segment_id)
    }

    fn request_correction_once(
        &mut self,
        segment_id: &str,
        local_text: &str,
        max_context_chars: usize,
    ) -> Option<SpeculativeCorrectionRequest> {
        if !self
            .correction_requested_segment_ids
            .insert(segment_id.to_string())
        {
            return None;
        }

        Some(SpeculativeCorrectionRequest {
            segment_id: segment_id.to_string(),
            local_text: local_text.to_string(),
            context_before: self.correction_context_before(segment_id, max_context_chars),
        })
    }

    fn correction_context_before(&self, current_segment_id: &str, max_chars: usize) -> String {
        if max_chars == 0 {
            return String::new();
        }
        let joined_context = self
            .committed_segment_ids
            .iter()
            .filter(|segment_id| segment_id.as_str() != current_segment_id)
            .filter_map(|segment_id| self.segments.get(segment_id))
            .map(|segment| segment.draft_text())
            .collect::<Vec<_>>()
            .join("\n");
        take_tail_chars(&joined_context, max_chars)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeSegmentCandidate {
    text: String,
    end_byte: usize,
    readiness: SegmentReadiness,
    commit: bool,
}

fn runtime_segment_candidates(
    text: &str,
    trailing_silence_ms: u64,
    asr_marked_final: bool,
    config: &SegmenterConfig,
) -> Vec<RuntimeSegmentCandidate> {
    let mut candidates = Vec::new();
    let mut start_byte = 0usize;

    for (character_byte, character) in text.char_indices() {
        let end_byte = character_byte + character.len_utf8();
        let candidate_text = text[start_byte..end_byte].trim_end();
        if candidate_text.is_empty() {
            continue;
        }
        let reached_punctuation = is_runtime_segment_punctuation(character)
            && !is_intra_number_separator(text, character_byte, end_byte, character);
        let reached_max_chunk = candidate_text
            .chars()
            .filter(|item| !item.is_whitespace())
            .count()
            >= config.max_chunk_chars;
        if !reached_punctuation && !reached_max_chunk {
            continue;
        }

        let has_following_text = !text[end_byte..].trim().is_empty();
        let candidate_trailing_silence_ms = if reached_punctuation && has_following_text {
            trailing_silence_ms.max(config.punctuation_pause_ms)
        } else {
            trailing_silence_ms
        };
        let readiness = evaluate_segment_readiness(
            config,
            &SegmenterInput {
                text: candidate_text.to_string(),
                trailing_silence_ms: candidate_trailing_silence_ms,
                asr_marked_final: asr_marked_final && !has_following_text,
            },
        );
        if readiness != SegmentReadiness::Ready {
            continue;
        }

        // A length-cap break may land mid-word in Latin text; back up to the
        // last whole word so clauses do not end on a partial token. Punctuation
        // and CJK breaks are unaffected.
        let commit_end_byte = if reached_max_chunk && !reached_punctuation {
            word_boundary_break_byte(text, start_byte, end_byte)
        } else {
            end_byte
        };
        let committed_slice = text[start_byte..commit_end_byte].trim_end();

        let candidate_text = if !asr_marked_final
            && trailing_silence_ms >= config.soft_pause_ms
            && !ends_with_any_punctuation(committed_slice)
        {
            append_pause_boundary_punctuation(committed_slice)
        } else {
            committed_slice.to_string()
        };

        candidates.push(RuntimeSegmentCandidate {
            text: candidate_text,
            end_byte: commit_end_byte,
            readiness,
            commit: true,
        });
        start_byte = commit_end_byte;
    }

    let remaining_text = text[start_byte..].trim_end();
    if !remaining_text.is_empty() {
        let readiness = evaluate_segment_readiness(
            config,
            &SegmenterInput {
                text: remaining_text.to_string(),
                trailing_silence_ms,
                asr_marked_final,
            },
        );
        let remaining_text = if !asr_marked_final
            && readiness == SegmentReadiness::Ready
            && trailing_silence_ms >= config.soft_pause_ms
            && !ends_with_any_punctuation(remaining_text)
        {
            append_pause_boundary_punctuation(remaining_text)
        } else {
            remaining_text.to_string()
        };
        candidates.push(RuntimeSegmentCandidate {
            text: remaining_text,
            end_byte: text.len(),
            readiness,
            commit: asr_marked_final || readiness == SegmentReadiness::Ready,
        });
    }

    candidates
}

/// Byte length of the longest common prefix of two strings, always landing on a
/// UTF-8 char boundary (never splits a multi-byte codepoint).
fn longest_common_prefix_len(left: &str, right: &str) -> usize {
    let mut common_len = 0;
    let mut right_chars = right.chars();
    for left_char in left.chars() {
        match right_chars.next() {
            Some(right_char) if right_char == left_char => common_len += left_char.len_utf8(),
            _ => break,
        }
    }
    common_len
}

/// Sub-segment id for the `index`-th (0-based) committed segment of a source,
/// matching [`SpeculativeRuntimeState::next_runtime_segment_id`]: the first is
/// the bare source id, later ones are `source#2`, `source#3`, ...
fn source_segment_sub_id(source_segment_id: &str, index: usize) -> String {
    if index == 0 {
        source_segment_id.to_string()
    } else {
        format!("{source_segment_id}#{}", index + 1)
    }
}

fn is_runtime_segment_punctuation(character: char) -> bool {
    matches!(
        character,
        '，' | ',' | '；' | ';' | '：' | ':' | '。' | '！' | '？' | '.' | '!' | '?'
    )
}

/// Whether the separator at `separator_byte` sits between two ASCII digits
/// (e.g. `3.14`, `1,000`, `3:30`), which is part of a number/time and must not
/// be treated as a clause/sentence boundary.
fn is_intra_number_separator(
    text: &str,
    separator_byte: usize,
    separator_end_byte: usize,
    character: char,
) -> bool {
    matches!(character, '.' | ',' | ':')
        && text[..separator_byte]
            .chars()
            .next_back()
            .is_some_and(|previous| previous.is_ascii_digit())
        && text[separator_end_byte..]
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_digit())
}

fn ends_with_any_punctuation(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .is_some_and(is_runtime_segment_punctuation)
}

fn append_pause_boundary_punctuation(text: &str) -> String {
    let trimmed = text.trim_end();
    // Match the pause comma to the script of the character it attaches to: a
    // clause ending in CJK text gets a full-width comma, one ending in Latin
    // text gets an ASCII comma. Keying off the boundary character (rather than
    // "any CJK char in the clause") keeps mixed-script clauses correct.
    let punctuation = if trimmed.chars().next_back().is_some_and(is_cjk_character) {
        '，'
    } else {
        ','
    };
    format!("{trimmed}{punctuation}")
}

fn is_cjk_character(character: char) -> bool {
    let code_point = character as u32;
    (0x3040..=0x30ff).contains(&code_point)
        || (0x3400..=0x4dbf).contains(&code_point)
        || (0x4e00..=0x9fff).contains(&code_point)
        || (0xf900..=0xfaff).contains(&code_point)
}

/// Whether `character` is part of a multi-character word (Latin letters, digits)
/// as opposed to a CJK character, where every character is its own token.
fn is_latin_word_char(character: char) -> bool {
    character.is_alphanumeric() && !is_cjk_character(character)
}

/// When a length-cap (`max_chunk_chars`) break would land inside a Latin word,
/// return the byte offset of the last whitespace in the chunk so the clause
/// ends on a whole word instead. Returns `end_byte` unchanged for CJK text, when
/// the break is already at a word boundary, or when the chunk is a single long
/// token with no whitespace to back up to.
fn word_boundary_break_byte(text: &str, start_byte: usize, end_byte: usize) -> usize {
    let boundary_char = text[..end_byte].chars().next_back();
    let following_char = text[end_byte..].chars().next();
    let splits_word = matches!(
        (boundary_char, following_char),
        (Some(before), Some(after)) if is_latin_word_char(before) && is_latin_word_char(after)
    );
    if !splits_word {
        return end_byte;
    }
    match text[start_byte..end_byte].rfind(char::is_whitespace) {
        Some(whitespace_offset) if whitespace_offset > 0 => start_byte + whitespace_offset,
        _ => end_byte,
    }
}

fn take_tail_chars(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    text.chars().skip(char_count - max_chars).collect()
}

pub fn run_mock_speculative_session(
    inputs: Vec<(bool, &str, &str)>,
) -> Result<Vec<SpeculativeRuntimeEvent>, TalkError> {
    let mut state = SpeculativeRuntimeState::default();
    let mut events = Vec::new();
    for (is_final, segment_id, text) in inputs {
        let event = if is_final {
            StreamingAsrEvent::try_final(segment_id, text)?
        } else {
            StreamingAsrEvent::try_partial(segment_id, text)?
        };
        events.push(state.accept_asr_event(event)?);
    }
    Ok(events)
}
