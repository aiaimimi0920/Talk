use serde::Deserialize;
use talk_core::{
    SpeculativeCorrectionPatch, SpeculativeEdit, SpeculativeEditKind, SpeculativeMode, TalkError,
};

#[derive(Debug, Deserialize)]
struct CloudPatchPayload {
    segment_id: String,
    original_text: String,
    corrected_text: String,
    confidence: f32,
    edits: Vec<CloudEditPayload>,
}

#[derive(Debug, Deserialize)]
struct CloudEditPayload {
    kind: String,
    before: String,
    after: String,
}

fn parse_edit_kind(value: &str) -> Result<SpeculativeEditKind, TalkError> {
    match value {
        "punctuation" => Ok(SpeculativeEditKind::Punctuation),
        "spacing" => Ok(SpeculativeEditKind::Spacing),
        "casing" => Ok(SpeculativeEditKind::Casing),
        "dictionary_correction" => Ok(SpeculativeEditKind::DictionaryCorrection),
        "filler_removal" => Ok(SpeculativeEditKind::FillerRemoval),
        "rewrite" => Ok(SpeculativeEditKind::Rewrite),
        other => Err(TalkError::Provider(format!(
            "unknown speculative edit kind: {other}"
        ))),
    }
}

pub fn parse_cloud_correction_patch(
    json: &str,
    mode: SpeculativeMode,
) -> Result<SpeculativeCorrectionPatch, TalkError> {
    let payload: CloudPatchPayload = serde_json::from_str(json)
        .map_err(|error| TalkError::Provider(format!("invalid correction patch json: {error}")))?;
    let edits = payload
        .edits
        .into_iter()
        .map(|item| {
            Ok(SpeculativeEdit::new(
                parse_edit_kind(&item.kind)?,
                item.before,
                item.after,
            ))
        })
        .collect::<Result<Vec<_>, TalkError>>()?;
    let patch = SpeculativeCorrectionPatch::new(
        payload.segment_id,
        payload.original_text,
        payload.corrected_text,
        payload.confidence,
        edits,
    )?;
    patch.validate_for_mode(mode)?;
    Ok(patch)
}
