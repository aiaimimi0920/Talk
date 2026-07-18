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
}

#[derive(Debug, Default)]
pub struct SpeculativeRuntimeState {
    segments: HashMap<String, SpeculativeSegment>,
    committed_segment_ids: Vec<String>,
    correction_requested_segment_ids: HashSet<String>,
    cumulative_source_committed_text: HashMap<String, String>,
    cumulative_source_commit_counts: HashMap<String, usize>,
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
        let source_text = event.text().to_string();
        let Some((segment_id, text)) =
            self.runtime_segment_for_asr_source(&source_segment_id, &source_text)
        else {
            return Ok(Vec::new());
        };
        let readiness = evaluate_segment_readiness(
            config,
            &SegmenterInput {
                text: text.clone(),
                trailing_silence_ms,
                asr_marked_final: event.is_final(),
            },
        );

        if event.is_final() || readiness == SegmentReadiness::Ready {
            if self.is_segment_committed(&segment_id) {
                self.commit_local_segment_from_asr_source(
                    &source_segment_id,
                    &source_text,
                    &segment_id,
                    &text,
                )?;
                return Ok(Vec::new());
            }
            self.commit_local_segment_from_asr_source(
                &source_segment_id,
                &source_text,
                &segment_id,
                &text,
            )?;
            let mut events = vec![SpeculativeRuntimeEvent::LocalSegmentCommitted {
                segment_id: segment_id.clone(),
                text: text.clone(),
            }];
            if readiness == SegmentReadiness::Ready {
                if let Some(request) = self.request_correction_once(
                    &segment_id,
                    &text,
                    config.correction_context_chars,
                ) {
                    events.push(SpeculativeRuntimeEvent::CorrectionRequested {
                        segment_id: request.segment_id,
                        local_text: request.local_text,
                        context_before: request.context_before,
                    });
                }
            }
            return Ok(events);
        }

        self.segments.insert(
            segment_id.clone(),
            SpeculativeSegment::new(segment_id.clone(), text.clone())?,
        );
        Ok(vec![SpeculativeRuntimeEvent::DraftUpdated {
            segment_id,
            text,
        }])
    }

    fn runtime_segment_for_asr_source(
        &self,
        source_segment_id: &str,
        source_text: &str,
    ) -> Option<(String, String)> {
        let source_text = source_text.trim();
        if source_text.is_empty() {
            return None;
        }

        let Some(committed_text) = self.cumulative_source_committed_text.get(source_segment_id)
        else {
            return Some((source_segment_id.to_string(), source_text.to_string()));
        };
        let committed_text = committed_text.trim();
        if source_text == committed_text {
            return None;
        }
        let tail_text = source_text.strip_prefix(committed_text)?.trim_start();
        if tail_text.is_empty() {
            return None;
        }

        let next_index = self
            .cumulative_source_commit_counts
            .get(source_segment_id)
            .copied()
            .unwrap_or(1)
            + 1;
        Some((
            format!("{source_segment_id}#{next_index}"),
            tail_text.to_string(),
        ))
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
