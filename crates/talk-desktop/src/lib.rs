mod model_bootstrap;
mod product_payload;

pub use model_bootstrap::{
    default_zipformer_model_spec, download_and_install_model, install_model_from_reader,
    resolve_talk_data_root, validate_installed_model, ModelSpec,
};
pub use product_payload::{
    build_embedded_runtime_payload, extract_embedded_runtime_payload,
    parse_embedded_runtime_payload, EmbeddedRuntimePayload, EmbeddedRuntimePayloadFile,
    EmbeddedRuntimePayloadSource,
};

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use talk_core::{
    DesktopPasteShortcut, DesktopPasteShortcutOverride, NativeReadinessStatus, OutputMode,
    SpeculativeLocalAsrDaemonConfig, SpeculativeLocalAsrDaemonMode,
    SpeculativeSherpaOnlineModelFamily, TalkConfig, TriggerMode, VoiceMode,
};
use talk_insert::should_auto_apply_corrected_text;
use talk_runtime::{RuntimePhase, SpeculativeRuntimeEvent};

pub const TALK_DESKTOP_AUDIO_FILE_OVERRIDE_ENV: &str = "TALK_DESKTOP_AUDIO_FILE_OVERRIDE";
pub const TALK_DESKTOP_INSERT_TARGET_WINDOW_ENV: &str = "TALK_DESKTOP_INSERT_TARGET_WINDOW";
pub const TALK_DESKTOP_INSERT_TARGET_FOCUS_ENV: &str = "TALK_DESKTOP_INSERT_TARGET_FOCUS";
pub const TALK_DESKTOP_DEFAULT_CONFIG_FILE_NAME: &str = "talk.toml";
pub const TALK_PACKAGED_LOCAL_ASR_DAEMON_EXE_NAME: &str = "talk-local-asr-sherpa.exe";
const DESKTOP_LISTENING_LOCAL_DETECTION_PLACEHOLDER: &str = "...";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopLocalAsrDaemonLaunchPlan {
    pub executable_path: PathBuf,
    pub bind: String,
    pub args: Vec<String>,
}

pub fn desktop_preferred_paste_shortcut_for_process_name(
    process_name: Option<&str>,
) -> Option<&'static str> {
    let normalized = normalized_process_name(process_name)?;

    match normalized.as_str() {
        "tabby" => Some("ctrl_shift_v"),
        _ => None,
    }
}

pub fn desktop_preferred_paste_shortcut_for_target(
    overrides: &[DesktopPasteShortcutOverride],
    process_name: Option<&str>,
    target: Option<&DesktopInsertTargetContext>,
) -> Option<DesktopPasteShortcut> {
    desktop_configured_paste_shortcut_for_target(overrides, process_name, target).or_else(|| {
        desktop_preferred_paste_shortcut_for_process_name(process_name)
            .and_then(desktop_paste_shortcut_from_legacy_env_value)
    })
}

fn desktop_configured_paste_shortcut_for_target(
    overrides: &[DesktopPasteShortcutOverride],
    process_name: Option<&str>,
    target: Option<&DesktopInsertTargetContext>,
) -> Option<DesktopPasteShortcut> {
    overrides
        .iter()
        .find(|override_rule| {
            desktop_paste_shortcut_override_matches_target(override_rule, process_name, target)
        })
        .map(|override_rule| override_rule.paste_shortcut)
}

fn desktop_paste_shortcut_override_matches_target(
    override_rule: &DesktopPasteShortcutOverride,
    process_name: Option<&str>,
    target: Option<&DesktopInsertTargetContext>,
) -> bool {
    if let Some(expected_process_name) = override_rule.process_name.as_deref() {
        if normalized_process_name(process_name)
            != normalized_process_name(Some(expected_process_name))
        {
            return false;
        }
    }
    if let Some(expected_focus_class_name) = override_rule.focus_class_name.as_deref() {
        if normalized_match_value(target.and_then(|target| target.focus_class_name.as_deref()))
            != normalized_match_value(Some(expected_focus_class_name))
        {
            return false;
        }
    }
    if let Some(expected_framework_id) = override_rule.automation_framework_id.as_deref() {
        if normalized_match_value(
            target.and_then(|target| target.automation_framework_id.as_deref()),
        ) != normalized_match_value(Some(expected_framework_id))
        {
            return false;
        }
    }
    if let Some(expected_control_type) = override_rule.automation_control_type.as_deref() {
        if normalized_match_value(
            target.and_then(|target| target.automation_control_type.as_deref()),
        ) != normalized_match_value(Some(expected_control_type))
        {
            return false;
        }
    }

    true
}

fn normalized_process_name(process_name: Option<&str>) -> Option<String> {
    let normalized = normalized_match_value(process_name)?;
    Some(
        normalized
            .strip_suffix(".exe")
            .unwrap_or(normalized.as_str())
            .to_string(),
    )
}

fn normalized_match_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

fn desktop_paste_shortcut_from_legacy_env_value(value: &str) -> Option<DesktopPasteShortcut> {
    match value {
        "ctrl_v" => Some(DesktopPasteShortcut::ControlV),
        "ctrl_shift_v" => Some(DesktopPasteShortcut::ControlShiftV),
        "shift_insert" => Some(DesktopPasteShortcut::ShiftInsert),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundInsertTarget {
    pub window_handle: isize,
    pub focus_handle: Option<isize>,
    pub primary_focus_handle: Option<isize>,
    pub fallback_focus_handle: Option<isize>,
    pub focus_capture_source: Option<ForegroundFocusCaptureSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForegroundFocusCaptureSource {
    GuiThreadInfo,
    AttachedGetFocus,
}

impl ForegroundFocusCaptureSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GuiThreadInfo => "gui_thread_info",
            Self::AttachedGetFocus => "attached_get_focus",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundFocusCaptureResolution {
    pub focus_handle: Option<isize>,
    pub source: Option<ForegroundFocusCaptureSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForegroundTargetReleaseReason {
    TargetStable,
    Timeout,
}

impl ForegroundTargetReleaseReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TargetStable => "target_stable",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ForegroundTargetStabilityProgress {
    pub poll_count: u32,
    pub target_foreground_poll_count: u32,
    pub trailing_target_foreground_poll_count: u32,
}

pub fn observe_foreground_target_stability(
    mut progress: ForegroundTargetStabilityProgress,
    target_window_handle: isize,
    observed_foreground_handle: isize,
) -> ForegroundTargetStabilityProgress {
    progress.poll_count += 1;

    if target_window_handle != 0 && observed_foreground_handle == target_window_handle {
        progress.target_foreground_poll_count += 1;
        progress.trailing_target_foreground_poll_count += 1;
    } else {
        progress.trailing_target_foreground_poll_count = 0;
    }

    progress
}

pub fn foreground_target_stability_satisfied(
    progress: ForegroundTargetStabilityProgress,
    required_stable_foreground_polls: u32,
) -> bool {
    if required_stable_foreground_polls == 0 {
        return true;
    }

    progress.trailing_target_foreground_poll_count >= required_stable_foreground_polls
}

pub fn foreground_target_refresh_requested(
    target_window_handle: isize,
    observed_foreground_handle: isize,
) -> bool {
    target_window_handle != 0 && observed_foreground_handle != target_window_handle
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInsertTargetRestoreDiagnostic {
    pub attempted: bool,
    pub target_window_exists: Option<bool>,
    pub target_focus_exists: Option<bool>,
    pub focus_restore_requested: bool,
    pub post_insert_release_reason: Option<ForegroundTargetReleaseReason>,
    pub post_insert_wait_duration_ms: Option<u64>,
    pub post_insert_poll_count: Option<u32>,
    pub post_insert_target_foreground_poll_count: Option<u32>,
    pub post_insert_trailing_target_foreground_poll_count: Option<u32>,
    pub post_insert_required_stable_foreground_polls: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInsertTargetDiagnostic {
    pub captured_window_handle: String,
    pub captured_focus_handle: Option<String>,
    pub captured_primary_focus_handle: Option<String>,
    pub captured_fallback_focus_handle: Option<String>,
    pub captured_focus_source: Option<String>,
    pub output_strategy: Option<String>,
    pub focus_class_name: Option<String>,
    pub caret_window_handle: Option<String>,
    pub automation_control_type: Option<String>,
    pub automation_framework_id: Option<String>,
    pub automation_runtime_id: Option<Vec<i32>>,
    pub automation_is_keyboard_focusable: Option<bool>,
    pub automation_supports_text_pattern: bool,
    pub automation_supports_value_pattern: bool,
    pub focus_looks_editable: Option<bool>,
    pub restore_attempted: bool,
    pub restore_target_window_exists: Option<bool>,
    pub restore_target_focus_exists: Option<bool>,
    pub restore_focus_requested: bool,
    pub post_insert_release_reason: Option<String>,
    pub post_insert_wait_duration_ms: Option<u64>,
    pub post_insert_poll_count: Option<u32>,
    pub post_insert_target_foreground_poll_count: Option<u32>,
    pub post_insert_trailing_target_foreground_poll_count: Option<u32>,
    pub post_insert_required_stable_foreground_polls: Option<u32>,
    pub trace: Option<DesktopInsertTargetTraceDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInsertTargetTraceDiagnostic {
    pub selected_origin_source: Option<String>,
    pub origin_target: Option<DesktopInsertTargetSnapshotDiagnostic>,
    pub current_target: Option<DesktopInsertTargetSnapshotDiagnostic>,
    pub pending_hotkey_origin_target: Option<DesktopInsertTargetSnapshotDiagnostic>,
    pub release_time_origin_target: Option<DesktopInsertTargetSnapshotDiagnostic>,
    pub same_window_as_origin: Option<bool>,
    pub same_control_by_handle: Option<bool>,
    pub same_control_by_runtime_id: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInsertTargetSnapshotDiagnostic {
    pub window_handle: Option<String>,
    pub focus_handle: Option<String>,
    pub focus_class_name: Option<String>,
    pub caret_window_handle: Option<String>,
    pub automation_control_type: Option<String>,
    pub automation_framework_id: Option<String>,
    pub automation_runtime_id: Option<Vec<i32>>,
    pub automation_is_keyboard_focusable: Option<bool>,
    pub automation_supports_text_pattern: bool,
    pub automation_supports_value_pattern: bool,
    pub focus_looks_editable: bool,
}

pub fn build_desktop_insert_target_diagnostic(
    target: ForegroundInsertTarget,
    context: Option<&DesktopInsertTargetContext>,
    output_strategy: Option<DesktopOutputStrategy>,
    restore: Option<DesktopInsertTargetRestoreDiagnostic>,
) -> DesktopInsertTargetDiagnostic {
    build_desktop_insert_target_diagnostic_with_trace(
        target,
        context,
        output_strategy,
        restore,
        None,
    )
}

pub fn build_desktop_insert_target_diagnostic_with_trace(
    target: ForegroundInsertTarget,
    context: Option<&DesktopInsertTargetContext>,
    output_strategy: Option<DesktopOutputStrategy>,
    restore: Option<DesktopInsertTargetRestoreDiagnostic>,
    trace: Option<DesktopInsertTargetTraceDiagnostic>,
) -> DesktopInsertTargetDiagnostic {
    let restore_attempted = restore.map(|item| item.attempted).unwrap_or(false);
    let restore_target_window_exists = restore.and_then(|item| item.target_window_exists);
    let restore_target_focus_exists = restore.and_then(|item| item.target_focus_exists);
    let restore_focus_requested = restore
        .map(|item| item.focus_restore_requested)
        .unwrap_or(false);
    let post_insert_release_reason = restore.and_then(|item| {
        item.post_insert_release_reason
            .map(|reason| reason.as_str().to_string())
    });
    let post_insert_wait_duration_ms = restore.and_then(|item| item.post_insert_wait_duration_ms);
    let post_insert_poll_count = restore.and_then(|item| item.post_insert_poll_count);
    let post_insert_target_foreground_poll_count =
        restore.and_then(|item| item.post_insert_target_foreground_poll_count);
    let post_insert_trailing_target_foreground_poll_count =
        restore.and_then(|item| item.post_insert_trailing_target_foreground_poll_count);
    let post_insert_required_stable_foreground_polls =
        restore.and_then(|item| item.post_insert_required_stable_foreground_polls);
    let output_strategy = output_strategy.map(|strategy| strategy.as_str().to_string());
    let focus_class_name = context.and_then(|item| item.focus_class_name.clone());
    let caret_window_handle = context
        .and_then(|item| item.caret_window_handle)
        .map(format_desktop_window_handle);
    let automation_control_type = context.and_then(|item| item.automation_control_type.clone());
    let automation_framework_id = context.and_then(|item| item.automation_framework_id.clone());
    let automation_runtime_id = context.and_then(|item| item.automation_runtime_id.clone());
    let automation_is_keyboard_focusable =
        context.and_then(|item| item.automation_is_keyboard_focusable);
    let automation_supports_text_pattern = context
        .map(|item| item.automation_supports_text_pattern)
        .unwrap_or(false);
    let automation_supports_value_pattern = context
        .map(|item| item.automation_supports_value_pattern)
        .unwrap_or(false);
    let focus_looks_editable = context.map(|item| desktop_insert_target_looks_editable(Some(item)));

    DesktopInsertTargetDiagnostic {
        captured_window_handle: format_desktop_window_handle(target.window_handle),
        captured_focus_handle: target.focus_handle.map(format_desktop_window_handle),
        captured_primary_focus_handle: target
            .primary_focus_handle
            .map(format_desktop_window_handle),
        captured_fallback_focus_handle: target
            .fallback_focus_handle
            .map(format_desktop_window_handle),
        captured_focus_source: target
            .focus_capture_source
            .map(|source| source.as_str().to_string()),
        output_strategy,
        focus_class_name,
        caret_window_handle,
        automation_control_type,
        automation_framework_id,
        automation_runtime_id,
        automation_is_keyboard_focusable,
        automation_supports_text_pattern,
        automation_supports_value_pattern,
        focus_looks_editable,
        restore_attempted,
        restore_target_window_exists,
        restore_target_focus_exists,
        restore_focus_requested,
        post_insert_release_reason,
        post_insert_wait_duration_ms,
        post_insert_poll_count,
        post_insert_target_foreground_poll_count,
        post_insert_trailing_target_foreground_poll_count,
        post_insert_required_stable_foreground_polls,
        trace,
    }
}

pub fn build_desktop_insert_target_trace_diagnostic(
    selected_origin_source: Option<&str>,
    origin_target: Option<&DesktopInsertTargetContext>,
    current_target: Option<&DesktopInsertTargetContext>,
    pending_hotkey_origin_target: Option<&DesktopInsertTargetContext>,
    release_time_origin_target: Option<&DesktopInsertTargetContext>,
) -> Option<DesktopInsertTargetTraceDiagnostic> {
    if selected_origin_source.is_none()
        && origin_target.is_none()
        && current_target.is_none()
        && pending_hotkey_origin_target.is_none()
        && release_time_origin_target.is_none()
    {
        return None;
    }

    let same_window_as_origin = match (origin_target, current_target) {
        (Some(origin_target), Some(current_target)) => Some(
            origin_target.target.map(|target| target.window_handle)
                == current_target.target.map(|target| target.window_handle),
        ),
        _ => None,
    };
    let same_control_by_handle = match (origin_target, current_target) {
        (Some(origin_target), Some(current_target)) => Some(
            desktop_same_insert_control_via_handles(origin_target, current_target),
        ),
        _ => None,
    };
    let same_control_by_runtime_id = match (origin_target, current_target) {
        (Some(origin_target), Some(current_target)) => Some(
            desktop_same_insert_control_via_runtime_id(origin_target, current_target),
        ),
        _ => None,
    };

    Some(DesktopInsertTargetTraceDiagnostic {
        selected_origin_source: selected_origin_source.map(str::to_string),
        origin_target: origin_target.map(build_desktop_insert_target_snapshot_diagnostic),
        current_target: current_target.map(build_desktop_insert_target_snapshot_diagnostic),
        pending_hotkey_origin_target: pending_hotkey_origin_target
            .map(build_desktop_insert_target_snapshot_diagnostic),
        release_time_origin_target: release_time_origin_target
            .map(build_desktop_insert_target_snapshot_diagnostic),
        same_window_as_origin,
        same_control_by_handle,
        same_control_by_runtime_id,
    })
}

pub fn build_desktop_insert_target_snapshot_diagnostic(
    target: &DesktopInsertTargetContext,
) -> DesktopInsertTargetSnapshotDiagnostic {
    DesktopInsertTargetSnapshotDiagnostic {
        window_handle: target
            .target
            .map(|target| format_desktop_window_handle(target.window_handle)),
        focus_handle: target
            .target
            .and_then(|target| target.focus_handle)
            .map(format_desktop_window_handle),
        focus_class_name: target.focus_class_name.clone(),
        caret_window_handle: target.caret_window_handle.map(format_desktop_window_handle),
        automation_control_type: target.automation_control_type.clone(),
        automation_framework_id: target.automation_framework_id.clone(),
        automation_runtime_id: target.automation_runtime_id.clone(),
        automation_is_keyboard_focusable: target.automation_is_keyboard_focusable,
        automation_supports_text_pattern: target.automation_supports_text_pattern,
        automation_supports_value_pattern: target.automation_supports_value_pattern,
        focus_looks_editable: desktop_insert_target_looks_editable(Some(target)),
    }
}

pub fn desktop_insert_target_diagnostic_path(session_log_path: &Path) -> PathBuf {
    let parent = session_log_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = session_log_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("session");
    parent.join(format!("{stem}.desktop-insert-target.json"))
}

pub fn write_desktop_insert_target_diagnostic(
    session_log_path: &Path,
    diagnostic: &DesktopInsertTargetDiagnostic,
) -> std::io::Result<PathBuf> {
    let path = desktop_insert_target_diagnostic_path(session_log_path);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let json = serde_json::to_string_pretty(diagnostic)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

pub fn parse_desktop_window_handle(value: &str) -> Result<isize, String> {
    if value.trim().is_empty() {
        return Err("desktop window handle must not be blank".to_string());
    }
    if value.trim() != value {
        return Err(
            "desktop window handle must not have leading or trailing whitespace".to_string(),
        );
    }

    let (digits, radix) = if let Some(stripped) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        if stripped.is_empty() {
            return Err("desktop window handle hex value is missing digits".to_string());
        }
        (stripped, 16)
    } else {
        (value, 10)
    };

    usize::from_str_radix(digits, radix)
        .map(|handle| handle as isize)
        .map_err(|_| format!("invalid desktop window handle: {value}"))
}

fn format_desktop_window_handle(handle: isize) -> String {
    format!("0x{:X}", handle as usize)
}

pub fn resolve_foreground_focus_handle(
    foreground_handle: isize,
    primary_focus_handle: Option<isize>,
    fallback_focus_handle: Option<isize>,
    shell_handle: isize,
    hud_handle: isize,
) -> Option<isize> {
    resolve_foreground_focus_capture(
        foreground_handle,
        primary_focus_handle,
        fallback_focus_handle,
        shell_handle,
        hud_handle,
    )
    .focus_handle
}

pub fn resolve_foreground_focus_capture(
    foreground_handle: isize,
    primary_focus_handle: Option<isize>,
    fallback_focus_handle: Option<isize>,
    shell_handle: isize,
    hud_handle: isize,
) -> ForegroundFocusCaptureResolution {
    let normalize = |handle: isize| {
        handle != 0 && handle != foreground_handle && handle != shell_handle && handle != hud_handle
    };

    if let Some(handle) = primary_focus_handle.filter(|handle| normalize(*handle)) {
        return ForegroundFocusCaptureResolution {
            focus_handle: Some(handle),
            source: Some(ForegroundFocusCaptureSource::GuiThreadInfo),
        };
    }
    if let Some(handle) = fallback_focus_handle.filter(|handle| normalize(*handle)) {
        return ForegroundFocusCaptureResolution {
            focus_handle: Some(handle),
            source: Some(ForegroundFocusCaptureSource::AttachedGetFocus),
        };
    }

    ForegroundFocusCaptureResolution {
        focus_handle: None,
        source: None,
    }
}

pub fn select_foreground_insert_target(
    foreground_handle: isize,
    focus_handle: Option<isize>,
    shell_handle: isize,
    hud_handle: isize,
) -> Option<ForegroundInsertTarget> {
    if foreground_handle == 0
        || foreground_handle == shell_handle
        || foreground_handle == hud_handle
    {
        return None;
    }

    let focus_handle = resolve_foreground_focus_handle(
        foreground_handle,
        focus_handle,
        None,
        shell_handle,
        hud_handle,
    );

    Some(ForegroundInsertTarget {
        window_handle: foreground_handle,
        focus_handle,
        primary_focus_handle: None,
        fallback_focus_handle: None,
        focus_capture_source: None,
    })
}

pub fn hydrate_foreground_insert_target_focus(
    target: ForegroundInsertTarget,
    primary_focus_handle: Option<isize>,
    fallback_focus_handle: Option<isize>,
    shell_handle: isize,
    hud_handle: isize,
) -> ForegroundInsertTarget {
    if target.focus_handle.is_some() {
        return target;
    }

    let refreshed = resolve_foreground_focus_capture(
        target.window_handle,
        primary_focus_handle,
        fallback_focus_handle,
        shell_handle,
        hud_handle,
    );
    let Some(focus_handle) = refreshed.focus_handle else {
        return target;
    };

    ForegroundInsertTarget {
        focus_handle: Some(focus_handle),
        primary_focus_handle,
        fallback_focus_handle,
        focus_capture_source: refreshed.source,
        ..target
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeySpec {
    key_name: String,
    virtual_key: u32,
    ctrl: ModifierRequirement,
    alt: ModifierRequirement,
    shift: ModifierRequirement,
    win: ModifierRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModifierRequirement {
    None,
    Either,
    Left,
    Right,
}

impl ModifierRequirement {
    fn is_required(self) -> bool {
        !matches!(self, Self::None)
    }

    fn is_side_specific(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    fn display_label(self, base: &'static str) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Either => Some(base),
            Self::Left => Some(match base {
                "Ctrl" => "LeftCtrl",
                "Alt" => "LeftAlt",
                "Shift" => "LeftShift",
                "Win" => "LeftWin",
                _ => base,
            }),
            Self::Right => Some(match base {
                "Ctrl" => "RightCtrl",
                "Alt" => "RightAlt",
                "Shift" => "RightShift",
                "Win" => "RightWin",
                _ => base,
            }),
        }
    }

    #[cfg(windows)]
    fn matches_pressed(self, key_down: &impl Fn(i32) -> bool, left_vk: i32, right_vk: i32) -> bool {
        match self {
            Self::None => true,
            Self::Either => key_down(left_vk) || key_down(right_vk),
            Self::Left => key_down(left_vk),
            Self::Right => key_down(right_vk),
        }
    }

    fn matches_low_level_pressed(
        self,
        pressed_keys: &HashSet<u32>,
        left_vk: u32,
        right_vk: u32,
    ) -> bool {
        match self {
            Self::None => true,
            Self::Either => pressed_keys.contains(&left_vk) || pressed_keys.contains(&right_vk),
            Self::Left => pressed_keys.contains(&left_vk),
            Self::Right => pressed_keys.contains(&right_vk),
        }
    }

    fn matches_virtual_key(self, virtual_key: u32, left_vk: u32, right_vk: u32) -> bool {
        match self {
            Self::None => false,
            Self::Either => virtual_key == left_vk || virtual_key == right_vk,
            Self::Left => virtual_key == left_vk,
            Self::Right => virtual_key == right_vk,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowLevelHotkeyTransition {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsHotkeyBindingStrategy {
    RegisterHotKey,
    LowLevelHook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsHotkeyBindingRegistrationPlan {
    RegisterHotKeyWithOriginCapture,
    LowLevelHook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopActionRoute {
    Primary,
    Transcribe,
    Document,
    Command,
    Generate,
    Smart,
    Translate,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopActionBinding {
    pub route: DesktopActionRoute,
    pub shortcut: HotkeySpec,
    pub mode_override: Option<VoiceMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleDesktopHotkeyRouterEvent {
    pub action_index: Option<usize>,
    pub consume: bool,
    pub pending_hold: ToggleDesktopHotkeyRouterPendingHold,
}

#[derive(Debug, Clone)]
pub struct ToggleDesktopHotkeyRouter {
    bindings: Vec<ToggleDesktopHotkeyRouterBinding>,
    pressed_keys: HashSet<u32>,
    pending_action: Option<ToggleDesktopHotkeyPendingAction>,
    abandoned_trigger_virtual_key: Option<u32>,
}

#[derive(Debug, Clone)]
struct ToggleDesktopHotkeyRouterBinding {
    action_index: usize,
    shortcut: HotkeySpec,
    pending_on_release: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleDesktopHotkeyRouterPendingHold {
    None,
    Start { trigger_virtual_key: u32 },
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToggleDesktopHotkeyPendingAction {
    action_index: usize,
    trigger_virtual_key: u32,
    suppress_on_release: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LowLevelHotkeyEvent {
    pub transition: Option<LowLevelHotkeyTransition>,
    pub consume: bool,
}

#[derive(Debug, Clone)]
pub struct LowLevelHotkeyTracker {
    spec: HotkeySpec,
    pressed_keys: HashSet<u32>,
}

impl LowLevelHotkeyTracker {
    pub fn new(spec: HotkeySpec) -> Self {
        Self {
            spec,
            pressed_keys: HashSet::new(),
        }
    }

    pub fn handle_key_event(&mut self, virtual_key: u32, is_key_down: bool) -> LowLevelHotkeyEvent {
        let previously_pressed = self.pressed_keys.clone();
        let was_active = self
            .spec
            .matches_low_level_pressed_keys(&previously_pressed);
        let was_pressed = self.pressed_keys.contains(&virtual_key);

        if is_key_down {
            self.pressed_keys.insert(virtual_key);
        } else {
            self.pressed_keys.remove(&virtual_key);
        }

        let consume = self.spec.should_consume_low_level_event(
            virtual_key,
            &previously_pressed,
            &self.pressed_keys,
        );
        let is_active = self.spec.matches_low_level_pressed_keys(&self.pressed_keys);
        let transition = if is_key_down && !was_pressed && !was_active && is_active {
            Some(LowLevelHotkeyTransition::Pressed)
        } else if was_active && !is_active {
            Some(LowLevelHotkeyTransition::Released)
        } else {
            None
        };

        LowLevelHotkeyEvent {
            transition,
            consume,
        }
    }
}

pub fn select_windows_hotkey_binding_strategy(spec: &HotkeySpec) -> WindowsHotkeyBindingStrategy {
    if spec.requires_low_level_hook() {
        WindowsHotkeyBindingStrategy::LowLevelHook
    } else {
        WindowsHotkeyBindingStrategy::RegisterHotKey
    }
}

pub fn windows_hotkey_binding_registration_plan(
    spec: &HotkeySpec,
) -> WindowsHotkeyBindingRegistrationPlan {
    match select_windows_hotkey_binding_strategy(spec) {
        WindowsHotkeyBindingStrategy::RegisterHotKey => {
            WindowsHotkeyBindingRegistrationPlan::RegisterHotKeyWithOriginCapture
        }
        WindowsHotkeyBindingStrategy::LowLevelHook => {
            WindowsHotkeyBindingRegistrationPlan::LowLevelHook
        }
    }
}

pub fn desktop_action_bindings(config: &TalkConfig) -> Result<Vec<DesktopActionBinding>, String> {
    let mut bindings = vec![DesktopActionBinding {
        route: DesktopActionRoute::Primary,
        shortcut: parse_hotkey(&config.trigger.toggle_shortcut)?,
        mode_override: Some(config.default_voice_mode()),
    }];

    push_desktop_mode_binding(
        &mut bindings,
        DesktopActionRoute::Transcribe,
        config.desktop.shortcuts.transcribe_shortcut.as_deref(),
        VoiceMode::Transcribe,
    )?;
    push_desktop_mode_binding(
        &mut bindings,
        DesktopActionRoute::Document,
        config.desktop.shortcuts.document_shortcut.as_deref(),
        VoiceMode::Document,
    )?;
    push_desktop_mode_binding(
        &mut bindings,
        DesktopActionRoute::Command,
        config.desktop.shortcuts.command_shortcut.as_deref(),
        VoiceMode::Command,
    )?;
    push_desktop_mode_binding(
        &mut bindings,
        DesktopActionRoute::Generate,
        config.desktop.shortcuts.generate_shortcut.as_deref(),
        VoiceMode::Generate,
    )?;
    push_desktop_mode_binding(
        &mut bindings,
        DesktopActionRoute::Smart,
        config.desktop.shortcuts.smart_shortcut.as_deref(),
        VoiceMode::Smart,
    )?;

    if let Some(raw_shortcut) = config.desktop.shortcuts.translate_shortcut.as_deref() {
        bindings.push(DesktopActionBinding {
            route: DesktopActionRoute::Translate,
            shortcut: parse_hotkey(raw_shortcut)?,
            mode_override: Some(VoiceMode::Translate),
        });
    }

    if let Some(raw_shortcut) = config.desktop.shortcuts.ask_shortcut.as_deref() {
        bindings.push(DesktopActionBinding {
            route: DesktopActionRoute::Ask,
            shortcut: parse_hotkey(raw_shortcut)?,
            mode_override: Some(VoiceMode::Command),
        });
    }

    Ok(bindings)
}

fn push_desktop_mode_binding(
    bindings: &mut Vec<DesktopActionBinding>,
    route: DesktopActionRoute,
    raw_shortcut: Option<&str>,
    mode: VoiceMode,
) -> Result<(), String> {
    let Some(raw_shortcut) = raw_shortcut else {
        return Ok(());
    };
    bindings.push(DesktopActionBinding {
        route,
        shortcut: parse_hotkey(raw_shortcut)?,
        mode_override: Some(mode),
    });
    Ok(())
}

pub fn desktop_action_binding_label(bindings: &[DesktopActionBinding]) -> String {
    bindings
        .iter()
        .map(|binding| binding.shortcut.display_name())
        .collect::<Vec<_>>()
        .join(" | ")
}

impl ToggleDesktopHotkeyRouter {
    pub fn new(bindings: &[DesktopActionBinding]) -> Self {
        let router_bindings = bindings
            .iter()
            .enumerate()
            .map(|(action_index, binding)| ToggleDesktopHotkeyRouterBinding {
                action_index,
                pending_on_release: binding.shortcut.is_modifier_only_shortcut()
                    && bindings.iter().any(|other| {
                        other.shortcut != binding.shortcut
                            && other.shortcut.uses_virtual_key_as_required_modifier(
                                binding.shortcut.virtual_key(),
                            )
                    }),
                shortcut: binding.shortcut.clone(),
            })
            .collect();

        Self {
            bindings: router_bindings,
            pressed_keys: HashSet::new(),
            pending_action: None,
            abandoned_trigger_virtual_key: None,
        }
    }

    pub fn activate_pending_hold_help(&mut self) -> bool {
        let Some(pending) = self.pending_action.as_mut() else {
            return false;
        };

        if pending.suppress_on_release {
            false
        } else {
            pending.suppress_on_release = true;
            true
        }
    }

    pub fn handle_key_event(
        &mut self,
        virtual_key: u32,
        is_key_down: bool,
    ) -> ToggleDesktopHotkeyRouterEvent {
        let previously_pressed = self.pressed_keys.clone();

        if is_key_down {
            let inserted = self.pressed_keys.insert(virtual_key);

            if self.abandoned_trigger_virtual_key.is_some() {
                return ToggleDesktopHotkeyRouterEvent {
                    action_index: None,
                    consume: false,
                    pending_hold: ToggleDesktopHotkeyRouterPendingHold::None,
                };
            }

            if !inserted {
                let consume = self.should_consume_low_level_event(
                    virtual_key,
                    &previously_pressed,
                    &self.pressed_keys,
                );
                return ToggleDesktopHotkeyRouterEvent {
                    action_index: None,
                    consume,
                    pending_hold: ToggleDesktopHotkeyRouterPendingHold::None,
                };
            }

            let consume = self.should_consume_low_level_event(
                virtual_key,
                &previously_pressed,
                &self.pressed_keys,
            );

            let newly_active = self
                .bindings
                .iter()
                .filter(|binding| {
                    !binding
                        .shortcut
                        .matches_low_level_pressed_keys(&previously_pressed)
                        && binding
                            .shortcut
                            .matches_low_level_pressed_keys(&self.pressed_keys)
                })
                .collect::<Vec<_>>();

            if let Some(best_match) = newly_active
                .iter()
                .filter(|binding| !binding.pending_on_release)
                .max_by_key(|binding| binding.shortcut.low_level_specificity_score())
            {
                let pending_hold = if self.pending_action.take().is_some() {
                    ToggleDesktopHotkeyRouterPendingHold::Cancelled
                } else {
                    ToggleDesktopHotkeyRouterPendingHold::None
                };
                return ToggleDesktopHotkeyRouterEvent {
                    action_index: Some(best_match.action_index),
                    consume,
                    pending_hold,
                };
            }

            if let Some(pending_match) = newly_active
                .iter()
                .filter(|binding| binding.pending_on_release)
                .max_by_key(|binding| binding.shortcut.low_level_specificity_score())
            {
                let pending = ToggleDesktopHotkeyPendingAction {
                    action_index: pending_match.action_index,
                    trigger_virtual_key: pending_match.shortcut.virtual_key(),
                    suppress_on_release: false,
                };
                let pending_hold = if self.pending_action != Some(pending) {
                    ToggleDesktopHotkeyRouterPendingHold::Start {
                        trigger_virtual_key: pending.trigger_virtual_key,
                    }
                } else {
                    ToggleDesktopHotkeyRouterPendingHold::None
                };
                self.pending_action = Some(pending);
                return ToggleDesktopHotkeyRouterEvent {
                    action_index: None,
                    consume,
                    pending_hold,
                };
            }

            if self.pending_action.is_some() && !is_modifier_virtual_key(virtual_key) {
                self.abandoned_trigger_virtual_key = self
                    .pending_action
                    .take()
                    .map(|pending| pending.trigger_virtual_key);
                return ToggleDesktopHotkeyRouterEvent {
                    action_index: None,
                    consume,
                    pending_hold: ToggleDesktopHotkeyRouterPendingHold::Cancelled,
                };
            }

            ToggleDesktopHotkeyRouterEvent {
                action_index: None,
                consume,
                pending_hold: ToggleDesktopHotkeyRouterPendingHold::None,
            }
        } else {
            let pending_action = self.pending_action;
            let pending_match_was_active = pending_action.and_then(|pending| {
                self.bindings
                    .iter()
                    .find(|binding| binding.action_index == pending.action_index)
                    .map(|binding| {
                        binding
                            .shortcut
                            .matches_low_level_pressed_keys(&previously_pressed)
                            && pending.trigger_virtual_key == virtual_key
                    })
            });

            self.pressed_keys.remove(&virtual_key);

            if self.abandoned_trigger_virtual_key == Some(virtual_key) {
                self.abandoned_trigger_virtual_key = None;
                return ToggleDesktopHotkeyRouterEvent {
                    action_index: None,
                    consume: false,
                    pending_hold: ToggleDesktopHotkeyRouterPendingHold::None,
                };
            }
            if self.abandoned_trigger_virtual_key.is_some() {
                return ToggleDesktopHotkeyRouterEvent {
                    action_index: None,
                    consume: false,
                    pending_hold: ToggleDesktopHotkeyRouterPendingHold::None,
                };
            }

            let pending_hold = if pending_match_was_active == Some(true) {
                ToggleDesktopHotkeyRouterPendingHold::Cancelled
            } else {
                ToggleDesktopHotkeyRouterPendingHold::None
            };
            let consume = self.should_consume_low_level_event(
                virtual_key,
                &previously_pressed,
                &self.pressed_keys,
            );
            let action_index = if pending_match_was_active == Some(true) {
                self.pending_action
                    .take()
                    .filter(|pending| !pending.suppress_on_release)
                    .map(|pending| pending.action_index)
            } else {
                None
            };

            ToggleDesktopHotkeyRouterEvent {
                action_index,
                consume,
                pending_hold,
            }
        }
    }

    fn should_consume_low_level_event(
        &self,
        virtual_key: u32,
        previously_pressed: &HashSet<u32>,
        currently_pressed: &HashSet<u32>,
    ) -> bool {
        self.bindings.iter().any(|binding| {
            binding.shortcut.should_consume_low_level_event(
                virtual_key,
                previously_pressed,
                currently_pressed,
            )
        })
    }
}

impl HotkeySpec {
    pub fn trigger_key_name(&self) -> &str {
        &self.key_name
    }

    pub fn virtual_key(&self) -> u32 {
        self.virtual_key
    }

    pub fn has_ctrl(&self) -> bool {
        self.ctrl.is_required()
    }

    pub fn has_alt(&self) -> bool {
        self.alt.is_required()
    }

    pub fn has_shift(&self) -> bool {
        self.shift.is_required()
    }

    pub fn has_win(&self) -> bool {
        self.win.is_required()
    }

    pub fn requires_low_level_hook(&self) -> bool {
        self.ctrl.is_side_specific()
            || self.alt.is_side_specific()
            || self.shift.is_side_specific()
            || self.win.is_side_specific()
            || matches!(self.virtual_key, 0xA2..=0xA5)
    }

    pub fn display_name(&self) -> String {
        let mut parts = Vec::new();
        if let Some(label) = self.ctrl.display_label("Ctrl") {
            parts.push(label);
        }
        if let Some(label) = self.alt.display_label("Alt") {
            parts.push(label);
        }
        if let Some(label) = self.shift.display_label("Shift") {
            parts.push(label);
        }
        if let Some(label) = self.win.display_label("Win") {
            parts.push(label);
        }
        parts.push(self.trigger_key_name());
        parts.join("+")
    }

    #[cfg(windows)]
    pub fn modifier_mask(&self) -> u32 {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
        };

        let mut mask = MOD_NOREPEAT;
        if self.ctrl.is_required() {
            mask |= MOD_CONTROL;
        }
        if self.alt.is_required() {
            mask |= MOD_ALT;
        }
        if self.shift.is_required() {
            mask |= MOD_SHIFT;
        }
        if self.win.is_required() {
            mask |= MOD_WIN;
        }
        mask
    }

    #[cfg(windows)]
    pub fn is_pressed(&self) -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU,
            VK_RSHIFT, VK_RWIN,
        };

        let key_down = |vk: i32| unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };
        let modifiers_ok =
            self.ctrl
                .matches_pressed(&key_down, VK_LCONTROL as i32, VK_RCONTROL as i32)
                && self
                    .alt
                    .matches_pressed(&key_down, VK_LMENU as i32, VK_RMENU as i32)
                && self
                    .shift
                    .matches_pressed(&key_down, VK_LSHIFT as i32, VK_RSHIFT as i32)
                && self
                    .win
                    .matches_pressed(&key_down, VK_LWIN as i32, VK_RWIN as i32);

        modifiers_ok && key_down(self.virtual_key as i32)
    }

    fn matches_low_level_pressed_keys(&self, pressed_keys: &HashSet<u32>) -> bool {
        self.ctrl
            .matches_low_level_pressed(pressed_keys, 0xA2, 0xA3)
            && self.alt.matches_low_level_pressed(pressed_keys, 0xA4, 0xA5)
            && self
                .shift
                .matches_low_level_pressed(pressed_keys, 0xA0, 0xA1)
            && self.win.matches_low_level_pressed(pressed_keys, 0x5B, 0x5C)
            && pressed_keys.contains(&self.virtual_key)
    }

    fn matches_low_level_prefix_keys(&self, pressed_keys: &HashSet<u32>) -> bool {
        if pressed_keys.is_empty() {
            return false;
        }
        if pressed_keys
            .iter()
            .any(|virtual_key| !self.should_consume_low_level_key(*virtual_key))
        {
            return false;
        }

        let modifiers_ready = self
            .ctrl
            .matches_low_level_pressed(pressed_keys, 0xA2, 0xA3)
            && self.alt.matches_low_level_pressed(pressed_keys, 0xA4, 0xA5)
            && self
                .shift
                .matches_low_level_pressed(pressed_keys, 0xA0, 0xA1)
            && self.win.matches_low_level_pressed(pressed_keys, 0x5B, 0x5C);

        if pressed_keys.contains(&self.virtual_key) {
            return modifiers_ready;
        }

        modifiers_ready && (self.has_ctrl() || self.has_alt() || self.has_shift() || self.has_win())
    }

    fn should_consume_low_level_key(&self, virtual_key: u32) -> bool {
        self.virtual_key == virtual_key
            || self.ctrl.matches_virtual_key(virtual_key, 0xA2, 0xA3)
            || self.alt.matches_virtual_key(virtual_key, 0xA4, 0xA5)
            || self.shift.matches_virtual_key(virtual_key, 0xA0, 0xA1)
            || self.win.matches_virtual_key(virtual_key, 0x5B, 0x5C)
    }

    fn should_consume_low_level_event(
        &self,
        virtual_key: u32,
        previously_pressed: &HashSet<u32>,
        currently_pressed: &HashSet<u32>,
    ) -> bool {
        self.should_consume_low_level_key(virtual_key)
            && (self.matches_low_level_prefix_keys(previously_pressed)
                || self.matches_low_level_prefix_keys(currently_pressed))
    }

    fn low_level_specificity_score(&self) -> usize {
        self.has_ctrl() as usize
            + self.has_alt() as usize
            + self.has_shift() as usize
            + self.has_win() as usize
            + 1
    }

    fn is_modifier_only_shortcut(&self) -> bool {
        matches!(self.virtual_key, 0xA0..=0xA5 | 0x5B..=0x5C)
            && !self.has_ctrl()
            && !self.has_alt()
            && !self.has_shift()
            && !self.has_win()
    }

    fn uses_virtual_key_as_required_modifier(&self, virtual_key: u32) -> bool {
        self.ctrl.matches_virtual_key(virtual_key, 0xA2, 0xA3)
            || self.alt.matches_virtual_key(virtual_key, 0xA4, 0xA5)
            || self.shift.matches_virtual_key(virtual_key, 0xA0, 0xA1)
            || self.win.matches_virtual_key(virtual_key, 0x5B, 0x5C)
    }
}

fn is_modifier_virtual_key(virtual_key: u32) -> bool {
    matches!(virtual_key, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellActivity {
    Idle,
    Recording,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellState {
    activity: ShellActivity,
}

impl ShellState {
    pub fn idle() -> Self {
        Self {
            activity: ShellActivity::Idle,
        }
    }

    pub fn begin_recording(self) -> Option<Self> {
        match self.activity {
            ShellActivity::Idle => Some(Self {
                activity: ShellActivity::Recording,
            }),
            ShellActivity::Recording | ShellActivity::Busy => None,
        }
    }

    pub fn set_busy(self) -> Self {
        Self {
            activity: ShellActivity::Busy,
        }
    }

    pub fn complete(self) -> Self {
        let _ = self;
        Self::idle()
    }

    pub fn can_start_session(&self) -> bool {
        self.activity == ShellActivity::Idle
    }

    pub fn can_stop_session(&self) -> bool {
        self.activity == ShellActivity::Recording
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigAvailability {
    Ready,
    Unavailable { reason: String },
}

impl ConfigAvailability {
    pub fn ready() -> Self {
        Self::Ready
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Ready => None,
            Self::Unavailable { reason } => Some(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyBindingState {
    Active {
        primary_spec: HotkeySpec,
        shortcut_label: String,
    },
    InvalidConfig {
        raw: String,
        reason: String,
    },
    RegistrationFailed {
        shortcut_label: String,
        reason: String,
    },
    Unconfigured,
}

impl HotkeyBindingState {
    pub fn active(spec: HotkeySpec) -> Self {
        let shortcut_label = spec.display_name();
        Self::Active {
            primary_spec: spec,
            shortcut_label,
        }
    }

    pub fn active_with_label(spec: HotkeySpec, shortcut_label: impl Into<String>) -> Self {
        Self::Active {
            primary_spec: spec,
            shortcut_label: shortcut_label.into(),
        }
    }

    pub fn invalid_config(raw: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidConfig {
            raw: raw.into(),
            reason: reason.into(),
        }
    }

    pub fn registration_failed(
        shortcut_label: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::RegistrationFailed {
            shortcut_label: shortcut_label.into(),
            reason: reason.into(),
        }
    }

    pub fn shortcut_label(&self) -> Option<String> {
        match self {
            Self::Active { shortcut_label, .. } => Some(shortcut_label.clone()),
            Self::RegistrationFailed { shortcut_label, .. } => Some(shortcut_label.clone()),
            Self::InvalidConfig { raw, .. } => Some(raw.clone()),
            Self::Unconfigured => None,
        }
    }

    pub fn spec(&self) -> Option<&HotkeySpec> {
        match self {
            Self::Active { primary_spec, .. } => Some(primary_spec),
            _ => None,
        }
    }

    pub fn can_retry_registration(&self) -> bool {
        matches!(self, Self::RegistrationFailed { .. })
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::InvalidConfig { reason, .. } => Some(reason),
            Self::RegistrationFailed { reason, .. } => Some(reason),
            Self::Active { .. } | Self::Unconfigured => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuModel {
    pub hotkey_label: String,
    pub detail_label: Option<String>,
    pub start_enabled: bool,
    pub stop_enabled: bool,
    pub cancel_enabled: bool,
    pub reload_config_enabled: bool,
    pub open_config_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastSessionStatus {
    pub summary: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBackendSnapshot {
    pub configured_backend: String,
    pub status: Option<NativeReadinessStatus>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeReadinessSnapshot {
    pub audio: NativeBackendSnapshot,
    pub clipboard: NativeBackendSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub current_summary: String,
    pub current_detail: Option<String>,
    pub config_path: String,
    pub logs_dir: String,
    pub hotkey_label: String,
    pub hotkey_detail: Option<String>,
    pub last_session: Option<LastSessionStatus>,
    pub native_readiness: Option<NativeReadinessSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopHudVisualState {
    Listening,
    Thinking,
    Success,
    Error,
    Cancelled,
    Informational,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopHudViewModel {
    pub visual_state: DesktopHudVisualState,
    pub title: String,
    pub detail: Option<String>,
    pub meter: Option<DesktopHudAudioMeterModel>,
    pub progress_percent: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopSpeculativeTranscriptState {
    Partial,
    LocalFinal,
    CloudCorrecting,
    CloudCorrected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSpeculativeTranscriptViewModel {
    pub text: String,
    pub opacity_percent: u8,
    pub show_cloud_corrected_mark: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopTextLifecycleState {
    AudioWave,
    PreRecognized,
    Corrected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopTextLifecycleViewModel {
    pub text: Option<String>,
    pub text_rgb: Option<[u8; 3]>,
    pub insertable_to_target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopModeTextPaneLayout {
    SingleProcessingText,
    DualTranscriptAndResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopModeTextPane {
    pub label: String,
    pub text: String,
    pub lifecycle: DesktopTextLifecycleState,
    pub text_rgb: [u8; 3],
    pub insertable_to_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopModeTextResultModel {
    pub layout: DesktopModeTextPaneLayout,
    pub panes: Vec<DesktopModeTextPane>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopModeDropdownEntry {
    pub mode: VoiceMode,
    pub label: String,
    pub shortcut_hint: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopModeDropdownModel {
    pub title: String,
    pub current_label: String,
    pub entries: Vec<DesktopModeDropdownEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopModeOutputPolicy {
    pub insert_corrected_segments: bool,
    pub insert_generated_result: bool,
    pub insert_command_result: bool,
    pub show_command_result_in_gui: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopRuntimeInsertDirective {
    UseConfiguredOutput,
    DryRunOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopRuntimeInsertPlan {
    pub directive: DesktopRuntimeInsertDirective,
    pub show_result_in_gui: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopDocumentRecorrectionDecision {
    AutoApplyToTarget,
    ShowInTalkGuiOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSpeculativePipelineConfig {
    pub enabled: bool,
    pub local_asr: String,
    pub cloud_correction: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopSpeculativeCorrectionOutputTarget {
    PatchInsertedText(SpeculativeInsertAnchor),
    CopyPopupOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSpeculativeCorrectionJobModel {
    pub segment_id: String,
    pub local_text: String,
    pub context_before: String,
    pub output_target: DesktopSpeculativeCorrectionOutputTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopLiveStreamingLocalSegmentPlan {
    Insert {
        segment_id: String,
        text: String,
        insert_target: ForegroundInsertTarget,
    },
    DeferToStop {
        segment_id: String,
        text: String,
    },
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopStreamingStopPolicy {
    pub insert_final_transcript: bool,
    pub allow_final_correction_job: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopSpeculativeLocalAsrRoute {
    Disabled,
    MockPreview,
    ExternalCommand,
    StreamingService,
    Unsupported,
}

impl Default for DesktopSpeculativePipelineConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            local_asr: "mock".to_string(),
            cloud_correction: "disabled".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopCopyPopupPaneModel {
    pub label: String,
    pub text: String,
    pub editable: bool,
    pub copy_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopCopyPopupModel {
    pub title: String,
    pub editable_text: String,
    pub copy_label: String,
    pub panes: Vec<DesktopCopyPopupPaneModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopShortcutHelpEntry {
    pub title: String,
    pub shortcut: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopShortcutHelpModel {
    pub title: String,
    pub detail: String,
    pub entries: Vec<DesktopShortcutHelpEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopHudAudioMeterModel {
    pub bar_heights: [i32; 9],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopHudThinkingProgressModel {
    pub fill_percent: u8,
    pub text_wave_offset_px: i8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopHudThinkingPalette {
    pub track_start_rgb: [u8; 3],
    pub track_end_rgb: [u8; 3],
    pub fill_start_rgb: [u8; 3],
    pub fill_end_rgb: [u8; 3],
    pub fill_head_rgb: [u8; 3],
    pub border_rgb: [u8; 3],
    pub text_rgb: [u8; 3],
    pub text_shadow_rgb: [u8; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopCopyPopupAction {
    CopyToClipboard,
    Close,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopOverlayActivationPolicy {
    NoActivate,
    ActivateOnInteract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopHudPresentation {
    Hidden,
    Visible { auto_hide_ms: Option<u32> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopOverlayPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopOverlayRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl DesktopOverlayRect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopCopyPopupPaneLayout {
    pub label_rect: DesktopOverlayRect,
    pub editor_rect: DesktopOverlayRect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopCopyPopupMetrics {
    pub width: i32,
    pub height: i32,
    pub bottom_margin: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopShortcutHelpMetrics {
    pub width: i32,
    pub height: i32,
    pub bottom_margin: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopHudMetrics {
    pub width: i32,
    pub height: i32,
    pub bottom_margin: i32,
    pub corner_radius: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopListeningHudAction {
    Cancel,
    Complete,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopListeningHudPartialTextLayout {
    pub text_rect: DesktopOverlayRect,
    pub waveform_rect: DesktopOverlayRect,
    pub line_count: u8,
    pub wraps_text: bool,
    pub scrolls_text: bool,
    pub scrollbar_rect: Option<DesktopOverlayRect>,
    pub visible_text_units: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopRecordingStopWatcherPolicy {
    ManualOnly,
    TimeoutAfterSeconds(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopInsertTargetContext {
    pub target: Option<ForegroundInsertTarget>,
    pub focus_class_name: Option<String>,
    pub caret_window_handle: Option<isize>,
    pub automation_control_type: Option<String>,
    pub automation_framework_id: Option<String>,
    pub automation_runtime_id: Option<Vec<i32>>,
    pub automation_is_keyboard_focusable: Option<bool>,
    pub automation_supports_text_pattern: bool,
    pub automation_supports_value_pattern: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopOutputStrategy {
    HonorConfiguredOutput,
    ShowCopyPopupOnly,
}

impl DesktopOutputStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HonorConfiguredOutput => "honor_configured_output",
            Self::ShowCopyPopupOnly => "show_copy_popup_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopOutputPlan {
    pub strategy: DesktopOutputStrategy,
    pub insert_target: Option<ForegroundInsertTarget>,
}

pub fn desktop_hud_view_model_for_listening_level(level: f32) -> DesktopHudViewModel {
    DesktopHudViewModel {
        visual_state: DesktopHudVisualState::Listening,
        title: "Listening".to_string(),
        detail: Some(DESKTOP_LISTENING_LOCAL_DETECTION_PLACEHOLDER.to_string()),
        meter: Some(desktop_hud_audio_meter_model(level)),
        progress_percent: None,
    }
}

pub fn desktop_hud_view_model_for_listening_waveform(
    waveform_bins: [f32; 9],
) -> DesktopHudViewModel {
    desktop_hud_view_model_for_listening_waveform_with_partial(waveform_bins, None)
}

pub fn desktop_hud_view_model_for_listening_waveform_with_partial(
    waveform_bins: [f32; 9],
    partial_text: Option<&str>,
) -> DesktopHudViewModel {
    let detail = partial_text
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let detail = detail.or_else(|| Some(DESKTOP_LISTENING_LOCAL_DETECTION_PLACEHOLDER.to_string()));

    DesktopHudViewModel {
        visual_state: DesktopHudVisualState::Listening,
        title: "Listening".to_string(),
        detail,
        meter: Some(desktop_hud_audio_meter_model_for_waveform(waveform_bins)),
        progress_percent: None,
    }
}

pub fn desktop_streaming_hud_transcript(
    committed_segments: &[(&str, &str)],
    current_partial: Option<(&str, &str)>,
) -> String {
    let mut ordered_segments = Vec::<(&str, &str)>::new();

    for (segment_id, text) in committed_segments {
        desktop_streaming_hud_upsert_segment(&mut ordered_segments, segment_id, text);
    }
    if let Some((segment_id, text)) = current_partial {
        desktop_streaming_hud_upsert_segment(&mut ordered_segments, segment_id, text);
    }

    ordered_segments
        .iter()
        .map(|(_, text)| *text)
        .collect::<String>()
}

fn desktop_streaming_hud_upsert_segment<'a>(
    segments: &mut Vec<(&'a str, &'a str)>,
    segment_id: &'a str,
    text: &'a str,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }

    if let Some(existing) = segments
        .iter_mut()
        .find(|(existing_segment_id, _)| *existing_segment_id == segment_id)
    {
        *existing = (segment_id, text);
    } else {
        segments.push((segment_id, text));
    }
}

pub fn desktop_speculative_transcript_view_model(
    state: DesktopSpeculativeTranscriptState,
    text: &str,
) -> DesktopSpeculativeTranscriptViewModel {
    let opacity_percent = match state {
        DesktopSpeculativeTranscriptState::Partial => 62,
        DesktopSpeculativeTranscriptState::LocalFinal
        | DesktopSpeculativeTranscriptState::CloudCorrecting => 88,
        DesktopSpeculativeTranscriptState::CloudCorrected => 100,
    };

    DesktopSpeculativeTranscriptViewModel {
        text: text.to_string(),
        opacity_percent,
        show_cloud_corrected_mark: state == DesktopSpeculativeTranscriptState::CloudCorrected,
    }
}

pub fn desktop_text_lifecycle_view_model(
    state: DesktopTextLifecycleState,
    text: &str,
) -> DesktopTextLifecycleViewModel {
    match state {
        DesktopTextLifecycleState::AudioWave => DesktopTextLifecycleViewModel {
            text: None,
            text_rgb: None,
            insertable_to_target: false,
        },
        DesktopTextLifecycleState::PreRecognized => DesktopTextLifecycleViewModel {
            text: Some(text.to_string()),
            text_rgb: Some([245, 190, 72]),
            insertable_to_target: false,
        },
        DesktopTextLifecycleState::Corrected => DesktopTextLifecycleViewModel {
            text: Some(text.to_string()),
            text_rgb: Some([245, 247, 250]),
            insertable_to_target: true,
        },
    }
}

pub fn desktop_mode_text_pane_layout(
    mode: VoiceMode,
    smart_routed_mode: Option<VoiceMode>,
) -> DesktopModeTextPaneLayout {
    let effective_mode = effective_desktop_mode(mode, smart_routed_mode);
    match effective_mode {
        VoiceMode::Command | VoiceMode::Generate => {
            DesktopModeTextPaneLayout::DualTranscriptAndResult
        }
        _ => DesktopModeTextPaneLayout::SingleProcessingText,
    }
}

pub fn desktop_mode_dropdown_model(current_mode: VoiceMode) -> DesktopModeDropdownModel {
    let current_mode = canonical_desktop_voice_mode(current_mode);
    let entries = [
        (VoiceMode::Smart, "智能", "RightCtrl+5"),
        (VoiceMode::Transcribe, "转录", "RightCtrl+1"),
        (VoiceMode::Document, "公文", "RightCtrl+2"),
        (VoiceMode::Command, "命令", "RightCtrl+3"),
        (VoiceMode::Generate, "生成", "RightCtrl+4"),
    ]
    .into_iter()
    .map(|(mode, label, shortcut_hint)| DesktopModeDropdownEntry {
        mode,
        label: label.to_string(),
        shortcut_hint: Some(shortcut_hint.to_string()),
        selected: mode == current_mode,
    })
    .collect::<Vec<_>>();
    let current_label = entries
        .iter()
        .find(|entry| entry.selected)
        .map(|entry| entry.label.clone())
        .unwrap_or_else(|| "智能".to_string());

    DesktopModeDropdownModel {
        title: "模式".to_string(),
        current_label,
        entries,
    }
}

pub fn desktop_mode_text_result_model(
    mode: VoiceMode,
    smart_routed_mode: Option<VoiceMode>,
    transcript: &str,
    transcript_state: DesktopTextLifecycleState,
    result: &str,
    result_state: DesktopTextLifecycleState,
) -> DesktopModeTextResultModel {
    let layout = desktop_mode_text_pane_layout(mode, smart_routed_mode);
    let panes = match layout {
        DesktopModeTextPaneLayout::SingleProcessingText => {
            let (text, lifecycle) = if result.trim().is_empty() {
                (transcript, transcript_state)
            } else {
                (result, result_state)
            };
            vec![desktop_mode_text_pane("文本", text, lifecycle)]
        }
        DesktopModeTextPaneLayout::DualTranscriptAndResult => vec![
            desktop_mode_text_pane("转录", transcript, transcript_state),
            desktop_mode_text_pane("结果", result, result_state),
        ],
    };

    DesktopModeTextResultModel { layout, panes }
}

pub fn desktop_mode_text_result_popup_text(model: &DesktopModeTextResultModel) -> String {
    match model.layout {
        DesktopModeTextPaneLayout::SingleProcessingText => model
            .panes
            .first()
            .map(|pane| pane.text.clone())
            .unwrap_or_default(),
        DesktopModeTextPaneLayout::DualTranscriptAndResult => model
            .panes
            .iter()
            .filter(|pane| !pane.text.trim().is_empty())
            .map(|pane| format!("{}\n{}", pane.label, pane.text))
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

pub fn desktop_copy_popup_model_for_mode_text_result(
    model: &DesktopModeTextResultModel,
) -> DesktopCopyPopupModel {
    let default_index = match model.layout {
        DesktopModeTextPaneLayout::SingleProcessingText => 0,
        DesktopModeTextPaneLayout::DualTranscriptAndResult => model
            .panes
            .iter()
            .position(|pane| pane.label == "结果")
            .unwrap_or_else(|| model.panes.len().saturating_sub(1)),
    };
    let panes = model
        .panes
        .iter()
        .enumerate()
        .map(|(index, pane)| {
            let copy_default = index == default_index;
            DesktopCopyPopupPaneModel {
                label: pane.label.clone(),
                text: pane.text.trim().to_string(),
                editable: model.layout == DesktopModeTextPaneLayout::SingleProcessingText
                    || copy_default,
                copy_default,
            }
        })
        .collect::<Vec<_>>();
    let editable_text = panes
        .iter()
        .find(|pane| pane.copy_default)
        .or_else(|| panes.first())
        .map(|pane| pane.text.clone())
        .unwrap_or_default();

    DesktopCopyPopupModel {
        title: String::new(),
        editable_text,
        copy_label: "复制".to_string(),
        panes,
    }
}

fn desktop_mode_text_pane(
    label: &str,
    text: &str,
    lifecycle: DesktopTextLifecycleState,
) -> DesktopModeTextPane {
    let view_model = desktop_text_lifecycle_view_model(lifecycle, text);
    DesktopModeTextPane {
        label: label.to_string(),
        text: text.to_string(),
        lifecycle,
        text_rgb: view_model.text_rgb.unwrap_or([245, 247, 250]),
        insertable_to_target: view_model.insertable_to_target,
    }
}

pub fn desktop_mode_output_policy(
    mode: VoiceMode,
    smart_routed_mode: Option<VoiceMode>,
) -> DesktopModeOutputPolicy {
    let effective_mode = effective_desktop_mode(mode, smart_routed_mode);
    match effective_mode {
        VoiceMode::Command => DesktopModeOutputPolicy {
            insert_corrected_segments: false,
            insert_generated_result: false,
            insert_command_result: false,
            show_command_result_in_gui: true,
        },
        VoiceMode::Generate => DesktopModeOutputPolicy {
            insert_corrected_segments: false,
            insert_generated_result: true,
            insert_command_result: false,
            show_command_result_in_gui: false,
        },
        _ => DesktopModeOutputPolicy {
            insert_corrected_segments: true,
            insert_generated_result: false,
            insert_command_result: false,
            show_command_result_in_gui: false,
        },
    }
}

pub fn desktop_runtime_insert_directive_for_mode(
    mode: VoiceMode,
    smart_routed_mode: Option<VoiceMode>,
    output_strategy: DesktopOutputStrategy,
    lifecycle_state: DesktopTextLifecycleState,
) -> DesktopRuntimeInsertPlan {
    if output_strategy == DesktopOutputStrategy::ShowCopyPopupOnly {
        return DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::DryRunOnly,
            show_result_in_gui: true,
        };
    }

    if lifecycle_state != DesktopTextLifecycleState::Corrected {
        return DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::DryRunOnly,
            show_result_in_gui: false,
        };
    }

    let policy = desktop_mode_output_policy(mode, smart_routed_mode);
    if policy.insert_command_result {
        return DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::UseConfiguredOutput,
            show_result_in_gui: policy.show_command_result_in_gui,
        };
    }
    if policy.show_command_result_in_gui {
        return DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::DryRunOnly,
            show_result_in_gui: true,
        };
    }
    if policy.insert_corrected_segments || policy.insert_generated_result {
        return DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::UseConfiguredOutput,
            show_result_in_gui: false,
        };
    }

    DesktopRuntimeInsertPlan {
        directive: DesktopRuntimeInsertDirective::DryRunOnly,
        show_result_in_gui: false,
    }
}

fn effective_desktop_mode(mode: VoiceMode, smart_routed_mode: Option<VoiceMode>) -> VoiceMode {
    let mode = canonical_desktop_voice_mode(mode);
    if mode == VoiceMode::Smart {
        smart_routed_mode
            .map(canonical_desktop_voice_mode)
            .unwrap_or(VoiceMode::Transcribe)
    } else {
        mode
    }
}

fn canonical_desktop_voice_mode(mode: VoiceMode) -> VoiceMode {
    match mode {
        VoiceMode::Dictate => VoiceMode::Transcribe,
        VoiceMode::Polish | VoiceMode::Translate => VoiceMode::Document,
        other => other,
    }
}

pub fn desktop_document_recorrection_decision(
    originally_inserted_text: &str,
    current_target_text: &str,
    target_still_safe: bool,
) -> DesktopDocumentRecorrectionDecision {
    if target_still_safe && originally_inserted_text == current_target_text {
        DesktopDocumentRecorrectionDecision::AutoApplyToTarget
    } else {
        DesktopDocumentRecorrectionDecision::ShowInTalkGuiOnly
    }
}

pub fn desktop_document_recorrection_session_decision(
    inserted_segments: &[String],
    current_target_text: &str,
    target_still_safe: bool,
) -> DesktopDocumentRecorrectionDecision {
    let originally_inserted_text = inserted_segments.concat();
    desktop_document_recorrection_decision(
        &originally_inserted_text,
        current_target_text,
        target_still_safe,
    )
}

pub fn desktop_speculative_pipeline_enabled(config: &DesktopSpeculativePipelineConfig) -> bool {
    config.enabled && !config.local_asr.trim().is_empty()
}

pub fn desktop_speculative_local_asr_route(
    config: &DesktopSpeculativePipelineConfig,
) -> DesktopSpeculativeLocalAsrRoute {
    if !desktop_speculative_pipeline_enabled(config) {
        return DesktopSpeculativeLocalAsrRoute::Disabled;
    }

    match config.local_asr.trim().to_ascii_lowercase().as_str() {
        "mock" => DesktopSpeculativeLocalAsrRoute::MockPreview,
        "external_command" => DesktopSpeculativeLocalAsrRoute::ExternalCommand,
        "streaming_service" => DesktopSpeculativeLocalAsrRoute::StreamingService,
        _ => DesktopSpeculativeLocalAsrRoute::Unsupported,
    }
}

pub fn recording_stop_watcher_policy(
    trigger_mode: TriggerMode,
    max_recording_seconds: u64,
) -> DesktopRecordingStopWatcherPolicy {
    match trigger_mode {
        TriggerMode::Toggle => DesktopRecordingStopWatcherPolicy::ManualOnly,
        TriggerMode::PushToTalk => {
            DesktopRecordingStopWatcherPolicy::TimeoutAfterSeconds(max_recording_seconds)
        }
    }
}

pub fn desktop_speculative_cloud_correction_enabled(
    config: &DesktopSpeculativePipelineConfig,
) -> bool {
    desktop_speculative_pipeline_enabled(config)
        && config
            .cloud_correction
            .trim()
            .eq_ignore_ascii_case("provider_text_processor")
}

pub fn desktop_speculative_correction_job_model(
    config: &DesktopSpeculativePipelineConfig,
    event: &SpeculativeRuntimeEvent,
    insert_target: Option<ForegroundInsertTarget>,
    inserted_at_ms: u64,
) -> Option<DesktopSpeculativeCorrectionJobModel> {
    if !desktop_speculative_cloud_correction_enabled(config) {
        return None;
    }

    let SpeculativeRuntimeEvent::CorrectionRequested {
        segment_id,
        local_text,
        context_before,
    } = event
    else {
        return None;
    };
    if segment_id.trim().is_empty() || local_text.trim().is_empty() {
        return None;
    }

    let output_target = match insert_target {
        Some(target) => DesktopSpeculativeCorrectionOutputTarget::PatchInsertedText(
            SpeculativeInsertAnchor::new(
                target.window_handle,
                target.focus_handle,
                segment_id.as_str(),
                local_text.as_str(),
                inserted_at_ms,
            )
            .ok()?,
        ),
        None => DesktopSpeculativeCorrectionOutputTarget::CopyPopupOnly,
    };

    Some(DesktopSpeculativeCorrectionJobModel {
        segment_id: segment_id.clone(),
        local_text: local_text.clone(),
        context_before: context_before.clone(),
        output_target,
    })
}

pub fn live_streaming_local_segment_plan(
    output_mode: OutputMode,
    event: &SpeculativeRuntimeEvent,
    origin_target: Option<&DesktopInsertTargetContext>,
    current_target: Option<&DesktopInsertTargetContext>,
) -> DesktopLiveStreamingLocalSegmentPlan {
    live_streaming_segment_plan_for_lifecycle(
        output_mode,
        event,
        origin_target,
        current_target,
        DesktopTextLifecycleState::PreRecognized,
    )
}

pub fn live_streaming_segment_plan_for_lifecycle(
    output_mode: OutputMode,
    event: &SpeculativeRuntimeEvent,
    origin_target: Option<&DesktopInsertTargetContext>,
    current_target: Option<&DesktopInsertTargetContext>,
    lifecycle_state: DesktopTextLifecycleState,
) -> DesktopLiveStreamingLocalSegmentPlan {
    let SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text } = event else {
        return DesktopLiveStreamingLocalSegmentPlan::Ignore;
    };
    if segment_id.trim().is_empty() || text.trim().is_empty() {
        return DesktopLiveStreamingLocalSegmentPlan::Ignore;
    }
    if lifecycle_state != DesktopTextLifecycleState::Corrected {
        return DesktopLiveStreamingLocalSegmentPlan::DeferToStop {
            segment_id: segment_id.clone(),
            text: text.clone(),
        };
    }

    let output_plan = desktop_output_plan(output_mode, origin_target, current_target);
    match (output_plan.strategy, output_plan.insert_target) {
        (DesktopOutputStrategy::HonorConfiguredOutput, Some(insert_target)) => {
            DesktopLiveStreamingLocalSegmentPlan::Insert {
                segment_id: segment_id.clone(),
                text: text.clone(),
                insert_target,
            }
        }
        _ => DesktopLiveStreamingLocalSegmentPlan::DeferToStop {
            segment_id: segment_id.clone(),
            text: text.clone(),
        },
    }
}

pub fn desktop_streaming_stop_policy(
    live_inserted_segment_count: usize,
) -> DesktopStreamingStopPolicy {
    let insert_final_transcript = live_inserted_segment_count == 0;
    DesktopStreamingStopPolicy {
        insert_final_transcript,
        allow_final_correction_job: true,
    }
}

pub fn desktop_streaming_latest_segment_allows_auto_patch(
    inserted_segment_ids: &[String],
    segment_id: &str,
) -> bool {
    !segment_id.trim().is_empty()
        && inserted_segment_ids
            .last()
            .is_some_and(|latest_segment_id| latest_segment_id == segment_id)
}

pub fn desktop_streaming_stop_tail_text(
    final_segment_id: &str,
    final_text: &str,
    inserted_anchors: &[SpeculativeInsertAnchor],
) -> Option<String> {
    let final_segment_id = final_segment_id.trim();
    let final_text = final_text.trim();
    if final_segment_id.is_empty() || final_text.is_empty() {
        return None;
    }
    if inserted_anchors.is_empty() {
        return Some(final_text.to_string());
    }

    let Some(existing_anchor) = inserted_anchors
        .iter()
        .find(|anchor| anchor.segment_id == final_segment_id)
    else {
        return Some(final_text.to_string());
    };

    let remainder = final_text
        .strip_prefix(existing_anchor.inserted_text.as_str())
        .unwrap_or("")
        .trim();
    if remainder.is_empty() {
        None
    } else {
        Some(remainder.to_string())
    }
}

pub fn desktop_speculative_replacement_selection_count(text: &str) -> usize {
    text.chars().count()
}

pub fn desktop_hud_view_model_for_phase(phase: RuntimePhase) -> DesktopHudViewModel {
    match phase {
        RuntimePhase::TriggerArmed | RuntimePhase::Recording => {
            desktop_hud_view_model_for_listening_level(0.0)
        }
        RuntimePhase::Transcribing | RuntimePhase::Processing | RuntimePhase::Inserting => {
            DesktopHudViewModel {
                visual_state: DesktopHudVisualState::Thinking,
                title: "Thinking".to_string(),
                detail: None,
                meter: None,
                progress_percent: Some(match phase {
                    RuntimePhase::Transcribing => 28,
                    RuntimePhase::Processing => 62,
                    RuntimePhase::Inserting => 88,
                    _ => 0,
                }),
            }
        }
        RuntimePhase::Completed => DesktopHudViewModel {
            visual_state: DesktopHudVisualState::Success,
            title: "Done".to_string(),
            detail: None,
            meter: None,
            progress_percent: None,
        },
        RuntimePhase::Failed => DesktopHudViewModel {
            visual_state: DesktopHudVisualState::Error,
            title: "Failed".to_string(),
            detail: None,
            meter: None,
            progress_percent: None,
        },
        RuntimePhase::Cancelled => DesktopHudViewModel {
            visual_state: DesktopHudVisualState::Cancelled,
            title: "Cancelled".to_string(),
            detail: None,
            meter: None,
            progress_percent: None,
        },
    }
}

pub fn desktop_hud_audio_meter_model(level: f32) -> DesktopHudAudioMeterModel {
    const MAX_ADDITIONAL_HEIGHTS: [i32; 9] = [3, 6, 9, 12, 14, 12, 9, 6, 3];
    let eased_level = level.clamp(0.0, 1.0).powf(0.85);
    let mut bar_heights = [4; 9];
    for (index, max_additional) in MAX_ADDITIONAL_HEIGHTS.iter().enumerate() {
        bar_heights[index] += ((*max_additional as f32) * eased_level).floor() as i32;
    }
    DesktopHudAudioMeterModel { bar_heights }
}

pub fn desktop_hud_audio_meter_model_for_waveform(
    waveform_bins: [f32; 9],
) -> DesktopHudAudioMeterModel {
    let mut bar_heights = [4; 9];
    for (index, level) in waveform_bins.iter().enumerate() {
        let eased_level = level.clamp(0.0, 1.0).powf(0.9);
        bar_heights[index] += (14.0 * eased_level).floor() as i32;
    }
    DesktopHudAudioMeterModel { bar_heights }
}

pub fn desktop_hud_thinking_progress_model(
    progress_percent: Option<u8>,
    pulse_tick: u32,
) -> DesktopHudThinkingProgressModel {
    const SOFT_ETA_TICKS: u32 = 42;
    const MIN_FILL_PERCENT: u8 = 0;
    const SOFT_ETA_FILL_PERCENT: u8 = 90;
    const MAX_FILL_PERCENT: u8 = 98;
    const PHASE_FLOOR_RAMP_TICKS: u32 = 14;
    const TEXT_WAVE_SEQUENCE: [i8; 7] = [0, 1, 3, 1, -1, -3, -1];

    let base_fill_percent = if pulse_tick <= SOFT_ETA_TICKS {
        let progress = pulse_tick as f32 / SOFT_ETA_TICKS as f32;
        let eased = 1.0 - (1.0 - progress).powf(1.6);
        let fill_span = f32::from(SOFT_ETA_FILL_PERCENT - MIN_FILL_PERCENT);
        MIN_FILL_PERCENT + (fill_span * eased).round() as u8
    } else {
        let extra_ticks = (pulse_tick - SOFT_ETA_TICKS) as f32;
        let tail_progress = 1.0 - (1.0 / (1.0 + (extra_ticks / 18.0)));
        let fill_span = f32::from(MAX_FILL_PERCENT - SOFT_ETA_FILL_PERCENT);
        SOFT_ETA_FILL_PERCENT + (fill_span * tail_progress).round() as u8
    };

    let phase_floor_target_percent = match progress_percent.unwrap_or_default() {
        88..=u8::MAX => 84,
        62..=87 => 48,
        28..=61 => 10,
        _ => MIN_FILL_PERCENT,
    };
    let phase_floor_percent = ((u32::from(phase_floor_target_percent)
        * pulse_tick.min(PHASE_FLOOR_RAMP_TICKS))
        / PHASE_FLOOR_RAMP_TICKS) as u8;
    let fill_percent = base_fill_percent
        .max(phase_floor_percent)
        .min(MAX_FILL_PERCENT);
    let text_wave_index = ((pulse_tick / 2) as usize) % TEXT_WAVE_SEQUENCE.len();

    DesktopHudThinkingProgressModel {
        fill_percent,
        text_wave_offset_px: TEXT_WAVE_SEQUENCE[text_wave_index],
    }
}

pub fn desktop_hud_thinking_text_wave_offsets(glyph_count: usize, pulse_tick: u32) -> Vec<i8> {
    const TEXT_WAVE_SEQUENCE: [i8; 8] = [0, 1, 3, 1, 0, -1, -3, -1];
    let phase = ((pulse_tick / 2) as usize) % TEXT_WAVE_SEQUENCE.len();

    (0..glyph_count)
        .map(|index| TEXT_WAVE_SEQUENCE[(phase + index) % TEXT_WAVE_SEQUENCE.len()])
        .collect()
}

pub fn desktop_hud_thinking_palette() -> DesktopHudThinkingPalette {
    DesktopHudThinkingPalette {
        track_start_rgb: [11, 14, 18],
        track_end_rgb: [20, 24, 30],
        fill_start_rgb: [163, 204, 0],
        fill_end_rgb: [217, 255, 56],
        fill_head_rgb: [239, 255, 146],
        border_rgb: [76, 92, 18],
        text_rgb: [247, 252, 230],
        text_shadow_rgb: [11, 14, 18],
    }
}

pub fn desktop_hud_metrics_for_view_model(model: &DesktopHudViewModel) -> DesktopHudMetrics {
    match model.visual_state {
        DesktopHudVisualState::Listening => desktop_listening_hud_metrics(model.detail.as_deref()),
        DesktopHudVisualState::Thinking => DesktopHudMetrics {
            width: 188,
            height: 40,
            bottom_margin: 132,
            corner_radius: 0,
        },
        DesktopHudVisualState::Success
        | DesktopHudVisualState::Error
        | DesktopHudVisualState::Cancelled
        | DesktopHudVisualState::Informational => DesktopHudMetrics {
            width: 228,
            height: 60,
            bottom_margin: 132,
            corner_radius: 0,
        },
    }
}

fn desktop_listening_hud_metrics(partial_text: Option<&str>) -> DesktopHudMetrics {
    const BASE_WIDTH: i32 = 188;
    const BASE_HEIGHT: i32 = 52;
    const EXPANDED_WIDTH: i32 = 320;
    const MAX_HEIGHT: i32 = 178;
    const COMPACT_TEXT_UNITS: usize = 14;
    const EXTRA_LINE_HEIGHT_PX: i32 = 18;

    let text_units = partial_text
        .map(desktop_display_text_units)
        .unwrap_or_default();
    let width = if text_units <= COMPACT_TEXT_UNITS {
        BASE_WIDTH
    } else {
        EXPANDED_WIDTH
    };
    let line_count = desktop_listening_hud_visible_partial_line_count(partial_text, width, 96);
    let height = if line_count <= 1 {
        BASE_HEIGHT
    } else {
        (BASE_HEIGHT + ((i32::from(line_count) - 1) * EXTRA_LINE_HEIGHT_PX)).min(MAX_HEIGHT)
    };

    DesktopHudMetrics {
        width,
        height,
        bottom_margin: 130,
        corner_radius: 0,
    }
}

fn desktop_display_text_units(text: &str) -> usize {
    text.trim()
        .chars()
        .map(|ch| if ch.is_ascii() { 1 } else { 2 })
        .sum()
}

fn desktop_listening_hud_partial_line_count(
    partial_text: Option<&str>,
    width: i32,
    dpi: u32,
) -> (u8, u8, usize) {
    const MAX_VISIBLE_LINES: u8 = 8;

    let Some(text) = partial_text.map(str::trim).filter(|text| !text.is_empty()) else {
        return (0, 0, 0);
    };
    let waveform_rect = desktop_listening_hud_waveform_rect(width, 52, dpi);
    let text_width = (waveform_rect.right - waveform_rect.left).max(1);
    let unit_width = scale_desktop_overlay_length(7, dpi).max(4);
    let units_per_line = (text_width / unit_width).max(8) as usize;
    let units = desktop_display_text_units(text);
    let raw_line_count = units.div_ceil(units_per_line).clamp(1, 64) as u8;
    let visible_line_count = raw_line_count.min(MAX_VISIBLE_LINES);
    (
        raw_line_count,
        visible_line_count,
        units_per_line * usize::from(visible_line_count),
    )
}

fn desktop_listening_hud_visible_partial_line_count(
    partial_text: Option<&str>,
    width: i32,
    dpi: u32,
) -> u8 {
    let (_, visible_line_count, _) =
        desktop_listening_hud_partial_line_count(partial_text, width, dpi);
    visible_line_count
}

pub fn desktop_overlay_scale_factor_for_dpi(dpi: u32) -> f32 {
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

pub fn scale_desktop_overlay_length(length: i32, dpi: u32) -> i32 {
    ((length as f32) * desktop_overlay_scale_factor_for_dpi(dpi)).round() as i32
}

pub fn desktop_copy_popup_model(text: &str) -> DesktopCopyPopupModel {
    let editable_text = text.trim().to_string();
    DesktopCopyPopupModel {
        title: String::new(),
        editable_text: editable_text.clone(),
        copy_label: "复制".to_string(),
        panes: vec![DesktopCopyPopupPaneModel {
            label: String::new(),
            text: editable_text,
            editable: true,
            copy_default: true,
        }],
    }
}

pub fn desktop_shortcut_help_model(bindings: &[DesktopActionBinding]) -> DesktopShortcutHelpModel {
    DesktopShortcutHelpModel {
        title: "RightAlt".to_string(),
        detail: String::new(),
        entries: bindings
            .iter()
            .map(|binding| DesktopShortcutHelpEntry {
                title: match binding.route {
                    DesktopActionRoute::Primary => "输入".to_string(),
                    DesktopActionRoute::Transcribe => "转录".to_string(),
                    DesktopActionRoute::Document => "公文".to_string(),
                    DesktopActionRoute::Command => "命令".to_string(),
                    DesktopActionRoute::Generate => "生成".to_string(),
                    DesktopActionRoute::Smart => "智能".to_string(),
                    DesktopActionRoute::Translate => "翻译".to_string(),
                    DesktopActionRoute::Ask => "提问".to_string(),
                },
                shortcut: match binding.route {
                    DesktopActionRoute::Primary => "松开".to_string(),
                    DesktopActionRoute::Transcribe
                    | DesktopActionRoute::Document
                    | DesktopActionRoute::Command
                    | DesktopActionRoute::Generate
                    | DesktopActionRoute::Smart
                    | DesktopActionRoute::Translate
                    | DesktopActionRoute::Ask => {
                        compact_shortcut_help_label(binding.shortcut.trigger_key_name())
                    }
                },
                detail: String::new(),
            })
            .collect(),
    }
}

fn compact_shortcut_help_label(trigger_key_name: &str) -> String {
    match trigger_key_name {
        "Slash" => "/".to_string(),
        other => other.to_string(),
    }
}

pub fn desktop_copy_popup_action_for_virtual_key(virtual_key: u32) -> DesktopCopyPopupAction {
    match virtual_key {
        0x0D => DesktopCopyPopupAction::CopyToClipboard,
        0x1B => DesktopCopyPopupAction::Close,
        _ => DesktopCopyPopupAction::Ignore,
    }
}

pub fn desktop_hud_activation_policy() -> DesktopOverlayActivationPolicy {
    DesktopOverlayActivationPolicy::NoActivate
}

pub fn desktop_copy_popup_activation_policy() -> DesktopOverlayActivationPolicy {
    DesktopOverlayActivationPolicy::ActivateOnInteract
}

pub fn desktop_shortcut_help_activation_policy() -> DesktopOverlayActivationPolicy {
    DesktopOverlayActivationPolicy::NoActivate
}

pub fn desktop_hud_presentation_for_phase(phase: RuntimePhase) -> DesktopHudPresentation {
    match phase {
        RuntimePhase::Completed | RuntimePhase::Cancelled => DesktopHudPresentation::Hidden,
        RuntimePhase::Failed => DesktopHudPresentation::Visible {
            auto_hide_ms: Some(1500),
        },
        _ => DesktopHudPresentation::Visible { auto_hide_ms: None },
    }
}

pub fn desktop_copy_popup_position(
    screen_width: i32,
    screen_height: i32,
    popup_width: i32,
    popup_height: i32,
    bottom_margin: i32,
) -> DesktopOverlayPosition {
    DesktopOverlayPosition {
        x: ((screen_width - popup_width).max(0)) / 2,
        y: (screen_height - popup_height - bottom_margin).max(0),
    }
}

pub fn desktop_shortcut_help_position(
    screen_width: i32,
    screen_height: i32,
    popup_width: i32,
    popup_height: i32,
    bottom_margin: i32,
) -> DesktopOverlayPosition {
    DesktopOverlayPosition {
        x: ((screen_width - popup_width).max(0)) / 2,
        y: (screen_height - popup_height - bottom_margin).max(0),
    }
}

pub fn desktop_copy_popup_metrics() -> DesktopCopyPopupMetrics {
    DesktopCopyPopupMetrics {
        width: 388,
        height: 156,
        bottom_margin: 88,
    }
}

pub fn desktop_copy_popup_copy_button_rect(
    width: i32,
    height: i32,
    dpi: u32,
) -> DesktopOverlayRect {
    let half_width = scale_desktop_overlay_length(44, dpi);
    let bottom_margin = scale_desktop_overlay_length(12, dpi);
    let button_height = scale_desktop_overlay_length(30, dpi);
    let bottom = height - bottom_margin;
    DesktopOverlayRect {
        left: (width / 2) - half_width,
        top: bottom - button_height,
        right: (width / 2) + half_width,
        bottom,
    }
}

pub fn desktop_copy_popup_close_button_rect(
    width: i32,
    _height: i32,
    dpi: u32,
) -> DesktopOverlayRect {
    DesktopOverlayRect {
        left: width - scale_desktop_overlay_length(48, dpi),
        top: scale_desktop_overlay_length(8, dpi),
        right: width - scale_desktop_overlay_length(12, dpi),
        bottom: scale_desktop_overlay_length(40, dpi),
    }
}

pub fn desktop_copy_popup_editor_frame_rect(
    width: i32,
    height: i32,
    dpi: u32,
) -> DesktopOverlayRect {
    DesktopOverlayRect {
        left: scale_desktop_overlay_length(20, dpi),
        top: scale_desktop_overlay_length(46, dpi),
        right: width - scale_desktop_overlay_length(20, dpi),
        bottom: height - scale_desktop_overlay_length(48, dpi),
    }
}

pub fn desktop_copy_popup_editor_content_rect(
    width: i32,
    height: i32,
    dpi: u32,
    content_height: i32,
) -> DesktopOverlayRect {
    let frame = desktop_copy_popup_editor_frame_rect(width, height, dpi);
    let horizontal_inset = scale_desktop_overlay_length(10, dpi);
    let vertical_margin = scale_desktop_overlay_length(8, dpi);
    let min_content_height = scale_desktop_overlay_length(24, dpi);
    let comfort_padding = scale_desktop_overlay_length(4, dpi);
    let frame_height = (frame.bottom - frame.top).max(min_content_height);
    let max_content_height = (frame_height - (vertical_margin * 2)).max(min_content_height);
    let desired_height = (content_height + comfort_padding)
        .max(min_content_height)
        .min(max_content_height);
    let top = frame.top + ((frame_height - desired_height).max(0) / 2);

    DesktopOverlayRect {
        left: frame.left + horizontal_inset,
        top,
        right: frame.right - horizontal_inset,
        bottom: top + desired_height,
    }
}

pub fn desktop_copy_popup_pane_layouts(
    width: i32,
    height: i32,
    dpi: u32,
    content_heights: &[i32],
) -> Vec<DesktopCopyPopupPaneLayout> {
    if content_heights.is_empty() {
        return Vec::new();
    }

    if content_heights.len() == 1 {
        let editor_rect =
            desktop_copy_popup_editor_content_rect(width, height, dpi, content_heights[0]);
        return vec![DesktopCopyPopupPaneLayout {
            label_rect: DesktopOverlayRect {
                left: editor_rect.left,
                top: editor_rect.top,
                right: editor_rect.right,
                bottom: editor_rect.top,
            },
            editor_rect,
        }];
    }

    let frame = desktop_copy_popup_editor_frame_rect(width, height, dpi);
    let horizontal_inset = scale_desktop_overlay_length(10, dpi);
    let vertical_margin = scale_desktop_overlay_length(8, dpi);
    let pane_gap = scale_desktop_overlay_length(8, dpi);
    let label_height = scale_desktop_overlay_length(14, dpi);
    let label_edit_gap = scale_desktop_overlay_length(3, dpi);
    let min_editor_height = scale_desktop_overlay_length(28, dpi);
    let pane_count = content_heights.len() as i32;
    let usable_top = frame.top + vertical_margin;
    let usable_bottom = frame.bottom - vertical_margin;
    let usable_height = (usable_bottom - usable_top).max(1);
    let total_gap = pane_gap * (pane_count - 1).max(0);
    let slot_height = ((usable_height - total_gap).max(pane_count) / pane_count)
        .max(label_height + label_edit_gap + min_editor_height);
    let max_right = frame.right - horizontal_inset;
    let min_left = frame.left + horizontal_inset;

    (0..content_heights.len())
        .map(|index| {
            let slot_top = usable_top + (index as i32 * (slot_height + pane_gap));
            let slot_bottom = if index + 1 == content_heights.len() {
                usable_bottom
            } else {
                (slot_top + slot_height).min(usable_bottom)
            };
            let label_bottom = (slot_top + label_height).min(slot_bottom);
            let editor_top = (label_bottom + label_edit_gap).min(slot_bottom);
            let editor_bottom = slot_bottom.max(editor_top);
            DesktopCopyPopupPaneLayout {
                label_rect: DesktopOverlayRect {
                    left: min_left,
                    top: slot_top,
                    right: max_right,
                    bottom: label_bottom,
                },
                editor_rect: DesktopOverlayRect {
                    left: min_left,
                    top: editor_top,
                    right: max_right,
                    bottom: editor_bottom,
                },
            }
        })
        .collect()
}

pub fn desktop_listening_hud_cancel_button_rect(
    _width: i32,
    height: i32,
    dpi: u32,
) -> DesktopOverlayRect {
    let button_size = scale_desktop_overlay_length(28, dpi).max(20);
    let gutter = scale_desktop_overlay_length(12, dpi).max(8);
    let top = ((height - button_size).max(0)) / 2;
    DesktopOverlayRect {
        left: gutter,
        top,
        right: gutter + button_size,
        bottom: top + button_size,
    }
}

pub fn desktop_listening_hud_complete_button_rect(
    width: i32,
    height: i32,
    dpi: u32,
) -> DesktopOverlayRect {
    let button_size = scale_desktop_overlay_length(28, dpi).max(20);
    let gutter = scale_desktop_overlay_length(12, dpi).max(8);
    let top = ((height - button_size).max(0)) / 2;
    DesktopOverlayRect {
        left: width - gutter - button_size,
        top,
        right: width - gutter,
        bottom: top + button_size,
    }
}

pub fn desktop_listening_hud_waveform_rect(
    width: i32,
    height: i32,
    dpi: u32,
) -> DesktopOverlayRect {
    let cancel = desktop_listening_hud_cancel_button_rect(width, height, dpi);
    let complete = desktop_listening_hud_complete_button_rect(width, height, dpi);
    let gap = scale_desktop_overlay_length(10, dpi).max(6);
    DesktopOverlayRect {
        left: cancel.right + gap,
        top: scale_desktop_overlay_length(10, dpi).max(6),
        right: complete.left - gap,
        bottom: height - scale_desktop_overlay_length(10, dpi).max(6),
    }
}

pub fn desktop_listening_hud_partial_text_layout(
    width: i32,
    height: i32,
    dpi: u32,
    partial_text: Option<&str>,
) -> Option<DesktopListeningHudPartialTextLayout> {
    let (raw_line_count, line_count, visible_text_units) =
        desktop_listening_hud_partial_line_count(partial_text, width, dpi);
    if line_count == 0 {
        return None;
    }

    let mut waveform_rect = desktop_listening_hud_waveform_rect(width, height, dpi);
    let scrolls_text = raw_line_count > line_count;
    if line_count == 1 {
        let text_rect = DesktopOverlayRect {
            left: waveform_rect.left,
            top: scale_desktop_overlay_length(4, dpi),
            right: waveform_rect.right,
            bottom: scale_desktop_overlay_length(20, dpi),
        };
        waveform_rect.top = (waveform_rect.top + scale_desktop_overlay_length(13, dpi))
            .min(waveform_rect.bottom - scale_desktop_overlay_length(4, dpi).max(2));
        return Some(DesktopListeningHudPartialTextLayout {
            text_rect,
            waveform_rect,
            line_count,
            wraps_text: false,
            scrolls_text: false,
            scrollbar_rect: None,
            visible_text_units,
        });
    }

    let text_top = scale_desktop_overlay_length(8, dpi);
    let line_height = scale_desktop_overlay_length(17, dpi).max(12);
    let text_bottom = text_top + (i32::from(line_count) * line_height);
    let scrollbar_width = scale_desktop_overlay_length(3, dpi).max(2);
    let scrollbar_gap = scale_desktop_overlay_length(5, dpi).max(3);
    let scrollbar_rect = scrolls_text.then_some(DesktopOverlayRect {
        left: waveform_rect.right - scrollbar_width,
        top: text_top,
        right: waveform_rect.right,
        bottom: text_bottom,
    });
    let text_rect = DesktopOverlayRect {
        left: waveform_rect.left,
        top: text_top,
        right: if scrolls_text {
            waveform_rect.right - scrollbar_width - scrollbar_gap
        } else {
            waveform_rect.right
        },
        bottom: text_bottom,
    };
    waveform_rect.top = (text_bottom + scale_desktop_overlay_length(4, dpi))
        .min(waveform_rect.bottom - scale_desktop_overlay_length(4, dpi).max(2));

    Some(DesktopListeningHudPartialTextLayout {
        text_rect,
        waveform_rect,
        line_count,
        wraps_text: true,
        scrolls_text,
        scrollbar_rect,
        visible_text_units,
    })
}

pub fn desktop_listening_hud_visible_partial_text(
    partial_text: &str,
    layout: &DesktopListeningHudPartialTextLayout,
) -> String {
    let text = partial_text.trim();
    if !layout.scrolls_text {
        return text.to_string();
    }

    let mut collected = Vec::new();
    let mut used_units = 0usize;
    for ch in text.chars().rev() {
        let units = if ch.is_ascii() { 1 } else { 2 };
        if used_units + units > layout.visible_text_units {
            break;
        }
        used_units += units;
        collected.push(ch);
    }
    collected.iter().rev().collect()
}

pub fn desktop_listening_hud_action_for_point(
    width: i32,
    height: i32,
    dpi: u32,
    x: i32,
    y: i32,
) -> DesktopListeningHudAction {
    if desktop_listening_hud_cancel_button_rect(width, height, dpi).contains(x, y) {
        return DesktopListeningHudAction::Cancel;
    }
    if desktop_listening_hud_complete_button_rect(width, height, dpi).contains(x, y) {
        return DesktopListeningHudAction::Complete;
    }
    DesktopListeningHudAction::Ignore
}

pub fn desktop_shortcut_help_metrics() -> DesktopShortcutHelpMetrics {
    DesktopShortcutHelpMetrics {
        width: 420,
        height: 184,
        bottom_margin: 36,
    }
}

pub fn desktop_copy_popup_copy_shows_follow_up_hud() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeculativeInsertAnchor {
    pub window_handle: isize,
    pub focus_handle: Option<isize>,
    pub segment_id: String,
    pub inserted_text: String,
    pub inserted_at_ms: u64,
}

impl SpeculativeInsertAnchor {
    pub fn new(
        window_handle: isize,
        focus_handle: Option<isize>,
        segment_id: impl Into<String>,
        inserted_text: impl Into<String>,
        inserted_at_ms: u64,
    ) -> Result<Self, String> {
        let segment_id = segment_id.into();
        let inserted_text = inserted_text.into();
        if window_handle == 0 {
            return Err("window handle must not be zero".to_string());
        }
        if segment_id.trim().is_empty() {
            return Err("segment id must not be blank".to_string());
        }
        if inserted_text.trim().is_empty() {
            return Err("inserted text must not be blank".to_string());
        }
        Ok(Self {
            window_handle,
            focus_handle,
            segment_id,
            inserted_text,
            inserted_at_ms,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeculativePatchCandidate {
    pub current_window_handle: isize,
    pub current_focus_handle: Option<isize>,
    pub segment_id: String,
    pub corrected_text: String,
    pub received_at_ms: u64,
}

impl SpeculativePatchCandidate {
    pub fn new(
        current_window_handle: isize,
        current_focus_handle: Option<isize>,
        segment_id: impl Into<String>,
        corrected_text: impl Into<String>,
        received_at_ms: u64,
    ) -> Result<Self, String> {
        let segment_id = segment_id.into();
        let corrected_text = corrected_text.into();
        if current_window_handle == 0 {
            return Err("current window handle must not be zero".to_string());
        }
        if segment_id.trim().is_empty() {
            return Err("segment id must not be blank".to_string());
        }
        if corrected_text.trim().is_empty() {
            return Err("corrected text must not be blank".to_string());
        }
        Ok(Self {
            current_window_handle,
            current_focus_handle,
            segment_id,
            corrected_text,
            received_at_ms,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeculativePatchApplication {
    Apply,
    KeepLocalText,
    DeferToPopup,
}

pub fn decide_speculative_patch_application(
    anchor: &SpeculativeInsertAnchor,
    candidate: &SpeculativePatchCandidate,
    max_age_ms: u64,
    max_edit_ratio: f32,
) -> SpeculativePatchApplication {
    if anchor.segment_id != candidate.segment_id {
        return SpeculativePatchApplication::DeferToPopup;
    }
    if anchor.window_handle != candidate.current_window_handle {
        return SpeculativePatchApplication::DeferToPopup;
    }
    if anchor.focus_handle != candidate.current_focus_handle {
        return SpeculativePatchApplication::DeferToPopup;
    }
    if candidate
        .received_at_ms
        .saturating_sub(anchor.inserted_at_ms)
        > max_age_ms
    {
        return SpeculativePatchApplication::DeferToPopup;
    }
    if anchor.inserted_text == candidate.corrected_text {
        return SpeculativePatchApplication::KeepLocalText;
    }
    if should_auto_apply_corrected_text(
        &anchor.inserted_text,
        &candidate.corrected_text,
        max_edit_ratio,
    ) {
        SpeculativePatchApplication::Apply
    } else {
        SpeculativePatchApplication::DeferToPopup
    }
}

pub fn decide_desktop_output_strategy(
    output_mode: OutputMode,
    target: Option<&DesktopInsertTargetContext>,
) -> DesktopOutputStrategy {
    match output_mode {
        OutputMode::DryRun => DesktopOutputStrategy::HonorConfiguredOutput,
        OutputMode::ClipboardPaste => {
            if desktop_insert_target_looks_editable(target) {
                DesktopOutputStrategy::HonorConfiguredOutput
            } else {
                DesktopOutputStrategy::ShowCopyPopupOnly
            }
        }
    }
}

pub fn desktop_output_plan(
    output_mode: OutputMode,
    origin_target: Option<&DesktopInsertTargetContext>,
    current_target: Option<&DesktopInsertTargetContext>,
) -> DesktopOutputPlan {
    let insert_target = match output_mode {
        OutputMode::DryRun => origin_target.and_then(|target| target.target),
        OutputMode::ClipboardPaste => {
            desktop_output_insert_target_for_clipboard_paste(origin_target, current_target)
        }
    };
    let strategy = if insert_target.is_some() {
        DesktopOutputStrategy::HonorConfiguredOutput
    } else {
        DesktopOutputStrategy::ShowCopyPopupOnly
    };

    DesktopOutputPlan {
        strategy,
        insert_target,
    }
}

pub fn resolve_hotkey_origin_insert_target(
    pending_target: Option<&DesktopInsertTargetContext>,
    fallback_target: Option<&DesktopInsertTargetContext>,
) -> Option<DesktopInsertTargetContext> {
    pending_target.cloned().or_else(|| fallback_target.cloned())
}

pub fn resolve_pending_hotkey_origin_capture(
    existing_target: Option<&DesktopInsertTargetContext>,
    candidate_target: Option<&DesktopInsertTargetContext>,
) -> Option<DesktopInsertTargetContext> {
    match (existing_target, candidate_target) {
        (Some(existing_target), Some(candidate_target)) => {
            let existing_score = desktop_insert_target_capture_quality(existing_target);
            let candidate_score = desktop_insert_target_capture_quality(candidate_target);
            if candidate_score >= existing_score {
                Some(candidate_target.clone())
            } else {
                Some(existing_target.clone())
            }
        }
        (Some(existing_target), None) => Some(existing_target.clone()),
        (None, Some(candidate_target)) => Some(candidate_target.clone()),
        (None, None) => None,
    }
}

pub fn resolve_hotkey_recording_origin_enrichment(
    existing_target: Option<&DesktopInsertTargetContext>,
    candidate_target: Option<&DesktopInsertTargetContext>,
) -> Option<DesktopInsertTargetContext> {
    match (existing_target, candidate_target) {
        (Some(existing_target), Some(candidate_target)) => {
            let existing_window_handle = existing_target.target.map(|target| target.window_handle);
            let candidate_window_handle =
                candidate_target.target.map(|target| target.window_handle);

            if existing_window_handle.is_some() && candidate_window_handle != existing_window_handle
            {
                return Some(existing_target.clone());
            }

            let existing_score = desktop_insert_target_capture_quality(existing_target);
            let candidate_score = desktop_insert_target_capture_quality(candidate_target);
            if candidate_score > existing_score {
                Some(candidate_target.clone())
            } else {
                Some(existing_target.clone())
            }
        }
        (Some(existing_target), None) => Some(existing_target.clone()),
        (None, Some(_candidate_target)) => None,
        (None, None) => None,
    }
}

fn desktop_output_insert_target_for_clipboard_paste(
    origin_target: Option<&DesktopInsertTargetContext>,
    current_target: Option<&DesktopInsertTargetContext>,
) -> Option<ForegroundInsertTarget> {
    let origin_target = origin_target?;
    let origin_window_handle = origin_target.target?.window_handle;
    let current_target = current_target?;
    let current_foreground_target = current_target.target?;
    if current_foreground_target.window_handle != origin_window_handle
        || desktop_insert_target_is_explicitly_noneditable(Some(current_target))
    {
        return None;
    }

    if desktop_insert_target_looks_editable(Some(current_target))
        && desktop_same_insert_control(origin_target, current_target)
    {
        return Some(current_foreground_target);
    }

    None
}

fn desktop_same_insert_control(
    origin_target: &DesktopInsertTargetContext,
    current_target: &DesktopInsertTargetContext,
) -> bool {
    desktop_same_insert_control_via_handles(origin_target, current_target)
        || desktop_same_insert_control_via_runtime_id(origin_target, current_target)
}

fn desktop_same_insert_control_via_handles(
    origin_target: &DesktopInsertTargetContext,
    current_target: &DesktopInsertTargetContext,
) -> bool {
    let origin_handles = desktop_insert_target_identity_handles(origin_target);
    let current_handles = desktop_insert_target_identity_handles(current_target);

    !origin_handles.is_empty()
        && !current_handles.is_empty()
        && origin_handles
            .iter()
            .any(|origin_handle| current_handles.contains(origin_handle))
}

fn desktop_insert_target_identity_handles(target: &DesktopInsertTargetContext) -> Vec<isize> {
    let mut handles = Vec::new();

    if let Some(focus_handle) = target.target.and_then(|target| target.focus_handle) {
        handles.push(focus_handle);
    }
    if let Some(caret_window_handle) = target.caret_window_handle {
        if !handles.contains(&caret_window_handle) {
            handles.push(caret_window_handle);
        }
    }

    handles
}

fn desktop_insert_target_capture_quality(target: &DesktopInsertTargetContext) -> u32 {
    let mut score = 0;

    if target.target.is_some() {
        score += 1;
    }
    if desktop_insert_target_looks_editable(Some(target)) {
        score += 8;
    }
    if target.automation_runtime_id.is_some() {
        score += 6;
    }
    if !desktop_insert_target_identity_handles(target).is_empty() {
        score += 4;
    }
    if target.automation_control_type.is_some() {
        score += 2;
    }
    if target.automation_framework_id.is_some() {
        score += 1;
    }
    if target.focus_class_name.is_some() {
        score += 1;
    }

    score
}

fn desktop_same_insert_control_via_runtime_id(
    origin_target: &DesktopInsertTargetContext,
    current_target: &DesktopInsertTargetContext,
) -> bool {
    match (
        origin_target.automation_runtime_id.as_ref(),
        current_target.automation_runtime_id.as_ref(),
    ) {
        (Some(origin_runtime_id), Some(current_runtime_id)) => {
            !origin_runtime_id.is_empty() && origin_runtime_id == current_runtime_id
        }
        _ => false,
    }
}

pub fn desktop_insert_target_restore_requested(
    target: ForegroundInsertTarget,
    current_target: Option<&DesktopInsertTargetContext>,
) -> bool {
    let Some(current_target) = current_target.and_then(|target| target.target) else {
        return target.focus_handle.is_some();
    };

    if current_target.window_handle != target.window_handle {
        return true;
    }

    match target.focus_handle {
        Some(target_focus_handle) => current_target.focus_handle != Some(target_focus_handle),
        None => false,
    }
}

fn desktop_insert_target_looks_editable(target: Option<&DesktopInsertTargetContext>) -> bool {
    let Some(target) = target else {
        return false;
    };
    if target.target.is_none() {
        return false;
    }
    if target.caret_window_handle.is_some() {
        return true;
    }

    target
        .focus_class_name
        .as_deref()
        .map(focus_class_name_has_editable_hint)
        .unwrap_or(false)
        || automation_editability_hints(target)
}

fn desktop_insert_target_is_explicitly_noneditable(
    target: Option<&DesktopInsertTargetContext>,
) -> bool {
    let Some(target) = target else {
        return false;
    };
    if target.target.is_none() {
        return false;
    }
    if target.caret_window_handle.is_some() {
        return false;
    }
    if target.automation_is_keyboard_focusable == Some(false) {
        return true;
    }
    if target
        .focus_class_name
        .as_deref()
        .map(focus_class_name_has_editable_hint)
        .unwrap_or(false)
    {
        return false;
    }
    if target
        .focus_class_name
        .as_deref()
        .map(focus_class_name_has_noneditable_hint)
        .unwrap_or(false)
    {
        return true;
    }
    if target.automation_supports_text_pattern || target.automation_supports_value_pattern {
        return false;
    }
    if target
        .automation_control_type
        .as_deref()
        .map(automation_control_type_has_editable_hint)
        .unwrap_or(false)
        && target.automation_is_keyboard_focusable == Some(true)
    {
        return false;
    }

    target
        .automation_control_type
        .as_deref()
        .map(automation_control_type_has_noneditable_hint)
        .unwrap_or(false)
}

fn focus_class_name_has_editable_hint(class_name: &str) -> bool {
    let normalized = class_name.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    normalized == "edit"
        || normalized == "scintilla"
        || normalized.starts_with("richedit")
        || normalized.starts_with("windowsforms10.edit")
}

fn focus_class_name_has_noneditable_hint(class_name: &str) -> bool {
    let normalized = class_name.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    normalized == "button" || normalized == "static"
}

fn automation_editability_hints(target: &DesktopInsertTargetContext) -> bool {
    if target.automation_is_keyboard_focusable == Some(false) {
        return false;
    }

    if target.automation_supports_text_pattern || target.automation_supports_value_pattern {
        return true;
    }

    target
        .automation_control_type
        .as_deref()
        .map(automation_control_type_has_editable_hint)
        .unwrap_or(false)
        && target.automation_is_keyboard_focusable == Some(true)
}

fn automation_control_type_has_editable_hint(control_type: &str) -> bool {
    let normalized = control_type.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    normalized == "edit" || normalized == "document" || normalized == "combobox"
}

fn automation_control_type_has_noneditable_hint(control_type: &str) -> bool {
    let normalized = control_type.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    matches!(
        normalized.as_str(),
        "button"
            | "checkbox"
            | "hyperlink"
            | "image"
            | "listitem"
            | "menuitem"
            | "radiobutton"
            | "splitbutton"
            | "tabitem"
            | "treeitem"
    )
}

pub fn tray_menu_model(
    state: &ShellState,
    config: &ConfigAvailability,
    hotkey: &HotkeyBindingState,
    native_readiness: Option<&NativeReadinessSnapshot>,
) -> TrayMenuModel {
    let hotkey_label = if !config.is_ready() {
        "Config unavailable".to_string()
    } else {
        match hotkey {
            HotkeyBindingState::Active { shortcut_label, .. } => {
                format!("Hotkey: {shortcut_label}")
            }
            HotkeyBindingState::RegistrationFailed { shortcut_label, .. } => {
                format!("Hotkey unavailable: {shortcut_label}")
            }
            HotkeyBindingState::InvalidConfig { .. } => "Hotkey config invalid".to_string(),
            HotkeyBindingState::Unconfigured => "Hotkey not configured".to_string(),
        }
    };
    let detail_label = idle_status_detail(config, hotkey, native_readiness);

    TrayMenuModel {
        hotkey_label,
        detail_label,
        start_enabled: config.is_ready() && state.can_start_session(),
        stop_enabled: config.is_ready() && state.can_stop_session(),
        cancel_enabled: config.is_ready() && state.can_stop_session(),
        reload_config_enabled: true,
        open_config_enabled: true,
    }
}

pub fn config_status_message(config: &ConfigAvailability) -> Option<&'static str> {
    match config {
        ConfigAvailability::Ready => None,
        ConfigAvailability::Unavailable { .. } => Some("Talk: config unavailable"),
    }
}

pub fn hotkey_status_message(hotkey: &HotkeyBindingState) -> Option<&'static str> {
    match hotkey {
        HotkeyBindingState::Active { .. } => None,
        HotkeyBindingState::RegistrationFailed { .. } => Some("Talk: hotkey unavailable"),
        HotkeyBindingState::InvalidConfig { .. } => Some("Talk: hotkey config invalid"),
        HotkeyBindingState::Unconfigured => Some("Talk: hotkey not configured"),
    }
}

pub fn native_status_message(
    native_readiness: Option<&NativeReadinessSnapshot>,
) -> Option<&'static str> {
    native_readiness
        .filter(|snapshot| snapshot.has_unavailable_configured_native_backend())
        .map(|_| "Talk: native unavailable")
}

pub fn idle_status_detail(
    config: &ConfigAvailability,
    hotkey: &HotkeyBindingState,
    native_readiness: Option<&NativeReadinessSnapshot>,
) -> Option<String> {
    if let Some(reason) = config.reason() {
        return Some(reason.to_string());
    }
    if let Some(reason) = hotkey.reason() {
        return Some(reason.to_string());
    }
    native_readiness.and_then(NativeReadinessSnapshot::first_unavailable_detail)
}

pub fn compose_hud_message(summary: &str, detail: Option<&str>) -> String {
    match detail.map(str::trim).filter(|detail| !detail.is_empty()) {
        Some(detail) => format!("{summary}\n{detail}"),
        None => summary.to_string(),
    }
}

pub fn build_status_report(snapshot: &StatusSnapshot) -> String {
    let mut lines = vec![
        format!("Current: {}", snapshot.current_summary),
        format!("Config: {}", snapshot.config_path),
        format!("Logs: {}", snapshot.logs_dir),
        format!("Hotkey: {}", snapshot.hotkey_label),
    ];

    if let Some(detail) = snapshot.current_detail.as_deref() {
        lines.push(format!("Current detail: {detail}"));
    }
    if let Some(detail) = snapshot.hotkey_detail.as_deref() {
        lines.push(format!("Hotkey detail: {detail}"));
    }
    if let Some(last_session) = snapshot.last_session.as_ref() {
        lines.push(format!("Last session: {}", last_session.summary));
        if let Some(detail) = last_session.detail.as_deref() {
            lines.push(format!("Last session detail: {detail}"));
        }
    }
    if let Some(native_readiness) = snapshot.native_readiness.as_ref() {
        append_native_backend_report(&mut lines, "Audio", &native_readiness.audio);
        append_native_backend_report(&mut lines, "Clipboard", &native_readiness.clipboard);
    }

    lines.join("\n")
}

impl NativeReadinessSnapshot {
    pub fn has_unavailable_configured_native_backend(&self) -> bool {
        self.audio.is_configured_native_unavailable()
            || self.clipboard.is_configured_native_unavailable()
    }

    pub fn first_unavailable_detail(&self) -> Option<String> {
        self.audio
            .unavailable_detail()
            .or_else(|| self.clipboard.unavailable_detail())
    }
}

impl NativeBackendSnapshot {
    fn is_configured_native_backend(&self) -> bool {
        self.configured_backend == "native_windows"
    }

    fn is_configured_native_unavailable(&self) -> bool {
        self.is_configured_native_backend()
            && self.status == Some(NativeReadinessStatus::Unavailable)
    }

    fn unavailable_detail(&self) -> Option<String> {
        self.is_configured_native_unavailable()
            .then(|| self.detail.clone())
            .flatten()
    }
}

fn append_native_backend_report(
    lines: &mut Vec<String>,
    label: &str,
    backend: &NativeBackendSnapshot,
) {
    lines.push(format!("{label} backend: {}", backend.configured_backend));
    if let Some(status) = backend.status {
        lines.push(format!("{label} backend readiness: {}", status.as_str()));
    }
    if let Some(detail) = backend.detail.as_deref() {
        lines.push(format!("{label} backend detail: {detail}"));
    }
}

pub fn hud_message_for_phase(phase: RuntimePhase) -> &'static str {
    match phase {
        RuntimePhase::TriggerArmed | RuntimePhase::Recording => "Talk: listening",
        RuntimePhase::Transcribing => "Talk: transcribing",
        RuntimePhase::Processing => "Talk: polishing",
        RuntimePhase::Inserting => "Talk: inserting",
        RuntimePhase::Completed => "Talk: done",
        RuntimePhase::Failed => "Talk: failed",
        RuntimePhase::Cancelled => "Talk: cancelled",
    }
}

pub fn parse_hotkey(raw: &str) -> Result<HotkeySpec, String> {
    if raw.trim().is_empty() {
        return Err("shortcut must not be blank".to_string());
    }
    if raw.trim() != raw {
        return Err("shortcut must not have leading or trailing whitespace".to_string());
    }

    let tokens = raw.split('+').map(str::trim).collect::<Vec<_>>();
    let mut ctrl = ModifierRequirement::None;
    let mut alt = ModifierRequirement::None;
    let mut shift = ModifierRequirement::None;
    let mut win = ModifierRequirement::None;
    let mut trigger = None::<(String, u32)>;

    for token in tokens.iter().copied() {
        if token.is_empty() {
            return Err("shortcut contains an empty segment".to_string());
        }
        if let Some((modifier_name, requirement)) = parse_modifier_token(token, tokens.len() == 1) {
            match modifier_name {
                ModifierName::Ctrl => ctrl = requirement,
                ModifierName::Alt => alt = requirement,
                ModifierName::Shift => shift = requirement,
                ModifierName::Win => win = requirement,
            }
            continue;
        }
        if trigger.is_some() {
            return Err("shortcut must contain exactly one non-modifier key".to_string());
        }
        trigger = Some(parse_trigger_key(token)?);
    }

    let Some((key_name, virtual_key)) = trigger else {
        return Err("shortcut must contain a non-modifier key".to_string());
    };

    Ok(HotkeySpec {
        key_name,
        virtual_key,
        ctrl,
        alt,
        shift,
        win,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModifierName {
    Ctrl,
    Alt,
    Shift,
    Win,
}

fn parse_modifier_token(
    token: &str,
    allow_side_specific_trigger: bool,
) -> Option<(ModifierName, ModifierRequirement)> {
    let lower = token.to_ascii_lowercase();
    match lower.as_str() {
        "ctrl" | "control" => Some((ModifierName::Ctrl, ModifierRequirement::Either)),
        "alt" => Some((ModifierName::Alt, ModifierRequirement::Either)),
        "shift" => Some((ModifierName::Shift, ModifierRequirement::Either)),
        "win" | "meta" | "super" => Some((ModifierName::Win, ModifierRequirement::Either)),
        "leftctrl" | "left ctrl" | "leftcontrol" | "left control" | "lctrl" => {
            Some((ModifierName::Ctrl, ModifierRequirement::Left))
        }
        "rightctrl" | "right ctrl" | "rightcontrol" | "right control" | "rctrl" => {
            Some((ModifierName::Ctrl, ModifierRequirement::Right))
        }
        "leftalt" | "left alt" | "lalt" => Some((ModifierName::Alt, ModifierRequirement::Left)),
        "rightalt" | "right alt" | "ralt" if !allow_side_specific_trigger => {
            Some((ModifierName::Alt, ModifierRequirement::Right))
        }
        "leftshift" | "left shift" | "lshift" => {
            Some((ModifierName::Shift, ModifierRequirement::Left))
        }
        "rightshift" | "right shift" | "rshift" => {
            Some((ModifierName::Shift, ModifierRequirement::Right))
        }
        "leftwin" | "left win" | "lwin" => Some((ModifierName::Win, ModifierRequirement::Left)),
        "rightwin" | "right win" | "rwin" => Some((ModifierName::Win, ModifierRequirement::Right)),
        _ => None,
    }
}

pub fn resolve_desktop_audio_file_override(
    raw: Option<&str>,
    config_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Err(format!(
            "{TALK_DESKTOP_AUDIO_FILE_OVERRIDE_ENV} must not be blank"
        ));
    }
    if raw.trim() != raw {
        return Err(format!(
            "{TALK_DESKTOP_AUDIO_FILE_OVERRIDE_ENV} must not have leading or trailing whitespace"
        ));
    }

    let candidate = PathBuf::from(raw);
    let resolved = if candidate.is_absolute() {
        candidate
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(candidate)
    };

    if !resolved.exists() {
        return Err(format!(
            "Talk desktop audio override file does not exist: {}",
            resolved.display()
        ));
    }
    if !resolved.is_file() {
        return Err(format!(
            "Talk desktop audio override path is not a file: {}",
            resolved.display()
        ));
    }

    Ok(Some(resolved))
}

pub fn resolve_default_desktop_config_path(
    explicit_config_path: Option<&Path>,
    current_dir: &Path,
    executable_path: &Path,
) -> PathBuf {
    if let Some(explicit_config_path) = explicit_config_path {
        return explicit_config_path.to_path_buf();
    }

    let release_default_path = executable_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(TALK_DESKTOP_DEFAULT_CONFIG_FILE_NAME);
    if release_default_path.is_file() {
        return release_default_path;
    }

    let repo_example_path = current_dir.join("examples").join("dev-config.toml");
    if repo_example_path.is_file() {
        return repo_example_path;
    }

    release_default_path
}

pub fn desktop_packaged_local_asr_daemon_path(desktop_executable_path: &Path) -> PathBuf {
    desktop_executable_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".internal")
        .join(TALK_PACKAGED_LOCAL_ASR_DAEMON_EXE_NAME)
}

pub fn desktop_packaged_local_asr_daemon_launch_plan(
    desktop_executable_path: &Path,
    endpoint: &str,
) -> Result<Option<DesktopLocalAsrDaemonLaunchPlan>, String> {
    desktop_packaged_local_asr_daemon_launch_plan_with_config(
        desktop_executable_path,
        endpoint,
        None,
    )
}

pub fn desktop_packaged_local_asr_daemon_launch_plan_with_config(
    desktop_executable_path: &Path,
    endpoint: &str,
    local_daemon: Option<&SpeculativeLocalAsrDaemonConfig>,
) -> Result<Option<DesktopLocalAsrDaemonLaunchPlan>, String> {
    let executable_path = desktop_packaged_local_asr_daemon_path(desktop_executable_path);
    let model_root = desktop_executable_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".runtime")
        .join("models")
        .join("sherpa-onnx");
    desktop_product_local_asr_daemon_launch_plan_with_config(
        &executable_path,
        &model_root,
        endpoint,
        local_daemon,
    )
}

pub fn desktop_product_local_asr_daemon_launch_plan_with_config(
    worker_executable_path: &Path,
    model_root: &Path,
    endpoint: &str,
    local_daemon: Option<&SpeculativeLocalAsrDaemonConfig>,
) -> Result<Option<DesktopLocalAsrDaemonLaunchPlan>, String> {
    let executable_path = worker_executable_path.to_path_buf();
    if !executable_path.is_file() {
        return Ok(None);
    }

    let Some(bind) = desktop_local_asr_daemon_bind_from_endpoint(endpoint)? else {
        return Ok(None);
    };
    let mut args = vec!["--bind".to_string(), bind.clone()];
    let auto_local_daemon;
    let local_daemon = if let Some(local_daemon) = local_daemon {
        Some(local_daemon)
    } else {
        auto_local_daemon = desktop_auto_local_asr_daemon_config(model_root);
        auto_local_daemon.as_ref()
    };
    if let Some(local_daemon) = local_daemon {
        append_desktop_local_asr_daemon_args(&mut args, local_daemon);
    }
    Ok(Some(DesktopLocalAsrDaemonLaunchPlan {
        executable_path,
        bind: bind.clone(),
        args,
    }))
}

fn desktop_auto_local_asr_daemon_config(
    model_root: &Path,
) -> Option<SpeculativeLocalAsrDaemonConfig> {
    desktop_installed_zipformer_daemon_config(&model_root)
        .or_else(|| desktop_installed_paraformer_daemon_config(&model_root))
}

pub fn desktop_effective_streaming_asr_enabled(
    configured_route: DesktopSpeculativeLocalAsrRoute,
    local_asr_ready: bool,
) -> bool {
    configured_route == DesktopSpeculativeLocalAsrRoute::StreamingService && local_asr_ready
}

fn desktop_installed_zipformer_daemon_config(
    model_root: &Path,
) -> Option<SpeculativeLocalAsrDaemonConfig> {
    let model_id = "zipformer-zh-en-punct-int8-480ms";
    let model_dir = model_root.join(model_id);
    let tokens = model_dir.join("tokens.txt");
    let encoder = model_dir.join("encoder.int8.onnx");
    let decoder = model_dir.join("decoder.onnx");
    let joiner = model_dir.join("joiner.int8.onnx");
    if !tokens.is_file() || !encoder.is_file() || !decoder.is_file() || !joiner.is_file() {
        return None;
    }

    Some(SpeculativeLocalAsrDaemonConfig {
        mode: SpeculativeLocalAsrDaemonMode::SherpaOnline,
        engine: None,
        model: Some(model_id.to_string()),
        dry_run_text: None,
        dry_run_partial_text: None,
        model_family: SpeculativeSherpaOnlineModelFamily::Transducer,
        tokens: Some(tokens),
        encoder: Some(encoder),
        decoder: Some(decoder),
        joiner: Some(joiner),
        provider: Some("cpu".to_string()),
        num_threads: Some(2),
        sample_rate_hz: Some(16_000),
        decoding_method: Some("greedy_search".to_string()),
        hotwords_file: None,
        rule_fsts: None,
        rule_fars: None,
    })
}

fn desktop_installed_paraformer_daemon_config(
    model_root: &Path,
) -> Option<SpeculativeLocalAsrDaemonConfig> {
    let model_id = "paraformer-bilingual-zh-en";
    let model_dir = model_root.join(model_id);
    let tokens = model_dir.join("tokens.txt");
    let encoder = model_dir.join("encoder.int8.onnx");
    let decoder = model_dir.join("decoder.int8.onnx");
    if !tokens.is_file() || !encoder.is_file() || !decoder.is_file() {
        return None;
    }

    Some(SpeculativeLocalAsrDaemonConfig {
        mode: SpeculativeLocalAsrDaemonMode::SherpaOnline,
        engine: None,
        model: Some(model_id.to_string()),
        dry_run_text: None,
        dry_run_partial_text: None,
        model_family: SpeculativeSherpaOnlineModelFamily::Paraformer,
        tokens: Some(tokens),
        encoder: Some(encoder),
        decoder: Some(decoder),
        joiner: None,
        provider: Some("cpu".to_string()),
        num_threads: Some(2),
        sample_rate_hz: Some(16_000),
        decoding_method: Some("greedy_search".to_string()),
        hotwords_file: None,
        rule_fsts: None,
        rule_fars: None,
    })
}

fn append_desktop_local_asr_daemon_args(
    args: &mut Vec<String>,
    config: &SpeculativeLocalAsrDaemonConfig,
) {
    append_desktop_daemon_arg(args, "--mode", config.mode.as_daemon_arg());
    append_optional_desktop_daemon_arg(args, "--engine", config.engine.as_deref());
    append_optional_desktop_daemon_arg(args, "--model", config.model.as_deref());
    append_optional_desktop_daemon_arg(args, "--dry-run-text", config.dry_run_text.as_deref());
    append_optional_desktop_daemon_arg(
        args,
        "--dry-run-partial-text",
        config.dry_run_partial_text.as_deref(),
    );

    if config.mode == SpeculativeLocalAsrDaemonMode::SherpaOnline {
        append_desktop_daemon_arg(args, "--model-family", config.model_family.as_daemon_arg());
        append_optional_desktop_daemon_path_arg(args, "--tokens", config.tokens.as_ref());
        append_optional_desktop_daemon_path_arg(args, "--encoder", config.encoder.as_ref());
        append_optional_desktop_daemon_path_arg(args, "--decoder", config.decoder.as_ref());
        append_optional_desktop_daemon_path_arg(args, "--joiner", config.joiner.as_ref());
        append_optional_desktop_daemon_arg(args, "--provider", config.provider.as_deref());
        if let Some(num_threads) = config.num_threads {
            append_desktop_daemon_arg(args, "--num-threads", &num_threads.to_string());
        }
        if let Some(sample_rate_hz) = config.sample_rate_hz {
            append_desktop_daemon_arg(args, "--sample-rate-hz", &sample_rate_hz.to_string());
        }
        append_optional_desktop_daemon_arg(
            args,
            "--decoding-method",
            config.decoding_method.as_deref(),
        );
        append_optional_desktop_daemon_path_arg(
            args,
            "--hotwords-file",
            config.hotwords_file.as_ref(),
        );
        append_optional_desktop_daemon_path_arg(args, "--rule-fsts", config.rule_fsts.as_ref());
        append_optional_desktop_daemon_path_arg(args, "--rule-fars", config.rule_fars.as_ref());
    }
}

fn append_optional_desktop_daemon_arg(args: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        append_desktop_daemon_arg(args, flag, value);
    }
}

fn append_optional_desktop_daemon_path_arg(
    args: &mut Vec<String>,
    flag: &str,
    value: Option<&PathBuf>,
) {
    if let Some(value) = value {
        append_desktop_daemon_arg(args, flag, &value.as_os_str().to_string_lossy());
    }
}

fn append_desktop_daemon_arg(args: &mut Vec<String>, flag: &str, value: &str) {
    args.push(flag.to_string());
    args.push(value.to_string());
}

pub fn desktop_local_asr_daemon_bind_from_endpoint(
    endpoint: &str,
) -> Result<Option<String>, String> {
    let subject = "speculative.streaming_service.endpoint";
    if endpoint.trim().is_empty() {
        return Err(format!("{subject} must not be blank"));
    }
    if endpoint.trim() != endpoint {
        return Err(format!(
            "{subject} must not have leading or trailing whitespace"
        ));
    }
    if endpoint.chars().any(char::is_whitespace) {
        return Err(format!("{subject} must not contain whitespace"));
    }
    if endpoint.contains('#') {
        return Err(format!("{subject} must not include a URL fragment"));
    }

    let Some((scheme, rest)) = endpoint.split_once("://") else {
        return Err(format!("{subject} must use ws or wss scheme"));
    };
    if !scheme.eq_ignore_ascii_case("ws") {
        return Ok(None);
    }

    let authority = rest
        .split(['/', '?'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| format!("{subject} must include a host"))?;
    if authority.contains('@') {
        return Err(format!("{subject} must not include user info"));
    }
    let (host, port) = desktop_endpoint_host_and_port(authority, subject)?;
    if host.is_empty() {
        return Err(format!("{subject} must include a host"));
    }
    let Some(port) = port else {
        return Ok(None);
    };
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{subject} port must be numeric"));
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| format!("{subject} port must be between 1 and 65535"))?;
    if port == 0 {
        return Err(format!("{subject} port must be between 1 and 65535"));
    }

    let bind_host = if host.eq_ignore_ascii_case("localhost") {
        "127.0.0.1".to_string()
    } else {
        let address = host
            .parse::<std::net::IpAddr>()
            .map_err(|_| format!("{subject} host must be loopback"))?;
        if !address.is_loopback() {
            return Err(format!("{subject} host must be loopback"));
        }
        match address {
            std::net::IpAddr::V4(_) => host.to_string(),
            std::net::IpAddr::V6(_) => format!("[{host}]"),
        }
    };

    Ok(Some(format!("{bind_host}:{port}")))
}

fn desktop_endpoint_host_and_port<'a>(
    authority: &'a str,
    subject: &str,
) -> Result<(&'a str, Option<&'a str>), String> {
    if let Some(bracketed) = authority.strip_prefix('[') {
        let Some(closing_bracket) = bracketed.find(']') else {
            return Err(format!("{subject} must include a host"));
        };
        let host = &bracketed[..closing_bracket];
        if !host.is_empty() && host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(format!(
                "{subject} bracketed host must be a valid IPv6 address"
            ));
        }
        let remainder = &bracketed[closing_bracket + 1..];
        if remainder.is_empty() {
            return Ok((host, None));
        }
        let Some(port) = remainder.strip_prefix(':') else {
            return Err(format!("{subject} port must be numeric"));
        };
        return Ok((host, Some(port)));
    }
    if authority.matches(':').count() > 1 {
        return Err(format!("{subject} IPv6 hosts must use [brackets]"));
    }
    if let Some((host, port)) = authority.rsplit_once(':') {
        Ok((host, Some(port)))
    } else {
        Ok((authority, None))
    }
}

fn parse_trigger_key(token: &str) -> Result<(String, u32), String> {
    if token.len() == 1 {
        let ch = token.chars().next().expect("single-char token");
        if ch.is_ascii_alphabetic() {
            let upper = ch.to_ascii_uppercase();
            return Ok((upper.to_string(), upper as u32));
        }
        if ch.is_ascii_digit() {
            return Ok((ch.to_string(), ch as u32));
        }
        if ch == '/' {
            return Ok(("Slash".to_string(), 0xBF));
        }
    }

    let named = match token.to_ascii_lowercase().as_str() {
        "space" => ("Space".to_string(), 0x20),
        "slash" => ("Slash".to_string(), 0xBF),
        "enter" | "return" => ("Enter".to_string(), 0x0D),
        "tab" => ("Tab".to_string(), 0x09),
        "escape" | "esc" => ("Escape".to_string(), 0x1B),
        "backspace" => ("Backspace".to_string(), 0x08),
        "rightalt" | "right alt" | "ralt" => ("RightAlt".to_string(), 0xA5),
        "up" => ("Up".to_string(), 0x26),
        "down" => ("Down".to_string(), 0x28),
        "left" => ("Left".to_string(), 0x25),
        "right" => ("Right".to_string(), 0x27),
        other if other.starts_with('f') => {
            let suffix = &other[1..];
            let index = suffix
                .parse::<u32>()
                .map_err(|_| format!("shortcut key '{token}' is not supported"))?;
            if !(1..=24).contains(&index) {
                return Err(format!("shortcut key '{token}' is not supported"));
            }
            return Ok((format!("F{index}"), 0x70 + (index - 1)));
        }
        _ => return Err(format!("shortcut key '{token}' is not supported")),
    };

    Ok(named)
}
