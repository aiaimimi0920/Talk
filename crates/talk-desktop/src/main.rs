#[cfg(not(windows))]
fn main() {
    eprintln!("talk-desktop is only available on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
mod windows_app {
    use anyhow::{Context, Result};
    use clap::Parser;
    use futures_util::FutureExt;
    use serde_json::Value;
    use std::cell::{Cell, RefCell};
    use std::collections::{hash_map::Entry, HashMap, HashSet, VecDeque};
    use std::fs;
    use std::future::Future;
    use std::mem;
    use std::net::{SocketAddr, TcpStream};
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::ptr;
    use std::sync::{Arc, Condvar, Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};
    use talk_audio::{
        probe_native_windows_audio_readiness_for_device, start_recording, AudioCaptureRequest,
        RecordingPcmSource, RecordingSession, WavSettings,
    };
    use talk_client::{
        final_transcript_from_streaming_asr_events, FrontContext, StreamingAsrEvent,
    };
    use talk_core::{
        AudioBackendMode, ClipboardBackendMode, DesktopPasteShortcut, OutputMode, ProviderKind,
        TalkConfig, TriggerMode, VoiceEvent, VoiceMode, VoiceSession,
    };
    use talk_desktop::{
        build_desktop_insert_target_diagnostic_with_trace,
        build_desktop_insert_target_trace_diagnostic, build_status_report, compose_hud_message,
        config_status_message, decide_speculative_patch_application,
        default_product_local_asr_model_spec, desktop_action_binding_label,
        desktop_action_bindings, desktop_copy_popup_action_for_virtual_key,
        desktop_copy_popup_activation_policy,
        desktop_copy_popup_close_button_rect as popup_close_button_layout_rect,
        desktop_copy_popup_copy_button_rect as popup_copy_button_layout_rect,
        desktop_copy_popup_copy_shows_follow_up_hud, desktop_copy_popup_editor_content_rect,
        desktop_copy_popup_editor_frame_rect as popup_editor_frame_layout_rect,
        desktop_copy_popup_metrics, desktop_copy_popup_model,
        desktop_copy_popup_model_for_mode_text_result, desktop_copy_popup_pane_layouts,
        desktop_copy_popup_position, desktop_correction_text_with_local_boundary,
        desktop_direct_control_paste_focus_handle,
        desktop_document_recorrection_generation_is_current,
        desktop_document_recorrection_session_decision, desktop_effective_streaming_asr_enabled,
        desktop_failure_cleanup_plan, desktop_final_correction_processing_mode,
        desktop_hud_activation_policy, desktop_hud_audio_meter_model_for_waveform,
        desktop_hud_detail_lifecycle, desktop_hud_geometry_update_plan,
        desktop_hud_metrics_for_view_model, desktop_hud_presentation_for_phase,
        desktop_hud_thinking_palette, desktop_hud_thinking_progress_model,
        desktop_hud_thinking_text_wave_offsets, desktop_hud_view_model_for_corrected_text,
        desktop_hud_view_model_for_listening_waveform_with_partial_and_lifecycle,
        desktop_hud_view_model_for_phase, desktop_insert_target_restore_requested,
        desktop_listening_hud_action_for_point, desktop_listening_hud_auto_follow_for_scroll,
        desktop_listening_hud_cancel_button_rect, desktop_listening_hud_complete_button_rect,
        desktop_listening_hud_line_origin, desktop_listening_hud_partial_text_layout,
        desktop_listening_hud_reconcile_scroll_state, desktop_listening_hud_requires_text_layout,
        desktop_listening_hud_scroll_line_offset_for_pointer,
        desktop_listening_hud_scroll_line_offset_for_wheel,
        desktop_listening_hud_scroll_max_offset, desktop_listening_hud_scrollbar_hit_rect,
        desktop_listening_hud_scrollbar_thumb_rect, desktop_listening_hud_text_unit_width,
        desktop_listening_hud_visible_lines, desktop_listening_hud_waveform_rect,
        desktop_live_clipboard_settle_delay_ms, desktop_live_correction_anchor_policy,
        desktop_live_correction_context_before, desktop_live_correction_eligibility,
        desktop_live_correction_inserted_baseline, desktop_live_correction_ordered_backlog,
        desktop_live_correction_presentation, desktop_live_correction_processing_mode,
        desktop_live_correction_target_apply_allowed, desktop_live_correction_timing_log,
        desktop_live_correction_worker_policy, desktop_live_smart_route_lock,
        desktop_live_smart_transcribe_evidence, desktop_live_streaming_segmenter_config,
        desktop_local_asr_daemon_bind_from_endpoint, desktop_mode_dropdown_model,
        desktop_mode_text_result_model, desktop_output_plan, desktop_overlay_scale_factor_for_dpi,
        desktop_packaged_local_asr_daemon_launch_plan_with_config,
        desktop_preferred_paste_shortcut_for_target,
        desktop_product_local_asr_daemon_launch_plan_with_config,
        desktop_product_local_asr_model_available, desktop_product_local_asr_startup_timeout_ms,
        desktop_resolve_product_local_asr_model_root, desktop_runtime_insert_directive_for_mode,
        desktop_shortcut_help_activation_policy, desktop_shortcut_help_metrics,
        desktop_shortcut_help_metrics_for_entry_count, desktop_shortcut_help_model,
        desktop_shortcut_help_position, desktop_speculative_cloud_correction_enabled,
        desktop_speculative_correction_job_model, desktop_speculative_local_asr_route,
        desktop_speculative_replacement_selection_count, desktop_streaming_effective_segment_count,
        desktop_streaming_final_correction_job_enabled,
        desktop_streaming_hud_transcript_parts_for_auto_follow,
        desktop_streaming_hud_transcript_parts_text,
        desktop_streaming_hud_transcript_summary_apply_fallback_text,
        desktop_streaming_hud_transcript_summary_owned,
        desktop_streaming_hud_transcript_summary_with_fallback_text_owned,
        desktop_streaming_latest_segment_allows_auto_patch, desktop_streaming_segment_cache_text,
        desktop_streaming_stop_aggregate_with_pending, desktop_streaming_stop_policy,
        desktop_streaming_stop_reconciliation_plan, desktop_streaming_stop_tail_target_unchanged,
        desktop_target_matched_context_for_paste, desktop_ui_color_tokens,
        desktop_ui_derived_colors, download_and_install_model,
        embedded_runtime_payload_is_appended, extract_embedded_runtime_payload,
        foreground_target_refresh_requested, foreground_target_stability_satisfied,
        hotkey_status_message, hud_message_for_phase, hydrate_foreground_insert_target_focus,
        idle_status_detail, live_streaming_segment_plan_for_lifecycle,
        locate_verified_embedded_runtime, native_status_message,
        observe_foreground_target_stability, parse_desktop_window_handle,
        recording_stop_watcher_policy, resolve_default_desktop_config_path,
        resolve_desktop_audio_file_override, resolve_foreground_focus_capture,
        resolve_hotkey_origin_insert_target, resolve_hotkey_recording_origin_enrichment,
        resolve_pending_hotkey_origin_capture, resolve_talk_data_root,
        scale_desktop_overlay_length, select_foreground_insert_target,
        select_windows_hotkey_binding_strategy, tray_menu_model, validate_installed_model,
        windows_hotkey_binding_registration_plan, write_desktop_insert_target_diagnostic,
        ConfigAvailability, DesktopActionBinding, DesktopActionRoute, DesktopCopyPopupAction,
        DesktopCopyPopupMetrics, DesktopCopyPopupModel, DesktopCopyPopupPaneModel,
        DesktopDocumentRecorrectionDecision, DesktopHudGeometry, DesktopHudMetrics,
        DesktopHudPresentation, DesktopHudViewModel, DesktopHudVisualState,
        DesktopInsertTargetContext, DesktopInsertTargetRestoreDiagnostic,
        DesktopListeningHudAction, DesktopListeningHudPartialTextLayout,
        DesktopLiveCorrectionAnchorPolicy, DesktopLiveCorrectionBacklogItem,
        DesktopLiveCorrectionEligibility, DesktopLiveCorrectionPresentation,
        DesktopLiveCorrectionSegment, DesktopLiveStreamingLocalSegmentPlan,
        DesktopLocalAsrDaemonLaunchPlan, DesktopOutputStrategy, DesktopOverlayActivationPolicy,
        DesktopOverlayRect, DesktopRecordingStopWatcherPolicy, DesktopRuntimeInsertDirective,
        DesktopShortcutHelpMetrics, DesktopShortcutHelpModel,
        DesktopSpeculativeCorrectionOutputTarget, DesktopSpeculativeLocalAsrRoute,
        DesktopSpeculativePipelineConfig, DesktopStreamingHudTranscriptParts,
        DesktopStreamingStopReconciliationPlan, DesktopTextLifecycleState, DesktopUiColorTokens,
        DesktopUiDerivedColors, ForegroundInsertTarget, ForegroundTargetReleaseReason,
        ForegroundTargetStabilityProgress, HotkeyBindingState, HotkeySpec, LastSessionStatus,
        LowLevelHotkeyTracker, LowLevelHotkeyTransition, ModelSpec, NativeBackendSnapshot,
        NativeReadinessSnapshot, ShellState, SpeculativeInsertAnchor, SpeculativePatchApplication,
        SpeculativePatchCandidate, StatusSnapshot, ToggleDesktopHotkeyRouter,
        ToggleDesktopHotkeyRouterPendingHold, WindowsHotkeyBindingRegistrationPlan,
        WindowsHotkeyBindingStrategy, TALK_DESKTOP_AUDIO_FILE_OVERRIDE_ENV,
        TALK_DESKTOP_INSERT_TARGET_FOCUS_ENV, TALK_DESKTOP_INSERT_TARGET_WINDOW_ENV,
        TALK_PACKAGED_LOCAL_ASR_DAEMON_EXE_NAME,
    };
    use talk_insert::{
        flush_pending_clipboard_restore, probe_native_windows_clipboard_readiness,
        set_windows_paste_thread_overrides, ClipboardBackend, ClipboardPasteInserter,
        ClipboardRestorePolicy, ConfiguredWindowsPasteShortcut, TextInserter,
        WindowsClipboardBackend, WindowsPasteOverrides, WindowsPasteShortcutMode,
    };
    use talk_runtime::{
        analyze_smart_voice_mode, complete_cancelled_session,
        complete_failed_session_with_mode_override, load_effective_config,
        process_voice_transcript_text_with_diagnostics,
        provider_text_processing_credentials_available,
        run_local_streaming_asr_service_from_recording, run_mock_speculative_session,
        run_voice_session_from_audio_artifact_with_insert_hooks,
        run_voice_session_from_audio_artifact_with_route_evidence_and_insert_hooks,
        run_voice_session_from_external_asr_command_with_insert_hooks,
        run_voice_session_from_local_transcript_with_route_evidence_and_insert_hooks,
        run_voice_session_from_transcript_with_route_evidence_and_insert_hooks,
        runtime_voice_text_result, smart_transcribe_fallback_is_stable,
        update_session_log_after_text_processing, LocalStreamingAsrLiveSession,
        RuntimeInsertDirective, RuntimePhase, RuntimeProcessedOutput, SegmenterConfig,
        SmartRouteEvidence, SmartRouteReason, SpeculativeRuntimeEvent, SpeculativeRuntimeState,
    };
    use tokio::runtime::Builder;
    use uiautomation::patterns::{UITextPattern, UIValuePattern};
    use uiautomation::types::ControlType as UiAutomationControlType;
    use uiautomation::types::Handle as UiAutomationHandle;
    use uiautomation::{UIAutomation, UIElement};
    use uuid::Uuid;
    use windows::Win32::Foundation::{HWND as WinHwnd, RPC_E_CHANGED_MODE};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows_sys::Win32::Foundation::{
        CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
        CreateRectRgn, CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawFocusRect,
        DrawTextW, Ellipse, EndPaint, FillRect, FrameRect, GetDC, GetStockObject,
        GetTextExtentPoint32W, InvalidateRect, LineTo, MoveToEx, ReleaseDC, RoundRect,
        SelectObject, SetBkColor, SetBkMode, SetTextColor, SetWindowRgn, TextOutW,
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH,
        DT_CALCRECT, DT_CENTER, DT_EDITCONTROL, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
        DT_WORDBREAK, FF_DONTCARE, FW_BOLD, HOLLOW_BRUSH, OPAQUE, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
        PS_SOLID, SRCCOPY, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::HiDpi::{
        GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetFocus, RegisterHotKey, ReleaseCapture, SendInput, SetActiveWindow,
        SetCapture, SetFocus, TrackMouseEvent, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD,
        KEYBDINPUT, KEYEVENTF_KEYUP, TME_LEAVE, TRACKMOUSEEVENT, VK_LEFT, VK_SHIFT,
    };
    use windows_sys::Win32::UI::Shell::{
        DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass, Shell_NotifyIconW, NIF_ICON,
        NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, BringWindowToTop, CallNextHookEx, CreatePopupMenu, CreateWindowExW,
        DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
        GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetParent, GetSystemMetrics,
        GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsIconic, IsWindow, KillTimer, LoadCursorW, LoadIconW, MessageBoxW, PostMessageW,
        PostQuitMessage, RegisterClassW, SendMessageW, SetForegroundWindow, SetTimer,
        SetWindowLongPtrW, SetWindowPos, SetWindowTextW, SetWindowsHookExW, ShowWindow,
        TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, CREATESTRUCTW, CS_HREDRAW,
        CS_VREDRAW, CW_USEDEFAULT, EN_CHANGE, ES_AUTOVSCROLL, ES_CENTER, ES_MULTILINE,
        GUITHREADINFO, GWLP_USERDATA, HC_ACTION, HHOOK, IDC_ARROW, IDI_APPLICATION,
        KBDLLHOOKSTRUCT, MB_ICONINFORMATION, MB_OK, MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING,
        MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNOACTIVATE, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TPM_RIGHTBUTTON, WH_KEYBOARD_LL, WM_APP, WM_CAPTURECHANGED, WM_CHAR, WM_COMMAND,
        WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_DESTROY, WM_ERASEBKGND, WM_GETFONT, WM_HOTKEY,
        WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_RBUTTONUP, WM_SETFONT, WM_SYSCHAR,
        WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WNDCLASSW, WS_CHILD, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
        WS_VSCROLL,
    };

    #[cfg(test)]
    use talk_desktop::DesktopShortcutHelpEntry;
    #[cfg(test)]
    use windows_sys::Win32::Graphics::Gdi::GetPixel;
    #[cfg(test)]
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindowVisible};

    /// Acquires a mutex while recovering from poisoning instead of panicking.
    ///
    /// A panic on any thread holding one of the desktop mutexes would otherwise
    /// poison the lock and cascade into panics on every later acquisition,
    /// killing the tray application without running cleanup paths.
    fn lock_recovering<'a, T>(mutex: &'a Mutex<T>, context: &str) -> std::sync::MutexGuard<'a, T> {
        mutex.lock().unwrap_or_else(|poisoned| {
            eprintln!("Talk desktop mutex poisoned ({context}); recovering last known state");
            poisoned.into_inner()
        })
    }

    const WINDOW_CLASS_NAME: &str = "TalkDesktopMessageWindow";
    const HUD_WINDOW_CLASS_NAME: &str = "TalkDesktopHudWindow";
    const COPY_POPUP_WINDOW_CLASS_NAME: &str = "TalkDesktopCopyPopupWindow";
    const SHORTCUT_HELP_WINDOW_CLASS_NAME: &str = "TalkDesktopShortcutHelpWindow";
    const COPY_POPUP_EDIT_CLASS_NAME: &str = "EDIT";
    const HOTKEY_ID: i32 = 1;
    const COPY_POPUP_EDIT_CONTROL_ID: isize = 2001;
    const COPY_POPUP_MAX_PANES: usize = 4;
    const COPY_POPUP_EDIT_SUBCLASS_ID: usize = 1;
    const EM_SETREADONLY_MESSAGE: u32 = 0x00CF;
    const VK_TAB_KEY: u32 = 0x09;
    const VK_CONTROL_KEY: u16 = 0x11;
    const VK_LEFT_CONTROL_KEY: u16 = 0xA2;
    const VK_RIGHT_CONTROL_KEY: u16 = 0xA3;
    const VK_MENU_KEY: i32 = 0x12;
    const VK_LEFT_MENU_KEY: u16 = 0xA4;
    const VK_RIGHT_MENU_KEY: u16 = 0xA5;
    const VK_LEFT_SHIFT_KEY: u16 = 0xA0;
    const VK_RIGHT_SHIFT_KEY: u16 = 0xA1;
    const VK_A_KEY: u16 = 0x41;
    const TRAY_ICON_ID: u32 = 1;
    const TRAY_MESSAGE: u32 = WM_APP + 1;
    const PHASE_MESSAGE: u32 = WM_APP + 2;
    const STOP_MESSAGE: u32 = WM_APP + 3;
    const WORKER_DONE_MESSAGE: u32 = WM_APP + 4;
    const LOW_LEVEL_HOTKEY_RELEASE_MESSAGE: u32 = WM_APP + 5;
    const HOTKEY_ACTION_MESSAGE: u32 = WM_APP + 6;
    const HOTKEY_PENDING_HOLD_START_MESSAGE: u32 = WM_APP + 7;
    const HOTKEY_PENDING_HOLD_CANCEL_MESSAGE: u32 = WM_APP + 8;
    const CORRECTION_COPY_POPUP_MESSAGE: u32 = WM_APP + 9;
    const MODEL_BOOTSTRAP_MESSAGE: u32 = WM_APP + 10;
    const CORRECTED_HUD_MESSAGE: u32 = WM_APP + 11;
    const STREAMING_CORRECTED_HUD_MESSAGE: u32 = WM_APP + 12;
    const STREAMING_PUMP_DONE_MESSAGE: u32 = WM_APP + 13;
    const LOCAL_ASR_PREPARE_DONE_MESSAGE: u32 = WM_APP + 14;
    const AUDIO_START_DONE_MESSAGE: u32 = WM_APP + 15;
    const CONFIG_RELOAD_DONE_MESSAGE: u32 = WM_APP + 16;
    const TIMER_HIDE_HUD: usize = 1;
    const TIMER_SHORTCUT_HELP_HOLD: usize = 2;
    const TIMER_RECORDING_LEVEL: usize = 3;
    const TIMER_THINKING_PROGRESS: usize = 4;
    const TIMER_RECORDING_TIMEOUT: usize = 5;
    const HUD_RECORDING_LEVEL_REFRESH_MS: u32 = 48;
    const HUD_THINKING_PROGRESS_REFRESH_MS: u32 = 72;
    const HUD_WAVEFORM_BUCKET_COUNT: usize = 9;
    const HUD_STREAMING_ASR_EVENTS_PER_REFRESH: usize = 64;
    const STREAMING_PUMP_PARTIAL_IDLE_TIMEOUT_MS: u64 = 1;
    const COPY_POPUP_CORNER_RADIUS: i32 = 6;
    const SHORTCUT_HELP_CORNER_RADIUS: i32 = 6;
    const CREATE_NO_WINDOW_FLAG: u32 = 0x08000000;
    const SHORTCUT_HELP_HOLD_DELAY_MS: u32 = 650;
    const HOTKEY_ORIGIN_ENRICH_POLL_INTERVAL_MS: u64 = 35;
    const HOTKEY_ORIGIN_ENRICH_MAX_POLLS: usize = 6;
    const HOTKEY_ORIGIN_ENRICH_SOURCE: &str = "hotkey_post_start_enrichment";
    const INSERT_TARGET_POST_INSERT_POLL_INTERVAL_MS: u64 = 30;
    const INSERT_TARGET_POST_INSERT_MAX_HOLD_MS: u64 = 480;
    const INSERT_TARGET_POST_INSERT_REQUIRED_STABLE_FOREGROUND_POLLS: u32 = 4;

    const MENU_START: u16 = 1001;
    const MENU_STOP: u16 = 1002;
    const MENU_CANCEL: u16 = 1003;
    const MENU_SHOW_STATUS: u16 = 1004;
    const MENU_OPEN_LOGS: u16 = 1005;
    const MENU_OPEN_CONFIG: u16 = 1006;
    const MENU_RELOAD_CONFIG: u16 = 1007;
    const MENU_EXIT: u16 = 1008;
    const MENU_MODE_SMART: u16 = 1010;
    const MENU_MODE_TRANSCRIBE: u16 = 1011;
    const MENU_MODE_DOCUMENT: u16 = 1012;
    const MENU_MODE_COMMAND: u16 = 1013;
    const MENU_MODE_GENERATE: u16 = 1014;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ActivationSource {
        Hotkey,
        Tray,
    }

    #[derive(Debug, Parser)]
    #[command(
        name = "talk-desktop",
        version,
        about = "Talk OpenLess/Typeless-style Windows desktop shell"
    )]
    struct Cli {
        #[arg(long)]
        config: Option<PathBuf>,
    }

    enum ActiveRecordingSource {
        Live {
            recording: RecordingSession,
            streaming_pump: Option<LocalStreamingAsrPumpController>,
        },
        ExplicitAudioFile(PathBuf),
    }

    enum StoppedRecordingSource {
        AudioFile(PathBuf),
        LiveRecording {
            recording: RecordingSession,
            streaming_pump: Option<LocalStreamingAsrPumpController>,
        },
        StreamingRecording {
            recording: RecordingSession,
            streaming_pump: Option<LocalStreamingAsrPumpController>,
        },
        StreamingEvents {
            events: Result<Vec<StreamingAsrEvent>>,
            audio_path: Option<PathBuf>,
        },
        RecordingFinalizeFailed(String),
    }

    struct ActiveRecording {
        action_index: usize,
        mode_override: Option<VoiceMode>,
        live_smart_routed_mode: Option<VoiceMode>,
        generation: u64,
        session: VoiceSession,
        trigger_events: Vec<&'static str>,
        origin_insert_target: Option<DesktopInsertTargetContext>,
        origin_insert_target_source: Option<String>,
        pending_hotkey_origin_insert_target: Option<DesktopInsertTargetContext>,
        release_time_origin_insert_target: Option<DesktopInsertTargetContext>,
        source: ActiveRecordingSource,
        use_streaming_speculative_asr: bool,
        speculative_runtime_state: SpeculativeRuntimeState,
        speculative_segmenter_config: SegmenterConfig,
        live_streaming_inserted_anchors: HashMap<String, SpeculativeInsertAnchor>,
        live_streaming_inserted_segment_ids: Vec<String>,
        hud_streaming_segments: Arc<Vec<(String, String)>>,
        hud_streaming_segments_revision: u64,
        live_streaming_correction_sender:
            Option<tokio::sync::mpsc::Sender<SpeculativeCloudCorrectionJob>>,
        live_streaming_correction_tracker: Arc<LiveCorrectionTracker>,
        live_correction_snapshot_revision: u64,
        live_correction_snapshot: Vec<DesktopLiveCorrectionSegment>,
        last_streaming_asr_event: Option<StreamingAsrEvent>,
        last_streaming_asr_event_at: Option<Instant>,
        last_streaming_idle_evaluated_ms: u64,
        hud_waveform: [f32; HUD_WAVEFORM_BUCKET_COUNT],
        pending_streaming_pump_results:
            VecDeque<std::result::Result<Vec<StreamingAsrEvent>, String>>,
        streaming_unavailable_hint: Option<String>,
    }

    struct PendingRecordingBegin {
        action_index: usize,
        source: ActivationSource,
        config: Arc<TalkConfig>,
        runtime_handle: tokio::runtime::Handle,
        hotkey: Option<HotkeySpec>,
        mode_override: Option<VoiceMode>,
        generation: u64,
        trigger_mode: TriggerMode,
        max_recording_seconds: u64,
        session_id: String,
        config_path: PathBuf,
        session: VoiceSession,
        trigger_events: Vec<&'static str>,
        origin_insert_target: Option<DesktopInsertTargetContext>,
        origin_insert_target_source: Option<String>,
        pending_hotkey_origin_insert_target: Option<DesktopInsertTargetContext>,
        release_time_origin_target: Option<DesktopInsertTargetContext>,
        local_asr_result: Option<std::result::Result<bool, String>>,
        audio_start_result:
            Option<std::result::Result<PreparedRecordingAudio, RecordingAudioStartFailure>>,
    }

    struct PreparedRecordingAudio {
        source: ActiveRecordingSource,
        use_streaming_speculative_asr: bool,
    }

    struct RecordingAudioStartFailure {
        message: String,
        reason: &'static str,
        hud_detail: String,
    }

    struct PendingConfigReload {
        generation: u64,
        result: Option<std::result::Result<PreparedConfigReload, String>>,
    }

    struct PreparedConfigReload {
        config: TalkConfig,
        selected_voice_mode: VoiceMode,
        shortcut_label: String,
        hotkey_binding:
            std::result::Result<(Vec<DesktopActionBinding>, HotkeyBindingState), String>,
        native_readiness: NativeReadinessSnapshot,
        uses_streaming_service: bool,
    }

    enum PreparedProductBootstrap {
        EngineeringFallback,
        FallbackCloud(String),
        Ready {
            worker_path: PathBuf,
            model_root: PathBuf,
        },
        Download {
            worker_path: PathBuf,
            model_root: PathBuf,
            primary_model_root: PathBuf,
            spec: ModelSpec,
        },
    }

    #[derive(Clone)]
    struct LocalStreamingAsrPumpController {
        pump_sender: tokio::sync::mpsc::Sender<()>,
        terminal_sender: tokio::sync::mpsc::Sender<LocalStreamingAsrPumpTerminalCommand>,
    }

    enum LocalStreamingAsrPumpTerminalCommand {
        Stop(tokio::sync::oneshot::Sender<Result<Vec<StreamingAsrEvent>>>),
        Cancel(tokio::sync::oneshot::Sender<Result<()>>),
    }

    struct SharedState {
        config: Option<Arc<TalkConfig>>,
        config_status: ConfigAvailability,
        config_path: PathBuf,
        hotkey: HotkeyBindingState,
        desktop_actions: Vec<DesktopActionBinding>,
        selected_voice_mode: VoiceMode,
        native_readiness: Option<NativeReadinessSnapshot>,
        shell_state: ShellState,
        current_phase: Option<RuntimePhase>,
        last_session: Option<LastSessionStatus>,
        active_recording: Option<ActiveRecording>,
        pending_recording_begin: Option<PendingRecordingBegin>,
        worker_generation: Option<u64>,
        pending_worker_error: Option<(u64, String)>,
        pending_worker_task: Option<(u64, tokio::task::JoinHandle<()>)>,
        pending_stop_live_correction_tracker: Option<Arc<LiveCorrectionTracker>>,
        pending_copy_popup: Option<PendingCopyPopup>,
        pending_corrected_hud: Option<PendingCorrectedHud>,
        pending_hotkey_origin_insert_target: Option<DesktopInsertTargetContext>,
        local_asr_daemon: Option<ManagedLocalAsrDaemon>,
        local_asr_daemon_epoch: u64,
        local_asr_daemon_lifecycle: Arc<Mutex<()>>,
        local_asr_bootstrap_status: LocalAsrBootstrapStatus,
        product_runtime_worker: Option<PathBuf>,
        product_model_root: Option<PathBuf>,
        correction_worker_gate: CorrectionWorkerGate,
        foreground_apply_gate: ForegroundApplyGate,
        pending_document_correction_task: Option<(u64, tokio::task::JoinHandle<()>)>,
        pending_model_bootstrap_task: Option<tokio::task::JoinHandle<()>>,
        product_bootstrap_generation: u64,
        pending_product_bootstrap_prepare_generation: Option<u64>,
        config_reload_generation: u64,
        pending_config_reload: Option<PendingConfigReload>,
        shutting_down: bool,
        runtime_handle: tokio::runtime::Handle,
        next_generation: u64,
    }

    struct ManagedLocalAsrDaemon {
        endpoint: String,
        launch_plan: DesktopLocalAsrDaemonLaunchPlan,
        child: std::process::Child,
    }

    #[derive(Clone)]
    struct CorrectionWorkerGate {
        semaphore: Arc<tokio::sync::Semaphore>,
    }

    #[derive(Clone)]
    struct ForegroundApplyGate {
        state: Arc<(Mutex<bool>, Condvar)>,
    }

    struct ForegroundApplyLease {
        state: Arc<(Mutex<bool>, Condvar)>,
    }

    impl ForegroundApplyGate {
        fn new() -> Self {
            Self {
                state: Arc::new((Mutex::new(false), Condvar::new())),
            }
        }

        fn acquire(&self) -> ForegroundApplyLease {
            let (active, idle) = &*self.state;
            let mut active = active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            while *active {
                active = idle
                    .wait(active)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            *active = true;
            ForegroundApplyLease {
                state: Arc::clone(&self.state),
            }
        }
    }

    impl Drop for ForegroundApplyLease {
        fn drop(&mut self) {
            let (active, idle) = &*self.state;
            let mut active = active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *active = false;
            drop(active);
            idle.notify_one();
        }
    }

    impl CorrectionWorkerGate {
        fn new(max_concurrency: usize) -> Self {
            Self {
                semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrency)),
            }
        }

        async fn acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
            Arc::clone(&self.semaphore).acquire_owned().await.ok()
        }
    }

    impl LocalStreamingAsrPumpController {
        fn request_pump(&self) {
            let _ = self.pump_sender.try_send(());
        }

        async fn stop(self) -> Result<Vec<StreamingAsrEvent>> {
            let (reply_sender, reply_receiver) = tokio::sync::oneshot::channel();
            self.terminal_sender
                .send(LocalStreamingAsrPumpTerminalCommand::Stop(reply_sender))
                .await
                .context("send stop command to local streaming ASR pump")?;
            reply_receiver
                .await
                .context("receive stop result from local streaming ASR pump")?
        }

        async fn cancel(self) -> Result<()> {
            let (reply_sender, reply_receiver) = tokio::sync::oneshot::channel();
            self.terminal_sender
                .send(LocalStreamingAsrPumpTerminalCommand::Cancel(reply_sender))
                .await
                .context("send cancel command to local streaming ASR pump")?;
            reply_receiver
                .await
                .context("receive cancel result from local streaming ASR pump")?
        }
    }

    fn spawn_local_streaming_asr_pump(
        runtime_handle: &tokio::runtime::Handle,
        shared: Arc<Mutex<SharedState>>,
        hwnd: HWND,
        generation: u64,
        source: RecordingPcmSource,
        config: Arc<TalkConfig>,
        session_id: String,
    ) -> LocalStreamingAsrPumpController {
        let (pump_sender, mut pump_receiver) = tokio::sync::mpsc::channel::<()>(1);
        let (terminal_sender, mut terminal_receiver) =
            tokio::sync::mpsc::channel::<LocalStreamingAsrPumpTerminalCommand>(1);
        let hwnd_value = hwnd as usize;
        runtime_handle.spawn(async move {
            let mut session = None::<LocalStreamingAsrLiveSession>;
            loop {
                tokio::select! {
                    biased;
                    command = terminal_receiver.recv() => {
                        match command {
                            Some(LocalStreamingAsrPumpTerminalCommand::Stop(reply)) => {
                                let result = match session.take() {
                                    Some(session) => session.stop_from_pcm_source(&source).await,
                                    None => match LocalStreamingAsrLiveSession::start(
                                        &config,
                                        &session_id,
                                        None,
                                    )
                                    .await
                                    {
                                        Ok(session) => session.stop_from_pcm_source(&source).await,
                                        Err(error) => Err(error),
                                    },
                                };
                                let _ = reply.send(result);
                            }
                            Some(LocalStreamingAsrPumpTerminalCommand::Cancel(reply)) => {
                                let result = match session.take() {
                                    Some(session) => session.cancel().await,
                                    None => Ok(()),
                                };
                                let _ = reply.send(result);
                            }
                            None => {
                                if let Some(session) = session.take() {
                                    let _ = session.cancel().await;
                                }
                            }
                        }
                        return;
                    }
                    pump = pump_receiver.recv() => {
                        if pump.is_none() {
                            if let Some(session) = session.take() {
                                let _ = session.cancel().await;
                            }
                            return;
                        }
                        if session.is_none() {
                            match LocalStreamingAsrLiveSession::start(
                                &config,
                                &session_id,
                                None,
                            )
                            .await
                            {
                                Ok(started) => session = Some(started),
                                Err(error) => {
                                    let result = Err(error.to_string());
                                    let should_post = shared.lock().ok().is_some_and(|mut shared| {
                                        if shared.shutting_down
                                            || shared.current_phase != Some(RuntimePhase::Recording)
                                        {
                                            return false;
                                        }
                                        let Some(active) = shared.active_recording.as_mut() else {
                                            return false;
                                        };
                                        if active.generation != generation {
                                            return false;
                                        }
                                        active.pending_streaming_pump_results.push_back(result);
                                        true
                                    });
                                    if should_post {
                                        unsafe {
                                            let _ = PostMessageW(
                                                hwnd_value as HWND,
                                                STREAMING_PUMP_DONE_MESSAGE,
                                                generation as usize,
                                                0,
                                            );
                                        }
                                    }
                                    return;
                                }
                            }
                        }
                        let result = session
                            .as_mut()
                            .expect("local streaming ASR session started")
                            .pump_available_pcm_source(
                                &source,
                                Duration::from_millis(STREAMING_PUMP_PARTIAL_IDLE_TIMEOUT_MS),
                            )
                            .await
                            .map_err(|error| error.to_string());
                        let failed = result.is_err();
                        let should_post = shared.lock().ok().is_some_and(|mut shared| {
                            if shared.shutting_down
                                || shared.current_phase != Some(RuntimePhase::Recording)
                            {
                                return false;
                            }
                            let Some(active) = shared.active_recording.as_mut() else {
                                return false;
                            };
                            if active.generation != generation {
                                return false;
                            }
                            active.pending_streaming_pump_results.push_back(result);
                            true
                        });
                        if should_post {
                            unsafe {
                                let _ = PostMessageW(
                                    hwnd_value as HWND,
                                    STREAMING_PUMP_DONE_MESSAGE,
                                    generation as usize,
                                    0,
                                );
                            }
                        }
                        if failed {
                            if let Some(session) = session.take() {
                                let _ = session.cancel().await;
                            }
                            return;
                        }
                    }
                }
            }
        });
        LocalStreamingAsrPumpController {
            pump_sender,
            terminal_sender,
        }
    }

    fn abort_pending_task(pending: &mut Option<(u64, tokio::task::JoinHandle<()>)>) {
        if let Some((_, task)) = pending.take() {
            task.abort();
        }
    }

    fn abort_pending_join_handle(pending: &mut Option<tokio::task::JoinHandle<()>>) {
        if let Some(task) = pending.take() {
            task.abort();
        }
    }

    fn register_pending_worker_task(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
        task: tokio::task::JoinHandle<()>,
    ) {
        let mut task = Some(task);
        if let Ok(mut shared) = shared.lock() {
            if !shared.shutting_down && shared.worker_generation == Some(generation) {
                if let Some((_, previous_task)) = shared.pending_worker_task.take() {
                    previous_task.abort();
                }
                shared.pending_worker_task = Some((generation, task.take().expect("worker task")));
            }
        }
        if let Some(task) = task {
            task.abort();
        }
    }

    fn stop_worker_side_effects_allowed(
        shutting_down: bool,
        worker_generation: Option<u64>,
        next_generation: u64,
        generation: u64,
    ) -> bool {
        !shutting_down
            && worker_generation == Some(generation)
            && desktop_document_recorrection_generation_is_current(next_generation, generation)
    }

    fn stop_worker_side_effects_allowed_for_shared(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
    ) -> bool {
        shared.lock().ok().is_some_and(|shared| {
            stop_worker_side_effects_allowed(
                shared.shutting_down,
                shared.worker_generation,
                shared.next_generation,
                generation,
            )
        })
    }

    fn model_bootstrap_side_effects_allowed(
        shutting_down: bool,
        current_generation: u64,
        generation: u64,
    ) -> bool {
        !shutting_down && current_generation == generation
    }

    fn correction_foreground_side_effects_allowed(
        shutting_down: bool,
        next_generation: u64,
        generation: u64,
    ) -> bool {
        !shutting_down
            && desktop_document_recorrection_generation_is_current(next_generation, generation)
    }

    fn correction_foreground_side_effects_allowed_for_shared(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
    ) -> bool {
        shared.lock().ok().is_some_and(|shared| {
            correction_foreground_side_effects_allowed(
                shared.shutting_down,
                shared.next_generation,
                generation,
            )
        })
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    struct PasteShortcutModifierKeyState {
        control: bool,
        alt: bool,
        shift: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct PasteShortcutModifierReleaseWaitOutcome {
        cleared: bool,
        poll_count: u32,
        initial_state: PasteShortcutModifierKeyState,
        final_state: PasteShortcutModifierKeyState,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct PasteShortcutModifierPreparation {
        wait_outcome: PasteShortcutModifierReleaseWaitOutcome,
        force_release: bool,
    }

    fn paste_shortcut_modifier_keys_clear(state: PasteShortcutModifierKeyState) -> bool {
        !state.control && !state.alt && !state.shift
    }

    fn wait_for_paste_shortcut_modifier_release_with_sampler<F>(
        timeout: Duration,
        poll_interval: Duration,
        mut sample: F,
    ) -> PasteShortcutModifierReleaseWaitOutcome
    where
        F: FnMut() -> PasteShortcutModifierKeyState,
    {
        let initial_state = sample();
        let mut final_state = initial_state;
        let mut poll_count = 1u32;
        if paste_shortcut_modifier_keys_clear(initial_state) {
            return PasteShortcutModifierReleaseWaitOutcome {
                cleared: true,
                poll_count,
                initial_state,
                final_state,
            };
        }

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            thread::sleep(poll_interval);
            final_state = sample();
            poll_count += 1;
            if paste_shortcut_modifier_keys_clear(final_state) {
                return PasteShortcutModifierReleaseWaitOutcome {
                    cleared: true,
                    poll_count,
                    initial_state,
                    final_state,
                };
            }
        }

        PasteShortcutModifierReleaseWaitOutcome {
            cleared: false,
            poll_count,
            initial_state,
            final_state,
        }
    }

    fn sample_paste_shortcut_modifier_key_state() -> PasteShortcutModifierKeyState {
        PasteShortcutModifierKeyState {
            control: unsafe { GetAsyncKeyState(VK_CONTROL_KEY as i32) } < 0,
            alt: unsafe { GetAsyncKeyState(VK_MENU_KEY) } < 0,
            shift: unsafe { GetAsyncKeyState(VK_SHIFT as i32) } < 0,
        }
    }

    fn prepare_paste_shortcut_modifier_state_with_sampler<F>(
        timeout: Duration,
        poll_interval: Duration,
        sample: F,
    ) -> PasteShortcutModifierPreparation
    where
        F: FnMut() -> PasteShortcutModifierKeyState,
    {
        let wait_outcome =
            wait_for_paste_shortcut_modifier_release_with_sampler(timeout, poll_interval, sample);
        PasteShortcutModifierPreparation {
            force_release: !wait_outcome.cleared
                && !paste_shortcut_modifier_keys_clear(wait_outcome.final_state),
            wait_outcome,
        }
    }

    fn force_release_pressed_paste_modifiers(state: PasteShortcutModifierKeyState) -> Result<()> {
        if paste_shortcut_modifier_keys_clear(state) {
            return Ok(());
        }

        let mut inputs = Vec::new();
        if state.control {
            inputs.push(keyboard_input(VK_LEFT_CONTROL_KEY, KEYEVENTF_KEYUP));
            inputs.push(keyboard_input(VK_RIGHT_CONTROL_KEY, KEYEVENTF_KEYUP));
            inputs.push(keyboard_input(VK_CONTROL_KEY, KEYEVENTF_KEYUP));
        }
        if state.alt {
            inputs.push(keyboard_input(VK_LEFT_MENU_KEY, KEYEVENTF_KEYUP));
            inputs.push(keyboard_input(VK_RIGHT_MENU_KEY, KEYEVENTF_KEYUP));
            inputs.push(keyboard_input(VK_MENU_KEY as u16, KEYEVENTF_KEYUP));
        }
        if state.shift {
            inputs.push(keyboard_input(VK_LEFT_SHIFT_KEY, KEYEVENTF_KEYUP));
            inputs.push(keyboard_input(VK_RIGHT_SHIFT_KEY, KEYEVENTF_KEYUP));
            inputs.push(keyboard_input(VK_SHIFT, KEYEVENTF_KEYUP));
        }

        if inputs.is_empty() {
            return Ok(());
        }

        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                mem::size_of::<INPUT>() as i32,
            )
        };
        if sent != inputs.len() as u32 {
            anyhow::bail!(
                "SendInput(release lingering modifiers) sent {sent}/{} key events",
                inputs.len()
            );
        }
        Ok(())
    }

    fn prepare_paste_shortcut_modifier_state() -> PasteShortcutModifierPreparation {
        let preparation = prepare_paste_shortcut_modifier_state_with_sampler(
            Duration::from_millis(40),
            Duration::from_millis(5),
            sample_paste_shortcut_modifier_key_state,
        );
        if preparation.force_release {
            if let Err(error) =
                force_release_pressed_paste_modifiers(preparation.wait_outcome.final_state)
            {
                eprintln!("Talk forced modifier release before paste failed: {error:#}");
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
        preparation
    }

    fn should_prepare_paste_shortcut_modifiers(direct_control_paste_active: bool) -> bool {
        !direct_control_paste_active
    }

    #[derive(Debug, Clone)]
    enum LocalAsrBootstrapStatus {
        NotStarted,
        EngineeringFallback,
        Downloading,
        Ready,
        FallbackCloud(String),
    }

    struct PendingCopyPopup {
        generation: u64,
        model: DesktopCopyPopupModel,
    }

    struct PendingCorrectedHud {
        generation: u64,
        text: String,
    }

    struct SpeculativeCloudCorrectionJob {
        config: Arc<TalkConfig>,
        segment_id: String,
        transcript: String,
        context_before: Option<String>,
        processing_mode: VoiceMode,
        requested_mode: VoiceMode,
        origin_insert_target: Option<DesktopInsertTargetContext>,
        anchor: Option<SpeculativeInsertAnchor>,
        full_document_inserted_segments: Vec<String>,
        session_log_path: Option<PathBuf>,
        latest_live_segment_guard: Option<LatestLiveSegmentGuard>,
        allow_target_apply: bool,
        generation: u64,
        started_at: Instant,
        hwnd_value: usize,
        hud_hwnd_value: usize,
    }

    struct LiveCorrectionTiming {
        segment_id: String,
        queued_at: Instant,
        worker_started_at: Instant,
        provider_started_at: Option<Instant>,
        provider_finished_at: Option<Instant>,
        apply_started_at: Option<Instant>,
        outcome: &'static str,
    }

    impl LiveCorrectionTiming {
        fn new(job: &SpeculativeCloudCorrectionJob) -> Self {
            Self {
                segment_id: job.segment_id.clone(),
                queued_at: job.started_at,
                worker_started_at: Instant::now(),
                provider_started_at: None,
                provider_finished_at: None,
                apply_started_at: None,
                outcome: "skipped",
            }
        }

        fn provider_started(&mut self) {
            self.provider_started_at = Some(Instant::now());
        }

        fn provider_finished(&mut self) {
            self.provider_finished_at = Some(Instant::now());
        }

        fn apply_started(&mut self) {
            self.apply_started_at = Some(Instant::now());
        }

        fn outcome(&mut self, outcome: &'static str) {
            self.outcome = outcome;
        }
    }

    impl Drop for LiveCorrectionTiming {
        fn drop(&mut self) {
            let queue_wait_ms = self
                .worker_started_at
                .saturating_duration_since(self.queued_at)
                .as_millis();
            let provider_ms = self
                .provider_started_at
                .zip(self.provider_finished_at)
                .map(|(started, finished)| finished.saturating_duration_since(started).as_millis())
                .unwrap_or_default();
            let apply_ms = self
                .apply_started_at
                .map(|started| started.elapsed().as_millis())
                .unwrap_or_default();
            let total_ms = self.queued_at.elapsed().as_millis();
            eprintln!(
                "{}",
                desktop_live_correction_timing_log(
                    &self.segment_id,
                    queue_wait_ms,
                    provider_ms,
                    apply_ms,
                    total_ms,
                    self.outcome,
                )
            );
        }
    }

    struct LiveCorrectionApplyLease<'a> {
        _guard: std::sync::MutexGuard<'a, ()>,
    }

    struct LiveCorrectionTracker {
        apply_gate: Mutex<()>,
        state: Mutex<LiveCorrectionTrackerState>,
        idle_notify: tokio::sync::Notify,
    }

    struct LiveCorrectionTrackerState {
        segments: Vec<DesktopLiveCorrectionSegment>,
        segment_indices: HashMap<String, usize>,
        segments_revision: u64,
        pending_jobs: usize,
        completed_jobs: HashSet<String>,
        stopping: bool,
        cancelled: bool,
    }

    impl LiveCorrectionTrackerState {
        fn segment_index(&self, segment_id: &str) -> Option<usize> {
            self.segment_indices.get(segment_id).copied()
        }

        fn segment(&self, segment_id: &str) -> Option<&DesktopLiveCorrectionSegment> {
            self.segments.get(self.segment_index(segment_id)?)
        }

        fn segment_mut(&mut self, segment_id: &str) -> Option<&mut DesktopLiveCorrectionSegment> {
            let index = self.segment_index(segment_id)?;
            self.segments.get_mut(index)
        }
    }

    impl LiveCorrectionTracker {
        fn new(_generation: u64) -> Self {
            Self {
                apply_gate: Mutex::new(()),
                state: Mutex::new(LiveCorrectionTrackerState {
                    segments: Vec::new(),
                    segment_indices: HashMap::new(),
                    segments_revision: 0,
                    pending_jobs: 0,
                    completed_jobs: HashSet::new(),
                    stopping: false,
                    cancelled: false,
                }),
                idle_notify: tokio::sync::Notify::new(),
            }
        }

        fn register_job(
            &self,
            segment_id: &str,
            local_text: &str,
            anchor: Option<SpeculativeInsertAnchor>,
        ) -> bool {
            let Ok(mut state) = self.state.lock() else {
                return false;
            };
            if state.cancelled || state.stopping || state.segment_indices.contains_key(segment_id) {
                return false;
            }
            let segment_index = state.segments.len();
            let segment_id = segment_id.to_string();
            state.segments.push(DesktopLiveCorrectionSegment {
                segment_id: segment_id.clone(),
                local_text: local_text.to_string(),
                corrected_text: None,
                insert_anchor: anchor,
            });
            state.segment_indices.insert(segment_id, segment_index);
            state.segments_revision = state.segments_revision.wrapping_add(1);
            state.pending_jobs += 1;
            true
        }

        fn corrected_context_before(&self, segment_id: &str, max_chars: usize) -> String {
            let Ok(state) = self.state.lock() else {
                return String::new();
            };
            desktop_live_correction_context_before(&state.segments, segment_id, max_chars)
        }

        fn can_process(&self, segment_id: &str) -> bool {
            let Ok(state) = self.state.lock() else {
                return false;
            };
            !state.cancelled
                && state
                    .segment(segment_id)
                    .is_some_and(|segment| segment.corrected_text.is_none())
        }

        fn acquire_apply_lease(&self, segment_id: &str) -> Option<LiveCorrectionApplyLease<'_>> {
            let guard = self.apply_gate.lock().ok()?;
            if !self.can_process(segment_id) {
                return None;
            }
            Some(LiveCorrectionApplyLease { _guard: guard })
        }

        fn should_show_live_feedback(&self) -> bool {
            self.state
                .lock()
                .ok()
                .is_some_and(|state| !state.cancelled && !state.stopping)
        }

        fn record_result(
            &self,
            segment_id: &str,
            corrected_text: &str,
            anchor: Option<SpeculativeInsertAnchor>,
        ) {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if state.cancelled {
                return;
            }
            if let Some(segment) = state.segment_mut(segment_id) {
                let mut segments_changed = false;
                if segment.corrected_text.as_deref() != Some(corrected_text) {
                    segment.corrected_text = Some(corrected_text.to_string());
                    segments_changed = true;
                }
                if let Some(anchor) = anchor {
                    if segment.insert_anchor.as_ref() != Some(&anchor) {
                        segment.insert_anchor = Some(anchor);
                        segments_changed = true;
                    }
                }
                if segments_changed {
                    state.segments_revision = state.segments_revision.wrapping_add(1);
                }
            }
        }

        fn local_fallback_text(&self, segment_id: &str) -> Option<String> {
            let state = self.state.lock().ok()?;
            if state.cancelled {
                return None;
            }
            let segment = state.segment(segment_id)?;
            if segment.corrected_text.is_some() {
                return None;
            }
            Some(segment.local_text.clone())
        }

        fn has_insert_anchor(&self, segment_id: &str) -> bool {
            self.state.lock().ok().is_some_and(|state| {
                state
                    .segment(segment_id)
                    .is_some_and(|segment| segment.insert_anchor.is_some())
            })
        }

        fn ordered_backlog(&self) -> Vec<DesktopLiveCorrectionBacklogItem> {
            self.state
                .lock()
                .map(|state| desktop_live_correction_ordered_backlog(&state.segments))
                .unwrap_or_default()
        }

        fn complete_job(
            &self,
            segment_id: &str,
            corrected_text: Option<&str>,
            anchor: Option<SpeculativeInsertAnchor>,
        ) {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            let mut segments_changed = false;
            if let Some(segment) = state.segment_mut(segment_id) {
                if let Some(corrected_text) = corrected_text {
                    if segment.corrected_text.as_deref() != Some(corrected_text) {
                        segment.corrected_text = Some(corrected_text.to_string());
                        segments_changed = true;
                    }
                }
                if let Some(anchor) = anchor {
                    if segment.insert_anchor.as_ref() != Some(&anchor) {
                        segment.insert_anchor = Some(anchor);
                        segments_changed = true;
                    }
                }
            }
            if segments_changed {
                state.segments_revision = state.segments_revision.wrapping_add(1);
            }
            state.completed_jobs.insert(segment_id.to_string());
            state.pending_jobs = state.pending_jobs.saturating_sub(1);
            self.idle_notify.notify_waiters();
        }

        fn mark_stopping(&self) {
            if let Ok(mut state) = self.state.lock() {
                state.stopping = true;
                if state.pending_jobs == 0 {
                    self.idle_notify.notify_waiters();
                }
            }
        }

        fn cancel(&self) {
            let _apply_guard = match self.apply_gate.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Ok(mut state) = self.state.lock() {
                state.cancelled = true;
                state.stopping = true;
            }
            self.idle_notify.notify_waiters();
        }

        fn is_cancelled(&self) -> bool {
            self.state.lock().ok().is_some_and(|state| state.cancelled)
        }

        async fn wait_until_cancelled(&self) {
            loop {
                let notified = self.idle_notify.notified();
                if self.is_cancelled() {
                    return;
                }
                notified.await;
            }
        }

        async fn wait_until_apply_turn(&self, segment_id: &str) -> bool {
            loop {
                let notified = self.idle_notify.notified();
                let decision = self.state.lock().ok().and_then(|state| {
                    if state.cancelled {
                        return Some(false);
                    }
                    let index = state.segment_index(segment_id)?;
                    if state
                        .segments
                        .get(..index)?
                        .iter()
                        .all(|segment| state.completed_jobs.contains(&segment.segment_id))
                    {
                        Some(true)
                    } else {
                        None
                    }
                });
                if let Some(decision) = decision {
                    return decision;
                }
                notified.await;
            }
        }

        async fn wait_until_idle(&self) {
            loop {
                let notified = self.idle_notify.notified();
                let is_idle = self
                    .state
                    .lock()
                    .ok()
                    .is_some_and(|state| state.pending_jobs == 0);
                if is_idle {
                    return;
                }
                notified.await;
            }
        }

        fn snapshot(&self) -> Vec<DesktopLiveCorrectionSegment> {
            self.snapshot_with_revision()
                .map(|(_, segments)| segments)
                .unwrap_or_default()
        }

        fn snapshot_with_revision(&self) -> Option<(u64, Vec<DesktopLiveCorrectionSegment>)> {
            self.state
                .lock()
                .ok()
                .map(|state| (state.segments_revision, state.segments.clone()))
        }

        fn current_revision(&self) -> Option<u64> {
            self.state.lock().ok().map(|state| state.segments_revision)
        }

        fn refresh_snapshot_if_changed(
            &self,
            revision: &mut u64,
            snapshot: &mut Vec<DesktopLiveCorrectionSegment>,
        ) -> bool {
            let Ok(state) = self.state.lock() else {
                snapshot.clear();
                *revision = 0;
                return false;
            };
            if *revision == state.segments_revision {
                return false;
            }
            snapshot.clone_from(&state.segments);
            *revision = state.segments_revision;
            true
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct LatestLiveSegmentGuard {
        generation: u64,
    }

    struct PendingLiveStreamingDispatch {
        config: Arc<TalkConfig>,
        pipeline_config: DesktopSpeculativePipelineConfig,
        generation: u64,
        origin_insert_target: Option<DesktopInsertTargetContext>,
        existing_anchors: HashMap<String, SpeculativeInsertAnchor>,
        live_correction_sender: tokio::sync::mpsc::Sender<SpeculativeCloudCorrectionJob>,
        live_correction_tracker: Arc<LiveCorrectionTracker>,
        events: Vec<SpeculativeRuntimeEvent>,
        requested_mode: VoiceMode,
        smart_routed_mode: Option<VoiceMode>,
        flush_corrected_backlog: bool,
        latest_live_segment_guard: Option<LatestLiveSegmentGuard>,
        allow_target_apply: bool,
        hwnd_value: usize,
        hud_hwnd_value: usize,
    }

    struct PendingLiveStreamingDispatchSeed {
        live_correction_tracker: Arc<LiveCorrectionTracker>,
        events: Vec<SpeculativeRuntimeEvent>,
        requested_mode: VoiceMode,
        smart_routed_mode: Option<VoiceMode>,
        flush_corrected_backlog: bool,
    }

    struct LiveStreamingDispatchMetadata {
        config: Arc<TalkConfig>,
        origin_insert_target: Option<DesktopInsertTargetContext>,
        existing_anchors: HashMap<String, SpeculativeInsertAnchor>,
        live_correction_sender: tokio::sync::mpsc::Sender<SpeculativeCloudCorrectionJob>,
    }

    struct PendingLiveCorrectionTranscript {
        tracker_revision: u64,
        tracker_snapshot: Vec<DesktopLiveCorrectionSegment>,
        hud_streaming_segments_revision: u64,
        latest_asr_text: Option<String>,
        fallback_text: Option<String>,
        transcript_parts: DesktopStreamingHudTranscriptParts,
        route_was_requested: bool,
        routed_mode: Option<VoiceMode>,
    }

    struct RecordingHudProcessingState {
        speculative_runtime_state: SpeculativeRuntimeState,
        hud_streaming_segments: Arc<Vec<(String, String)>>,
        hud_streaming_segments_revision: u64,
        live_correction_snapshot_revision: u64,
        live_correction_snapshot: Vec<DesktopLiveCorrectionSegment>,
        last_streaming_asr_event: Option<StreamingAsrEvent>,
        last_streaming_asr_event_at: Option<Instant>,
        last_streaming_idle_evaluated_ms: u64,
        live_smart_routed_mode: Option<VoiceMode>,
    }

    struct PendingRecordingHudRefresh {
        generation: u64,
        requested_mode: VoiceMode,
        segmenter_config: SegmenterConfig,
        processing_state: RecordingHudProcessingState,
        pending_streaming_pump_results:
            VecDeque<std::result::Result<Vec<StreamingAsrEvent>, String>>,
        waveform_source: Option<RecordingPcmSource>,
        raw_waveform: [f32; HUD_WAVEFORM_BUCKET_COUNT],
        streaming_pump: Option<LocalStreamingAsrPumpController>,
        streaming_unavailable_hint: Option<String>,
        live_correction_tracker: Arc<LiveCorrectionTracker>,
    }

    struct ProcessedRecordingHudRefresh {
        generation: u64,
        processing_state: RecordingHudProcessingState,
        pending_streaming_pump_results:
            VecDeque<std::result::Result<Vec<StreamingAsrEvent>, String>>,
        raw_waveform: [f32; HUD_WAVEFORM_BUCKET_COUNT],
        streaming_unavailable_hint: Option<String>,
        refreshed_hud_transcript_parts: Option<DesktopStreamingHudTranscriptParts>,
        live_dispatch_seed: Option<PendingLiveStreamingDispatchSeed>,
        streaming_pump_error: Option<String>,
    }

    enum LowLevelHookState {
        OriginCapture {
            hwnd_value: isize,
            tracker: LowLevelHotkeyTracker,
        },
        Single {
            hwnd_value: isize,
            trigger_mode: TriggerMode,
            tracker: LowLevelHotkeyTracker,
        },
        ToggleRouter {
            hwnd_value: isize,
            router: ToggleDesktopHotkeyRouter,
        },
    }

    struct WindowState {
        shared: Arc<Mutex<SharedState>>,
        hud_hwnd: Cell<HWND>,
        copy_popup_hwnd: Cell<HWND>,
        copy_popup_edit_hwnd: Cell<HWND>,
        copy_popup_pane_edit_hwnds: RefCell<Vec<HWND>>,
        copy_popup_restore_foreground_hwnd: Cell<HWND>,
        copy_popup_restore_focus_hwnd: Cell<HWND>,
        shortcut_help_hwnd: Cell<HWND>,
        recording_timeout_generation: Cell<Option<u64>>,
    }

    #[derive(Debug, Clone)]
    struct CapturedForegroundFocusTarget {
        focus_hwnd: Option<HWND>,
        primary_focus_hwnd: Option<HWND>,
        fallback_focus_hwnd: Option<HWND>,
        caret_hwnd: Option<HWND>,
        focus_class_name: Option<String>,
    }

    #[derive(Debug, Clone, Default)]
    struct CapturedAutomationFocusTarget {
        control_type: Option<String>,
        framework_id: Option<String>,
        runtime_id: Option<Vec<i32>>,
        is_keyboard_focusable: Option<bool>,
        supports_text_pattern: bool,
        supports_value_pattern: bool,
    }

    fn low_level_hook_state() -> &'static Mutex<Option<LowLevelHookState>> {
        static STATE: OnceLock<Mutex<Option<LowLevelHookState>>> = OnceLock::new();
        STATE.get_or_init(|| Mutex::new(None))
    }

    fn ensure_uia_com_initialized_for_current_thread() {
        thread_local! {
            static UIA_COM_READY: Cell<bool> = const { Cell::new(false) };
        }

        UIA_COM_READY.with(|ready| {
            if ready.get() {
                return;
            }

            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if result.is_ok() || result == RPC_E_CHANGED_MODE {
                ready.set(true);
            }
        });
    }

    fn low_level_hook_handle() -> &'static Mutex<Option<isize>> {
        static HANDLE: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
        HANDLE.get_or_init(|| Mutex::new(None))
    }

    fn with_low_level_toggle_router<T>(
        f: impl FnOnce(&mut ToggleDesktopHotkeyRouter, HWND) -> T,
    ) -> Option<T> {
        let mut state =
            lock_recovering(low_level_hook_state(), "Talk desktop low-level hook state");
        let LowLevelHookState::ToggleRouter { hwnd_value, router } = state.as_mut()? else {
            return None;
        };
        Some(f(router, *hwnd_value as HWND))
    }

    #[derive(Debug, Clone)]
    struct CopyPopupRenderState {
        model: DesktopCopyPopupModel,
        hovered_control: CopyPopupHoveredControl,
        pressed_control: CopyPopupHoveredControl,
        keyboard_focused_control: CopyPopupHoveredControl,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    enum CopyPopupHoveredControl {
        #[default]
        None,
        Copy,
        Close,
    }

    #[derive(Debug, Default)]
    struct OverlayUiState {
        hud_model: Option<DesktopHudViewModel>,
        hud_geometry: Option<DesktopHudGeometry>,
        hud_meter_bins: [f32; 9],
        hud_streaming_corrected_prefix: Option<String>,
        hud_streaming_partial_tail: Option<String>,
        hud_streaming_partial_layout: Option<DesktopListeningHudPartialTextLayout>,
        hud_streaming_scroll_line_offset: usize,
        hud_streaming_scroll_dragging: bool,
        hud_streaming_scroll_user_scrolled: bool,
        hud_thinking_pulse_tick: u32,
        copy_popup: Option<CopyPopupRenderState>,
        shortcut_help: Option<DesktopShortcutHelpModel>,
    }

    enum RecordingHudRefreshUpdate {
        Text(DesktopHudViewModel),
        WaveformOnly,
    }

    fn overlay_ui_state() -> &'static Mutex<OverlayUiState> {
        static STATE: OnceLock<Mutex<OverlayUiState>> = OnceLock::new();
        STATE.get_or_init(|| Mutex::new(OverlayUiState::default()))
    }

    fn upsert_hud_streaming_segment(
        segments: &mut Vec<(String, String)>,
        segment_id: &str,
        text: &str,
    ) -> bool {
        let Some(text) = desktop_streaming_segment_cache_text(text) else {
            return false;
        };

        if let Some((last_segment_id, last_text)) = segments.last_mut() {
            if last_segment_id == segment_id {
                return update_hud_streaming_segment_text(last_text, text);
            }
        }

        let search_end = segments.len().saturating_sub(1);
        if let Some((_, existing_text)) = segments[..search_end]
            .iter_mut()
            .find(|(existing_segment_id, _)| existing_segment_id == segment_id)
        {
            return update_hud_streaming_segment_text(existing_text, text);
        } else {
            segments.push((segment_id.to_string(), text.to_string()));
        }
        true
    }

    fn update_hud_streaming_segment_text(existing_text: &mut String, text: &str) -> bool {
        if existing_text == text {
            return false;
        }
        existing_text.clear();
        existing_text.push_str(text);
        true
    }

    /// Drop streaming HUD segments whose ids were rolled back by an ASR revision
    /// that segmented into fewer pieces. Retained/re-committed segments keep
    /// their position, so display order is preserved.
    fn remove_hud_streaming_segments(
        segments: &mut Vec<(String, String)>,
        segment_ids: &[String],
    ) -> bool {
        if segment_ids.is_empty() {
            return false;
        }
        let previous_len = segments.len();
        if let [removed] = segment_ids {
            segments.retain(|(segment_id, _)| segment_id != removed);
        } else {
            let removed = segment_ids
                .iter()
                .map(String::as_str)
                .collect::<HashSet<_>>();
            segments.retain(|(segment_id, _)| !removed.contains(segment_id.as_str()));
        }
        segments.len() != previous_len
    }

    fn display_text_units(text: &str) -> usize {
        text.chars()
            .map(|character| if character.is_ascii() { 1 } else { 2 })
            .sum()
    }

    fn live_smart_route_for_transcript(
        requested_mode: VoiceMode,
        current_lock: Option<VoiceMode>,
        transcript: &str,
        committed_segment_count: usize,
    ) -> Option<VoiceMode> {
        if requested_mode != VoiceMode::Smart {
            return None;
        }
        let analysis = analyze_smart_voice_mode(
            transcript,
            SmartRouteEvidence {
                committed_streaming_segment_count: committed_segment_count,
            },
        );
        desktop_live_smart_route_lock(
            current_lock,
            analysis.resolved_mode,
            analysis.long_form_evidence,
        )
    }

    fn live_smart_route_for_corrected_transcript(
        requested_mode: VoiceMode,
        current_lock: Option<VoiceMode>,
        transcript: &str,
        committed_segment_count: usize,
    ) -> Option<VoiceMode> {
        if requested_mode != VoiceMode::Smart {
            return None;
        }
        let analysis = analyze_smart_voice_mode(
            transcript,
            SmartRouteEvidence {
                committed_streaming_segment_count: committed_segment_count,
            },
        );
        let transcribe_evidence = desktop_live_smart_transcribe_evidence(
            analysis.resolved_mode,
            analysis.long_form_evidence,
            analysis.reason == SmartRouteReason::TranscribeFallback
                && smart_transcribe_fallback_is_stable(transcript),
        );
        desktop_live_smart_route_lock(current_lock, analysis.resolved_mode, transcribe_evidence)
    }

    fn commit_live_smart_route_candidate(
        requested_mode: VoiceMode,
        current_lock: Option<VoiceMode>,
        candidate_is_current: bool,
        candidate: Option<VoiceMode>,
    ) -> Option<VoiceMode> {
        if requested_mode == VoiceMode::Smart && current_lock.is_none() && candidate_is_current {
            candidate
        } else {
            current_lock
        }
    }

    fn live_correction_job_target_apply_allowed(
        job_allow_target_apply: bool,
        has_live_segment_guard: bool,
        requested_mode: VoiceMode,
        smart_routed_mode: Option<VoiceMode>,
    ) -> bool {
        job_allow_target_apply
            || (has_live_segment_guard
                && desktop_live_correction_target_apply_allowed(requested_mode, smart_routed_mode))
    }

    fn live_correction_backlog_drain_allowed(
        dispatch_allow_target_apply: bool,
        flush_corrected_backlog: bool,
        rejected_local_fallback: bool,
        requested_mode: VoiceMode,
        smart_routed_mode: Option<VoiceMode>,
    ) -> bool {
        dispatch_allow_target_apply
            && (flush_corrected_backlog || rejected_local_fallback)
            && desktop_live_correction_target_apply_allowed(requested_mode, smart_routed_mode)
    }

    fn listening_hud_transcript_text_and_lifecycle(
        parts: &DesktopStreamingHudTranscriptParts,
    ) -> (Option<String>, DesktopTextLifecycleState) {
        let corrected_is_empty = parts.corrected_prefix.trim().is_empty();
        let pre_recognized_is_empty = parts.pre_recognized_tail.trim().is_empty();
        let lifecycle = if pre_recognized_is_empty && !corrected_is_empty {
            DesktopTextLifecycleState::Corrected
        } else {
            DesktopTextLifecycleState::PreRecognized
        };

        let mut display_text =
            String::with_capacity(parts.corrected_prefix.len() + parts.pre_recognized_tail.len());
        display_text.push_str(&parts.corrected_prefix);
        display_text.push_str(&parts.pre_recognized_tail);
        let Some((trim_start, trim_end)) = ({
            let trimmed = display_text.trim();
            (!trimmed.is_empty()).then(|| {
                let trim_start = trimmed.as_ptr() as usize - display_text.as_ptr() as usize;
                (trim_start, trim_start + trimmed.len())
            })
        }) else {
            return (None, lifecycle);
        };
        display_text.truncate(trim_end);
        if trim_start > 0 {
            display_text.drain(..trim_start);
        }

        (Some(display_text), lifecycle)
    }

    const LISTENING_HUD_AUTO_FOLLOW_MAX_DISPLAY_UNITS: usize = 640;

    fn listening_hud_presented_transcript_parts(
        parts: &DesktopStreamingHudTranscriptParts,
        user_scrolled: bool,
    ) -> DesktopStreamingHudTranscriptParts {
        if user_scrolled {
            parts.clone()
        } else {
            desktop_streaming_hud_transcript_parts_for_auto_follow(
                parts,
                LISTENING_HUD_AUTO_FOLLOW_MAX_DISPLAY_UNITS,
            )
        }
    }

    fn desktop_windows_paste_mode(shortcut: DesktopPasteShortcut) -> WindowsPasteShortcutMode {
        match shortcut {
            DesktopPasteShortcut::ControlV => WindowsPasteShortcutMode::ControlV,
            DesktopPasteShortcut::ControlShiftV => WindowsPasteShortcutMode::ControlShiftV,
            DesktopPasteShortcut::ShiftInsert => WindowsPasteShortcutMode::ShiftInsert,
        }
    }

    fn configured_windows_paste_shortcut(
        preferred_mode: Option<DesktopPasteShortcut>,
        focus_handle: Option<isize>,
    ) -> ConfiguredWindowsPasteShortcut {
        let mut shortcut = ConfiguredWindowsPasteShortcut::new();
        if let Some(mode) = preferred_mode {
            shortcut = shortcut.with_shortcut_mode(desktop_windows_paste_mode(mode));
        }
        if let Some(hwnd) = focus_handle {
            shortcut = shortcut.with_target_hwnd(hwnd);
        }
        shortcut
    }

    fn windows_paste_overrides_for_target(
        preferred_mode: Option<DesktopPasteShortcut>,
        focus_handle: Option<isize>,
    ) -> WindowsPasteOverrides {
        let mut overrides = WindowsPasteOverrides::new();
        if let Some(mode) = preferred_mode {
            overrides = overrides.with_shortcut_mode(desktop_windows_paste_mode(mode));
        }
        if let Some(hwnd) = focus_handle {
            overrides = overrides.with_target_hwnd(hwnd);
        }
        overrides
    }

    fn cached_hud_streaming_transcript_parts() -> DesktopStreamingHudTranscriptParts {
        overlay_ui_state()
            .lock()
            .ok()
            .map(|overlay| DesktopStreamingHudTranscriptParts {
                corrected_prefix: overlay
                    .hud_streaming_corrected_prefix
                    .clone()
                    .unwrap_or_default(),
                pre_recognized_tail: overlay
                    .hud_streaming_partial_tail
                    .clone()
                    .unwrap_or_default(),
            })
            .unwrap_or(DesktopStreamingHudTranscriptParts {
                corrected_prefix: String::new(),
                pre_recognized_tail: String::new(),
            })
    }

    fn enable_desktop_dpi_awareness() {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    fn fallback_overlay_dpi() -> u32 {
        unsafe {
            let dpi = GetDpiForSystem();
            if dpi == 0 {
                96
            } else {
                dpi
            }
        }
    }

    fn overlay_dpi_for_window(hwnd: HWND) -> u32 {
        if hwnd.is_null() {
            return fallback_overlay_dpi();
        }

        unsafe {
            let dpi = GetDpiForWindow(hwnd);
            if dpi == 0 {
                fallback_overlay_dpi()
            } else {
                dpi
            }
        }
    }

    fn scale_hud_metrics_for_dpi(metrics: DesktopHudMetrics, dpi: u32) -> DesktopHudMetrics {
        let _ = desktop_overlay_scale_factor_for_dpi(dpi);
        DesktopHudMetrics {
            width: scale_desktop_overlay_length(metrics.width, dpi),
            height: scale_desktop_overlay_length(metrics.height, dpi),
            bottom_margin: scale_desktop_overlay_length(metrics.bottom_margin, dpi),
            corner_radius: scale_desktop_overlay_length(metrics.corner_radius, dpi).max(0),
        }
    }

    fn scale_copy_popup_metrics_for_dpi(
        metrics: DesktopCopyPopupMetrics,
        dpi: u32,
    ) -> DesktopCopyPopupMetrics {
        DesktopCopyPopupMetrics {
            width: scale_desktop_overlay_length(metrics.width, dpi),
            height: scale_desktop_overlay_length(metrics.height, dpi),
            bottom_margin: scale_desktop_overlay_length(metrics.bottom_margin, dpi),
        }
    }

    fn scale_shortcut_help_metrics_for_dpi(
        metrics: DesktopShortcutHelpMetrics,
        dpi: u32,
    ) -> DesktopShortcutHelpMetrics {
        DesktopShortcutHelpMetrics {
            width: scale_desktop_overlay_length(metrics.width, dpi),
            height: scale_desktop_overlay_length(metrics.height, dpi),
            bottom_margin: scale_desktop_overlay_length(metrics.bottom_margin, dpi),
        }
    }

    pub fn run() -> Result<()> {
        enable_desktop_dpi_awareness();
        let cli = Cli::parse();
        let config_path = resolve_default_desktop_config_path(
            cli.config.as_deref(),
            &std::env::current_dir().context("resolve Talk desktop working directory")?,
            &std::env::current_exe().context("resolve Talk desktop executable path")?,
        );
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("build Talk desktop tokio runtime")?;

        let shared = Arc::new(Mutex::new(SharedState {
            config: None,
            config_status: ConfigAvailability::loading(),
            config_path,
            hotkey: HotkeyBindingState::Unconfigured,
            desktop_actions: Vec::new(),
            selected_voice_mode: VoiceMode::Smart,
            native_readiness: None,
            shell_state: ShellState::idle(),
            current_phase: None,
            last_session: None,
            active_recording: None,
            pending_recording_begin: None,
            worker_generation: None,
            pending_worker_error: None,
            pending_worker_task: None,
            pending_stop_live_correction_tracker: None,
            pending_copy_popup: None,
            pending_corrected_hud: None,
            pending_hotkey_origin_insert_target: None,
            local_asr_daemon: None,
            local_asr_daemon_epoch: 0,
            local_asr_daemon_lifecycle: Arc::new(Mutex::new(())),
            local_asr_bootstrap_status: LocalAsrBootstrapStatus::NotStarted,
            product_runtime_worker: None,
            product_model_root: None,
            correction_worker_gate: CorrectionWorkerGate::new(
                desktop_live_correction_worker_policy().max_concurrency,
            ),
            foreground_apply_gate: ForegroundApplyGate::new(),
            pending_document_correction_task: None,
            pending_model_bootstrap_task: None,
            product_bootstrap_generation: 0,
            pending_product_bootstrap_prepare_generation: None,
            config_reload_generation: 0,
            pending_config_reload: None,
            shutting_down: false,
            runtime_handle: runtime.handle().clone(),
            next_generation: 1,
        }));

        let window = Box::new(WindowState {
            shared: Arc::clone(&shared),
            hud_hwnd: Cell::new(ptr::null_mut()),
            copy_popup_hwnd: Cell::new(ptr::null_mut()),
            copy_popup_edit_hwnd: Cell::new(ptr::null_mut()),
            copy_popup_pane_edit_hwnds: RefCell::new(Vec::new()),
            copy_popup_restore_foreground_hwnd: Cell::new(ptr::null_mut()),
            copy_popup_restore_focus_hwnd: Cell::new(ptr::null_mut()),
            shortcut_help_hwnd: Cell::new(ptr::null_mut()),
            recording_timeout_generation: Cell::new(None),
        });
        let window_ptr = Box::into_raw(window);

        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        if instance.is_null() {
            unsafe {
                drop(Box::from_raw(window_ptr));
            }
            anyhow::bail!("get Talk desktop module handle");
        }

        register_window_class(instance)?;
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                to_wide(WINDOW_CLASS_NAME).as_ptr(),
                to_wide("Talk Desktop").as_ptr(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                window_ptr.cast(),
            )
        };
        if hwnd.is_null() {
            unsafe {
                drop(Box::from_raw(window_ptr));
            }
            anyhow::bail!("create Talk desktop message window");
        }

        if let Err(error) = initialize_window(hwnd, instance) {
            unsafe {
                DestroyWindow(hwnd);
            }
            return Err(error);
        }
        reload_config(hwnd)?;

        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } > 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        drop(runtime);
        Ok(())
    }

    fn register_window_class(instance: HINSTANCE) -> Result<()> {
        register_named_window_class(instance, WINDOW_CLASS_NAME, window_proc)?;
        register_named_window_class(instance, HUD_WINDOW_CLASS_NAME, hud_window_proc)?;
        register_named_window_class(
            instance,
            COPY_POPUP_WINDOW_CLASS_NAME,
            copy_popup_window_proc,
        )?;
        register_named_window_class(
            instance,
            SHORTCUT_HELP_WINDOW_CLASS_NAME,
            shortcut_help_window_proc,
        )?;
        Ok(())
    }

    fn register_named_window_class(
        instance: HINSTANCE,
        class_name: &str,
        proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    ) -> Result<()> {
        let class_name_wide = to_wide(class_name);
        let cursor = unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW) };
        let icon = unsafe { LoadIconW(ptr::null_mut(), IDI_APPLICATION) };
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class_name_wide.as_ptr(),
            hCursor: cursor,
            hIcon: icon,
            hbrBackground: terminal_window_class_background_brush() as _,
            ..unsafe { mem::zeroed() }
        };
        let atom = unsafe { RegisterClassW(&class) };
        if atom == 0 {
            anyhow::bail!("register Talk desktop window class '{class_name}'");
        }
        Ok(())
    }

    fn initialize_window(hwnd: HWND, instance: HINSTANCE) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        state.hud_hwnd.set(create_hud_window(instance, hwnd)?);
        state
            .copy_popup_hwnd
            .set(create_copy_popup_window(instance, hwnd)?);
        state
            .shortcut_help_hwnd
            .set(create_shortcut_help_window(instance, hwnd)?);

        let (startup_message, tray_status) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            register_or_mark_hotkey_failure(hwnd, &mut shared);
            let summary = current_idle_status(&shared);
            (
                compose_hud_message(summary, current_idle_detail(&shared).as_deref()),
                summary.to_string(),
            )
        };
        update_tray_icon(hwnd, &tray_status)?;
        if startup_message != "Talk: idle" {
            show_hud_text(hwnd, &startup_message, Some(1800))?;
        }

        Ok(())
    }

    fn create_hud_window(instance: HINSTANCE, owner_hwnd: HWND) -> Result<HWND> {
        let metrics = scale_hud_metrics_for_dpi(
            desktop_hud_metrics_for_view_model(&desktop_hud_view_model_for_phase(
                RuntimePhase::Processing,
            )),
            fallback_overlay_dpi(),
        );
        let ex_style = match desktop_hud_activation_policy() {
            DesktopOverlayActivationPolicy::NoActivate => {
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
            }
            DesktopOverlayActivationPolicy::ActivateOnInteract => WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        };
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                to_wide(HUD_WINDOW_CLASS_NAME).as_ptr(),
                to_wide("Talk HUD").as_ptr(),
                WS_POPUP,
                0,
                0,
                metrics.width,
                metrics.height,
                owner_hwnd,
                ptr::null_mut(),
                instance,
                ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            anyhow::bail!("create Talk desktop HUD window");
        }
        unsafe {
            apply_rounded_window_region(hwnd, metrics.width, metrics.height, metrics.corner_radius);
            ShowWindow(hwnd, SW_HIDE);
        }
        Ok(hwnd)
    }

    fn create_copy_popup_window(instance: HINSTANCE, owner_hwnd: HWND) -> Result<HWND> {
        let metrics =
            scale_copy_popup_metrics_for_dpi(desktop_copy_popup_metrics(), fallback_overlay_dpi());
        let ex_style = match desktop_copy_popup_activation_policy() {
            DesktopOverlayActivationPolicy::NoActivate => {
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
            }
            DesktopOverlayActivationPolicy::ActivateOnInteract => WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        };
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                to_wide(COPY_POPUP_WINDOW_CLASS_NAME).as_ptr(),
                to_wide("Talk Copy Popup").as_ptr(),
                WS_POPUP,
                0,
                0,
                metrics.width,
                metrics.height,
                owner_hwnd,
                ptr::null_mut(),
                instance,
                ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            anyhow::bail!("create Talk desktop copy popup window");
        }
        let edit_hwnd = create_copy_popup_edit_control(
            hwnd,
            fallback_overlay_dpi(),
            COPY_POPUP_EDIT_CONTROL_ID,
        )?;
        unsafe {
            apply_rounded_window_region(
                hwnd,
                metrics.width,
                metrics.height,
                COPY_POPUP_CORNER_RADIUS,
            );
            ShowWindow(hwnd, SW_HIDE);
            ShowWindow(edit_hwnd, SW_HIDE);
        }
        if let Ok(state) = unsafe { get_window_state(owner_hwnd) } {
            state.copy_popup_edit_hwnd.set(edit_hwnd);
            state.copy_popup_pane_edit_hwnds.replace(vec![edit_hwnd]);
        }
        Ok(hwnd)
    }

    fn create_shortcut_help_window(instance: HINSTANCE, owner_hwnd: HWND) -> Result<HWND> {
        let metrics = scale_shortcut_help_metrics_for_dpi(
            desktop_shortcut_help_metrics(),
            fallback_overlay_dpi(),
        );
        let ex_style = match desktop_shortcut_help_activation_policy() {
            DesktopOverlayActivationPolicy::NoActivate => {
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
            }
            DesktopOverlayActivationPolicy::ActivateOnInteract => WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        };
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                to_wide(SHORTCUT_HELP_WINDOW_CLASS_NAME).as_ptr(),
                to_wide("Talk Shortcut Help").as_ptr(),
                WS_POPUP,
                0,
                0,
                metrics.width,
                metrics.height,
                owner_hwnd,
                ptr::null_mut(),
                instance,
                ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            anyhow::bail!("create Talk desktop shortcut help window");
        }
        unsafe {
            apply_rounded_window_region(
                hwnd,
                metrics.width,
                metrics.height,
                SHORTCUT_HELP_CORNER_RADIUS,
            );
            ShowWindow(hwnd, SW_HIDE);
        }
        Ok(hwnd)
    }

    fn copy_popup_edit_control_id(index: usize) -> isize {
        COPY_POPUP_EDIT_CONTROL_ID + index as isize
    }

    fn copy_popup_edit_control_index(control_id: isize) -> Option<usize> {
        let index = control_id.checked_sub(COPY_POPUP_EDIT_CONTROL_ID)? as usize;
        (index < COPY_POPUP_MAX_PANES).then_some(index)
    }

    fn copy_popup_edit_control_style() -> u32 {
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | WS_VSCROLL
            | ES_CENTER as u32
            | ES_MULTILINE as u32
            | ES_AUTOVSCROLL as u32
    }

    fn copy_popup_edit_background_mode() -> i32 {
        OPAQUE as i32
    }

    fn create_copy_popup_edit_control(
        copy_popup_hwnd: HWND,
        dpi: u32,
        control_id: isize,
    ) -> Result<HWND> {
        let metrics = scale_copy_popup_metrics_for_dpi(desktop_copy_popup_metrics(), dpi);
        let editor_rect = copy_popup_editor_content_rect_for_metrics(
            metrics,
            dpi,
            scale_desktop_overlay_length(24, dpi),
        );
        let edit_hwnd = unsafe {
            CreateWindowExW(
                0,
                to_wide(COPY_POPUP_EDIT_CLASS_NAME).as_ptr(),
                to_wide("").as_ptr(),
                copy_popup_edit_control_style(),
                editor_rect.left,
                editor_rect.top,
                editor_rect.right - editor_rect.left,
                editor_rect.bottom - editor_rect.top,
                copy_popup_hwnd,
                control_id as _,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if edit_hwnd.is_null() {
            anyhow::bail!("create Talk desktop copy popup edit control");
        }

        unsafe {
            let _ = SendMessageW(
                edit_hwnd,
                WM_SETFONT,
                GetStockObject(DEFAULT_GUI_FONT) as usize,
                1,
            );
            if SetWindowSubclass(
                edit_hwnd,
                Some(copy_popup_edit_subclass_proc),
                COPY_POPUP_EDIT_SUBCLASS_ID,
                0,
            ) == 0
            {
                let _ = DestroyWindow(edit_hwnd);
                anyhow::bail!("subclass Talk desktop copy popup edit control");
            }
        }
        Ok(edit_hwnd)
    }

    fn register_global_hotkey(hwnd: HWND, hotkey: &HotkeySpec) -> Result<()> {
        let ok = unsafe {
            RegisterHotKey(
                hwnd,
                HOTKEY_ID,
                hotkey.modifier_mask(),
                hotkey.virtual_key(),
            )
        };
        if ok == 0 {
            anyhow::bail!(
                "register Talk desktop hotkey '{}' failed",
                hotkey.trigger_key_name()
            );
        }
        Ok(())
    }

    fn prepare_product_bootstrap() -> PreparedProductBootstrap {
        let data_root = match resolve_talk_data_root() {
            Ok(root) => root,
            Err(error) => return PreparedProductBootstrap::FallbackCloud(error),
        };
        let primary_model_root = data_root.join("models").join("sherpa-onnx");
        let executable_path = match std::env::current_exe() {
            Ok(path) => path,
            Err(error) => {
                return PreparedProductBootstrap::FallbackCloud(format!(
                    "resolve Talk executable for local ASR payload: {error}"
                ));
            }
        };
        let model_root =
            desktop_resolve_product_local_asr_model_root(&executable_path, data_root.as_path());
        let runtime_root = data_root.join("runtime");
        // Fast path: resolve the extracted runtime cache from the payload
        // trailer alone, skipping the full executable read + SHA-256 pass
        // (which can cost seconds for a runtime-embedding product build).
        let cached_runtime_dir = locate_verified_embedded_runtime(&executable_path, &runtime_root);
        let worker_path = if let Some(runtime_dir) = cached_runtime_dir {
            runtime_dir.join(TALK_PACKAGED_LOCAL_ASR_DAEMON_EXE_NAME)
        } else {
            let executable_bytes = match fs::read(&executable_path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return PreparedProductBootstrap::FallbackCloud(format!(
                        "read Talk executable for local ASR payload: {error}"
                    ));
                }
            };
            if !embedded_runtime_payload_is_appended(&executable_bytes) {
                return PreparedProductBootstrap::EngineeringFallback;
            }
            match extract_embedded_runtime_payload(&executable_bytes, &runtime_root) {
                Ok(runtime_dir) => runtime_dir.join(TALK_PACKAGED_LOCAL_ASR_DAEMON_EXE_NAME),
                Err(error) => {
                    return PreparedProductBootstrap::FallbackCloud(format!(
                        "verify embedded Talk local ASR runtime: {error}"
                    ));
                }
            }
        };
        let spec = default_product_local_asr_model_spec();
        let model_dir = model_root.join(&spec.id);
        if validate_installed_model(&spec, &model_dir).is_ok()
            || (model_root != primary_model_root
                && desktop_product_local_asr_model_available(&model_root))
        {
            PreparedProductBootstrap::Ready {
                worker_path,
                model_root,
            }
        } else {
            PreparedProductBootstrap::Download {
                worker_path,
                model_root,
                primary_model_root,
                spec,
            }
        }
    }

    fn commit_product_bootstrap_if_current(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
        worker_path: Option<PathBuf>,
        model_root: Option<PathBuf>,
        status: LocalAsrBootstrapStatus,
    ) -> bool {
        shared.lock().ok().is_some_and(|mut shared_state| {
            if !model_bootstrap_side_effects_allowed(
                shared_state.shutting_down,
                shared_state.product_bootstrap_generation,
                generation,
            ) {
                return false;
            }
            advance_local_asr_daemon_epoch(&mut shared_state);
            if shared_state.pending_product_bootstrap_prepare_generation == Some(generation) {
                shared_state.pending_product_bootstrap_prepare_generation = None;
            }
            shared_state.product_runtime_worker = worker_path;
            shared_state.product_model_root = model_root;
            shared_state.local_asr_bootstrap_status = status;
            true
        })
    }

    fn post_model_bootstrap_status(hwnd_value: usize, generation: u64) -> bool {
        unsafe {
            PostMessageW(
                hwnd_value as HWND,
                MODEL_BOOTSTRAP_MESSAGE,
                generation as usize,
                0,
            ) != 0
        }
    }

    fn start_product_model_download(
        hwnd_value: usize,
        shared: Arc<Mutex<SharedState>>,
        generation: u64,
        spec: ModelSpec,
        primary_model_root: PathBuf,
    ) {
        let runtime_handle = match shared.lock() {
            Ok(shared_state)
                if model_bootstrap_side_effects_allowed(
                    shared_state.shutting_down,
                    shared_state.product_bootstrap_generation,
                    generation,
                ) =>
            {
                shared_state.runtime_handle.clone()
            }
            _ => return,
        };
        let shared_for_task = Arc::clone(&shared);
        let (start_sender, start_receiver) = tokio::sync::oneshot::channel::<()>();
        let task = runtime_handle.spawn(async move {
            if start_receiver.await.is_err() {
                return;
            }
            let status = match std::panic::AssertUnwindSafe(download_and_install_model(
                &spec,
                &primary_model_root,
            ))
            .catch_unwind()
            .await
            {
                Ok(Ok(_)) => LocalAsrBootstrapStatus::Ready,
                Ok(Err(error)) => LocalAsrBootstrapStatus::FallbackCloud(format!(
                    "download local ASR model: {error}"
                )),
                Err(_) => LocalAsrBootstrapStatus::FallbackCloud(
                    "Talk local ASR model download panicked".to_string(),
                ),
            };
            let became_ready = matches!(status, LocalAsrBootstrapStatus::Ready);
            let should_notify = shared_for_task.lock().ok().is_some_and(|mut shared_state| {
                if !model_bootstrap_side_effects_allowed(
                    shared_state.shutting_down,
                    shared_state.product_bootstrap_generation,
                    generation,
                ) {
                    return false;
                }
                advance_local_asr_daemon_epoch(&mut shared_state);
                if matches!(status, LocalAsrBootstrapStatus::Ready) {
                    shared_state.product_model_root = Some(primary_model_root.clone());
                }
                shared_state.local_asr_bootstrap_status = status;
                shared_state.pending_model_bootstrap_task.take();
                true
            });
            if should_notify {
                if became_ready {
                    spawn_product_local_asr_daemon_prewarm(Arc::clone(&shared_for_task));
                }
                let _ = post_model_bootstrap_status(hwnd_value, generation);
            }
        });
        let mut task = Some(task);
        let previous_task = shared.lock().ok().and_then(|mut shared_state| {
            if !model_bootstrap_side_effects_allowed(
                shared_state.shutting_down,
                shared_state.product_bootstrap_generation,
                generation,
            ) {
                return None;
            }
            shared_state
                .pending_model_bootstrap_task
                .replace(task.take().expect("model bootstrap task"))
        });
        if let Some(previous_task) = previous_task {
            previous_task.abort();
        }
        if let Some(task) = task {
            task.abort();
            return;
        }
        if start_sender.send(()).is_err() {
            if let Ok(mut shared_state) = shared.lock() {
                if model_bootstrap_side_effects_allowed(
                    shared_state.shutting_down,
                    shared_state.product_bootstrap_generation,
                    generation,
                ) {
                    shared_state.pending_model_bootstrap_task.take();
                    shared_state.local_asr_bootstrap_status =
                        LocalAsrBootstrapStatus::FallbackCloud(
                            "start Talk local ASR model download task".to_string(),
                        );
                }
            }
        }
        let _ = post_model_bootstrap_status(hwnd_value, generation);
    }

    fn start_product_bootstrap(hwnd: HWND, shared: Arc<Mutex<SharedState>>) {
        let (generation, previous_task) = {
            let mut shared_state = lock_recovering(&shared, "Talk desktop shared state");
            let configured_for_streaming = shared_state
                .config
                .as_ref()
                .map(|config| {
                    desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(
                        config,
                    )) == DesktopSpeculativeLocalAsrRoute::StreamingService
                })
                .unwrap_or(false);
            if shared_state.shutting_down || !configured_for_streaming {
                return;
            }
            if shared_state
                .pending_product_bootstrap_prepare_generation
                .is_some()
            {
                return;
            }
            if shared_state.product_runtime_worker.is_some()
                && shared_state.product_model_root.is_some()
                && matches!(
                    shared_state.local_asr_bootstrap_status,
                    LocalAsrBootstrapStatus::Ready | LocalAsrBootstrapStatus::Downloading
                )
            {
                return;
            }
            shared_state.product_bootstrap_generation =
                shared_state.product_bootstrap_generation.saturating_add(1);
            shared_state.pending_product_bootstrap_prepare_generation =
                Some(shared_state.product_bootstrap_generation);
            shared_state.local_asr_bootstrap_status = LocalAsrBootstrapStatus::NotStarted;
            (
                shared_state.product_bootstrap_generation,
                shared_state.pending_model_bootstrap_task.take(),
            )
        };
        if let Some(previous_task) = previous_task {
            previous_task.abort();
        }

        let hwnd_value = hwnd as usize;
        thread::spawn(move || {
            let prepared =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(prepare_product_bootstrap))
                    .unwrap_or_else(|_| {
                        PreparedProductBootstrap::FallbackCloud(
                            "Talk local ASR runtime preparation panicked".to_string(),
                        )
                    });
            match prepared {
                PreparedProductBootstrap::EngineeringFallback => {
                    if commit_product_bootstrap_if_current(
                        &shared,
                        generation,
                        None,
                        None,
                        LocalAsrBootstrapStatus::EngineeringFallback,
                    ) {
                        let _ = post_model_bootstrap_status(hwnd_value, generation);
                    }
                }
                PreparedProductBootstrap::FallbackCloud(reason) => {
                    if commit_product_bootstrap_if_current(
                        &shared,
                        generation,
                        None,
                        None,
                        LocalAsrBootstrapStatus::FallbackCloud(reason),
                    ) {
                        let _ = post_model_bootstrap_status(hwnd_value, generation);
                    }
                }
                PreparedProductBootstrap::Ready {
                    worker_path,
                    model_root,
                } => {
                    if commit_product_bootstrap_if_current(
                        &shared,
                        generation,
                        Some(worker_path),
                        Some(model_root),
                        LocalAsrBootstrapStatus::Ready,
                    ) {
                        spawn_product_local_asr_daemon_prewarm(Arc::clone(&shared));
                        let _ = post_model_bootstrap_status(hwnd_value, generation);
                    }
                }
                PreparedProductBootstrap::Download {
                    worker_path,
                    model_root,
                    primary_model_root,
                    spec,
                } => {
                    if commit_product_bootstrap_if_current(
                        &shared,
                        generation,
                        Some(worker_path),
                        Some(model_root),
                        LocalAsrBootstrapStatus::Downloading,
                    ) {
                        start_product_model_download(
                            hwnd_value,
                            shared,
                            generation,
                            spec,
                            primary_model_root,
                        );
                    }
                }
            }
        });
    }

    fn handle_model_bootstrap_status(hwnd: HWND, generation: u64) {
        let status = unsafe { get_window_state(hwnd) }.ok().and_then(|state| {
            state.shared.lock().ok().and_then(|mut shared| {
                if !model_bootstrap_side_effects_allowed(
                    shared.shutting_down,
                    shared.product_bootstrap_generation,
                    generation,
                ) {
                    return None;
                }
                if !matches!(
                    shared.local_asr_bootstrap_status,
                    LocalAsrBootstrapStatus::Downloading
                ) {
                    shared.pending_model_bootstrap_task.take();
                }
                Some(shared.local_asr_bootstrap_status.clone())
            })
        });
        match status {
            Some(LocalAsrBootstrapStatus::Ready) => {
                let _ = update_tray_icon(hwnd, "Talk: local ASR ready");
                let _ = show_hud_text(hwnd, "Talk: local ASR ready", Some(1200));
            }
            Some(LocalAsrBootstrapStatus::FallbackCloud(reason)) => {
                let _ = update_tray_icon(hwnd, "Talk: cloud ASR fallback");
                let _ = show_hud_text(
                    hwnd,
                    &format!("Talk: cloud ASR fallback\n{reason}"),
                    Some(2400),
                );
            }
            Some(LocalAsrBootstrapStatus::Downloading) => {
                let _ = update_tray_icon(hwnd, "Talk: downloading local ASR model");
            }
            _ => {}
        }
    }

    fn unregister_global_hotkey(hwnd: HWND) {
        unsafe {
            let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
        }
    }

    fn register_global_hotkey_with_origin_capture(hwnd: HWND, hotkey: &HotkeySpec) -> Result<()> {
        register_global_hotkey(hwnd, hotkey)?;
        if let Err(error) = register_low_level_origin_capture(hwnd, hotkey.clone()) {
            unregister_global_hotkey(hwnd);
            return Err(error);
        }
        Ok(())
    }

    fn install_low_level_hook(
        hook_state: LowLevelHookState,
        failure_message: &'static str,
    ) -> Result<()> {
        unregister_low_level_hotkey();

        {
            let mut state =
                lock_recovering(low_level_hook_state(), "Talk desktop low-level hook state");
            *state = Some(hook_state);
        }

        let module = unsafe { GetModuleHandleW(ptr::null()) };
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0) };
        if hook.is_null() {
            let mut state =
                lock_recovering(low_level_hook_state(), "Talk desktop low-level hook state");
            *state = None;
            anyhow::bail!("{failure_message}");
        }

        let mut handle = lock_recovering(
            low_level_hook_handle(),
            "Talk desktop low-level hook handle",
        );
        *handle = Some(hook as isize);
        Ok(())
    }

    fn register_low_level_origin_capture(hwnd: HWND, hotkey: HotkeySpec) -> Result<()> {
        install_low_level_hook(
            LowLevelHookState::OriginCapture {
                hwnd_value: hwnd as isize,
                tracker: LowLevelHotkeyTracker::new(hotkey),
            },
            "register Talk desktop origin capture hook failed",
        )
    }

    fn register_low_level_hotkey(
        hwnd: HWND,
        trigger_mode: TriggerMode,
        hotkey: HotkeySpec,
    ) -> Result<()> {
        install_low_level_hook(
            LowLevelHookState::Single {
                hwnd_value: hwnd as isize,
                trigger_mode,
                tracker: LowLevelHotkeyTracker::new(hotkey),
            },
            "register Talk desktop low-level keyboard hook failed",
        )
    }

    fn register_low_level_action_router(
        hwnd: HWND,
        bindings: &[DesktopActionBinding],
    ) -> Result<()> {
        install_low_level_hook(
            LowLevelHookState::ToggleRouter {
                hwnd_value: hwnd as isize,
                router: ToggleDesktopHotkeyRouter::new(bindings),
            },
            "register Talk desktop low-level keyboard hook failed",
        )
    }

    fn unregister_low_level_hotkey() {
        if let Some(hook) = lock_recovering(
            low_level_hook_handle(),
            "Talk desktop low-level hook handle",
        )
        .take()
        {
            unsafe {
                let _ = UnhookWindowsHookEx(hook as HHOOK);
            }
        }

        let mut state =
            lock_recovering(low_level_hook_state(), "Talk desktop low-level hook state");
        *state = None;
    }

    fn unregister_bound_hotkey(hwnd: HWND) {
        unregister_global_hotkey(hwnd);
        unregister_low_level_hotkey();
    }

    fn register_bound_hotkey(
        hwnd: HWND,
        trigger_mode: TriggerMode,
        bindings: &[DesktopActionBinding],
    ) -> Result<()> {
        if bindings.len() > 1 {
            return register_low_level_action_router(hwnd, bindings);
        }

        let primary = bindings
            .first()
            .map(|binding| &binding.shortcut)
            .context("Talk desktop bindings must not be empty")?;
        match windows_hotkey_binding_registration_plan(primary) {
            WindowsHotkeyBindingRegistrationPlan::RegisterHotKeyWithOriginCapture => {
                register_global_hotkey_with_origin_capture(hwnd, primary)
            }
            WindowsHotkeyBindingRegistrationPlan::LowLevelHook => {
                register_low_level_hotkey(hwnd, trigger_mode, primary.clone())
            }
        }
    }

    fn initial_hotkey_binding(
        config: &TalkConfig,
    ) -> Result<(Vec<DesktopActionBinding>, HotkeyBindingState), String> {
        let bindings = desktop_action_bindings(config)?;
        let primary_spec = bindings
            .first()
            .map(|binding| binding.shortcut.clone())
            .ok_or_else(|| "Talk desktop bindings must not be empty".to_string())?;
        let shortcut_label = desktop_action_binding_label(&bindings);
        Ok((
            bindings,
            HotkeyBindingState::active_with_label(primary_spec, shortcut_label),
        ))
    }

    fn current_idle_status(shared: &SharedState) -> &'static str {
        if let Some(status) = config_status_message(&shared.config_status) {
            status
        } else {
            hotkey_status_message(&shared.hotkey)
                .or_else(|| native_status_message(shared.native_readiness.as_ref()))
                .unwrap_or("Talk: idle")
        }
    }

    fn current_idle_detail(shared: &SharedState) -> Option<String> {
        idle_status_detail(
            &shared.config_status,
            &shared.hotkey,
            shared.native_readiness.as_ref(),
        )
    }

    fn local_asr_status_snapshot(shared: &SharedState) -> (Option<String>, Option<String>) {
        let configured_for_streaming = shared
            .config
            .as_ref()
            .map(|config| {
                desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(config))
                    == DesktopSpeculativeLocalAsrRoute::StreamingService
            })
            .unwrap_or(false);
        if !configured_for_streaming {
            return (None, None);
        }

        let status = match &shared.local_asr_bootstrap_status {
            LocalAsrBootstrapStatus::NotStarted => "not_started",
            LocalAsrBootstrapStatus::EngineeringFallback => "engineering_fallback",
            LocalAsrBootstrapStatus::Downloading => "downloading",
            LocalAsrBootstrapStatus::Ready => "ready",
            LocalAsrBootstrapStatus::FallbackCloud(_) => "fallback",
        }
        .to_string();

        let model_root_detail = shared
            .product_model_root
            .as_ref()
            .map(|path| format!("using model root {}", path.display()));
        let detail = match &shared.local_asr_bootstrap_status {
            LocalAsrBootstrapStatus::NotStarted => model_root_detail,
            LocalAsrBootstrapStatus::EngineeringFallback => {
                Some("embedded local ASR runtime payload is unavailable".to_string())
            }
            LocalAsrBootstrapStatus::Downloading => model_root_detail
                .map(|detail| format!("{detail}; default local ASR model install is in progress"))
                .or_else(|| Some("default local ASR model install is in progress".to_string())),
            LocalAsrBootstrapStatus::Ready => model_root_detail,
            LocalAsrBootstrapStatus::FallbackCloud(reason) => model_root_detail
                .map(|detail| format!("{detail}; {reason}"))
                .or_else(|| Some(reason.clone())),
        };

        (Some(status), detail)
    }

    fn configured_native_readiness(config: &TalkConfig) -> NativeReadinessSnapshot {
        let audio = match config.audio.backend {
            AudioBackendMode::NativeWindows => {
                let readiness = probe_native_windows_audio_readiness_for_device(
                    config.audio.input_device.as_deref(),
                );
                NativeBackendSnapshot {
                    configured_backend: "native_windows".to_string(),
                    status: Some(readiness.status),
                    detail: if readiness.status == talk_core::NativeReadinessStatus::Ready {
                        format_native_audio_detail(&readiness)
                    } else {
                        readiness.reason
                    },
                }
            }
            AudioBackendMode::Silent => NativeBackendSnapshot {
                configured_backend: "silent".to_string(),
                status: None,
                detail: None,
            },
        };

        let clipboard = match (config.output.mode, config.output.clipboard_backend) {
            (OutputMode::ClipboardPaste, ClipboardBackendMode::NativeWindows) => {
                let readiness = probe_native_windows_clipboard_readiness();
                NativeBackendSnapshot {
                    configured_backend: "native_windows".to_string(),
                    status: Some(readiness.status),
                    detail: if readiness.status == talk_core::NativeReadinessStatus::Ready {
                        Some("Windows clipboard path is callable".to_string())
                    } else {
                        readiness.reason
                    },
                }
            }
            (OutputMode::ClipboardPaste, ClipboardBackendMode::Fallback) => NativeBackendSnapshot {
                configured_backend: "fallback".to_string(),
                status: None,
                detail: None,
            },
            (OutputMode::DryRun, _) => NativeBackendSnapshot {
                configured_backend: "dry_run".to_string(),
                status: None,
                detail: None,
            },
        };

        NativeReadinessSnapshot { audio, clipboard }
    }

    fn format_native_audio_detail(
        readiness: &talk_audio::NativeWindowsAudioReadiness,
    ) -> Option<String> {
        let device_name = readiness.device_name.as_deref()?;
        let sample_rate_hz = readiness.default_sample_rate_hz?;
        let channels = readiness.default_channels?;
        let sample_format = readiness.sample_format.as_deref()?;
        Some(format!(
            "device '{device_name}', {sample_rate_hz} Hz, {channels} ch, {sample_format}"
        ))
    }

    fn desktop_shortcut_label_from_config(config: &TalkConfig) -> String {
        let mut shortcuts = vec![config.trigger.toggle_shortcut.clone()];
        if let Some(transcribe_shortcut) = config.desktop.shortcuts.transcribe_shortcut.as_ref() {
            shortcuts.push(transcribe_shortcut.clone());
        }
        if let Some(document_shortcut) = config.desktop.shortcuts.document_shortcut.as_ref() {
            shortcuts.push(document_shortcut.clone());
        }
        if let Some(command_shortcut) = config.desktop.shortcuts.command_shortcut.as_ref() {
            shortcuts.push(command_shortcut.clone());
        }
        if let Some(generate_shortcut) = config.desktop.shortcuts.generate_shortcut.as_ref() {
            shortcuts.push(generate_shortcut.clone());
        }
        if let Some(smart_shortcut) = config.desktop.shortcuts.smart_shortcut.as_ref() {
            shortcuts.push(smart_shortcut.clone());
        }
        if let Some(translate_shortcut) = config.desktop.shortcuts.translate_shortcut.as_ref() {
            shortcuts.push(translate_shortcut.clone());
        }
        if let Some(ask_shortcut) = config.desktop.shortcuts.ask_shortcut.as_ref() {
            shortcuts.push(ask_shortcut.clone());
        }
        shortcuts.join(" | ")
    }

    fn set_last_session(
        shared: &mut SharedState,
        summary: impl Into<String>,
        detail: Option<String>,
    ) {
        shared.last_session = Some(LastSessionStatus {
            summary: summary.into(),
            detail,
        });
    }

    fn status_snapshot(shared: &SharedState) -> StatusSnapshot {
        let current_summary = match shared.current_phase {
            Some(phase) => hud_message_for_phase(phase).to_string(),
            None => current_idle_status(shared).to_string(),
        };
        let current_detail = if shared.current_phase.is_some() {
            None
        } else {
            current_idle_detail(shared)
        };
        let logs_dir = shared
            .config
            .as_ref()
            .map(|config| resolve_logs_dir(&shared.config_path, &config.logging.dir))
            .unwrap_or_else(|| {
                shared
                    .config_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(".runtime")
                    .join("talk")
                    .join("logs")
            });

        let (local_asr_status, local_asr_detail) = local_asr_status_snapshot(shared);

        StatusSnapshot {
            current_summary,
            current_detail,
            config_path: shared.config_path.display().to_string(),
            logs_dir: logs_dir.display().to_string(),
            hotkey_label: shared
                .hotkey
                .shortcut_label()
                .unwrap_or_else(|| "unconfigured".to_string()),
            hotkey_detail: shared.hotkey.reason().map(str::to_string),
            last_session: shared.last_session.clone(),
            local_asr_status,
            local_asr_detail,
            native_readiness: shared.native_readiness.clone(),
        }
    }

    fn refresh_idle_tray_status(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let shared = lock_recovering(&state.shared, "Talk desktop shared state");
        update_tray_icon(hwnd, current_idle_status(&shared))
    }

    fn spawn_recording_source_cancel_worker(
        runtime_handle: &tokio::runtime::Handle,
        source: ActiveRecordingSource,
        reason: &'static str,
    ) {
        let ActiveRecordingSource::Live {
            recording,
            streaming_pump,
        } = source
        else {
            return;
        };

        runtime_handle.spawn(async move {
            if let Some(streaming_pump) = streaming_pump {
                if let Err(error) = streaming_pump.cancel().await {
                    eprintln!("Talk local streaming ASR cancel failed during {reason}: {error:#}");
                }
            }
            match tokio::task::spawn_blocking(move || recording.cancel()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    eprintln!("Talk recording cleanup failed during {reason}: {error}");
                }
                Err(error) => {
                    eprintln!("Talk recording cleanup worker failed during {reason}: {error}");
                }
            }
        });
    }

    fn cleanup_shutdown_resources(
        active: Option<ActiveRecording>,
        pending: Option<PendingRecordingBegin>,
        config: Option<Arc<TalkConfig>>,
        daemon: Option<ManagedLocalAsrDaemon>,
    ) {
        flush_pending_clipboard_restore();
        if let Some(mut active) = active {
            active.live_streaming_correction_tracker.cancel();
            active.live_streaming_correction_sender.take();
            let ActiveRecording {
                session,
                trigger_events,
                source,
                ..
            } = active;
            if let ActiveRecordingSource::Live {
                recording,
                streaming_pump,
            } = source
            {
                drop(streaming_pump);
                if let Err(error) = recording.cancel() {
                    eprintln!("Talk recording cleanup failed during shutdown: {error}");
                }
            }
            if let Some(config) = config.as_ref() {
                if let Err(error) =
                    complete_cancelled_session(config, session, trigger_events, |_| {})
                {
                    eprintln!(
                        "Talk cancelled-session persistence failed during shutdown: {error:#}"
                    );
                }
            }
        }
        if let Some(mut pending) = pending {
            if let Some(Ok(prepared)) = pending.audio_start_result.take() {
                if let ActiveRecordingSource::Live {
                    recording,
                    streaming_pump,
                } = prepared.source
                {
                    drop(streaming_pump);
                    if let Err(error) = recording.cancel() {
                        eprintln!(
                            "Talk prepared recording cleanup failed during shutdown: {error}"
                        );
                    }
                }
            }
        }
        if let Some(daemon) = daemon {
            stop_managed_local_asr_daemon(daemon);
        }
    }

    fn spawn_cancelled_session_persistence_worker(
        runtime_handle: &tokio::runtime::Handle,
        config: Option<Arc<TalkConfig>>,
        session: VoiceSession,
        trigger_events: Vec<&'static str>,
        reason: &'static str,
    ) {
        runtime_handle.spawn_blocking(move || {
            let Some(config) = config else {
                eprintln!(
                    "Talk cancelled-session persistence skipped during {reason}: config unavailable"
                );
                return;
            };
            if let Err(error) = complete_cancelled_session(&config, session, trigger_events, |_| {})
            {
                eprintln!("Talk cancelled-session persistence failed during {reason}: {error:#}");
            }
        });
    }

    fn spawn_failed_session_persistence_worker(
        runtime_handle: &tokio::runtime::Handle,
        config: Option<Arc<TalkConfig>>,
        session: VoiceSession,
        trigger_events: Vec<&'static str>,
        mode_override: Option<VoiceMode>,
        error: anyhow::Error,
        reason: &'static str,
    ) {
        runtime_handle.spawn_blocking(move || {
            let Some(config) = config else {
                eprintln!(
                    "Talk failed-session persistence skipped during {reason}: config unavailable"
                );
                return;
            };
            if let Err(error) = complete_failed_session_with_mode_override(
                &config,
                session,
                trigger_events,
                mode_override,
                error,
                false,
                |_| {},
            ) {
                eprintln!("Talk failed-session persistence failed during {reason}: {error:#}");
            }
        });
    }

    fn set_failed_last_session_if_current(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
        error_message: String,
    ) {
        if let Ok(mut shared) = shared.lock() {
            if !shared.shutting_down
                && generation
                    .checked_add(1)
                    .is_some_and(|next| shared.next_generation == next)
            {
                set_last_session(&mut shared, "failed", Some(error_message));
            }
        }
    }

    fn cancel_active_recording(hwnd: HWND) -> Result<()> {
        if cancel_pending_recording_begin(hwnd, None)? {
            return Ok(());
        }
        let state = unsafe { get_window_state(hwnd)? };
        let (config, runtime_handle, mut active) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            let Some(active) = shared.active_recording.take() else {
                return Ok(());
            };
            shared.shell_state = shared.shell_state.complete();
            shared.current_phase = None;
            (shared.config.clone(), shared.runtime_handle.clone(), active)
        };
        cancel_recording_timeout_timer(hwnd);

        active.live_streaming_correction_tracker.cancel();
        active.live_streaming_correction_sender.take();

        let ActiveRecording {
            session,
            trigger_events,
            source,
            ..
        } = active;
        spawn_recording_source_cancel_worker(&runtime_handle, source, "user cancellation");
        spawn_cancelled_session_persistence_worker(
            &runtime_handle,
            config,
            session,
            trigger_events,
            "user cancellation",
        );
        {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            set_last_session(
                &mut shared,
                "cancelled",
                Some("user cancelled during recording".to_string()),
            );
        }
        update_tray_icon(hwnd, "Talk: cancelled")?;
        hide_hud(hwnd)?;
        refresh_idle_tray_status(hwnd)?;
        Ok(())
    }

    fn fail_active_recording(
        hwnd: HWND,
        state: &WindowState,
        error_message: String,
        copy_text: Option<String>,
    ) -> Result<()> {
        let (config, runtime_handle, mut active) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            let Some(active) = shared.active_recording.take() else {
                return Ok(());
            };
            shared.shell_state = shared.shell_state.complete();
            shared.current_phase = None;
            shared.worker_generation = None;
            (shared.config.clone(), shared.runtime_handle.clone(), active)
        };
        cancel_recording_timeout_timer(hwnd);
        let generation = active.generation;

        active.live_streaming_correction_tracker.cancel();
        active.live_streaming_correction_sender.take();

        let ActiveRecording {
            session,
            trigger_events,
            source,
            ..
        } = active;
        spawn_recording_source_cancel_worker(&runtime_handle, source, "recording failure");

        let copy_text = copy_text.filter(|text| !text.trim().is_empty());
        let persistence_scheduled = config.is_some();
        spawn_failed_session_persistence_worker(
            &runtime_handle,
            config,
            session,
            trigger_events,
            None,
            anyhow::anyhow!(error_message.clone()),
            "recording failure",
        );
        let cleanup_plan = desktop_failure_cleanup_plan(persistence_scheduled, copy_text.is_some());
        {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            set_last_session(&mut shared, "failed", Some(error_message));
            if let Some(copy_text) = copy_text.as_deref() {
                shared.pending_copy_popup = Some(PendingCopyPopup {
                    generation,
                    model: desktop_copy_popup_model(copy_text),
                });
            }
        }

        if cleanup_plan.stop_recording_timer {
            unsafe {
                KillTimer(hwnd, TIMER_RECORDING_LEVEL);
            }
        }
        if let Err(error) = update_tray_icon(hwnd, "Talk: failed") {
            eprintln!("Talk failed-state tray update failed: {error:#}");
        }
        if cleanup_plan.hide_hud {
            if let Err(error) = hide_hud(hwnd) {
                eprintln!("Talk failed-state HUD cleanup failed: {error:#}");
            }
        }
        if cleanup_plan.show_copy_popup {
            unsafe {
                let _ = PostMessageW(hwnd, CORRECTION_COPY_POPUP_MESSAGE, generation as usize, 0);
            }
        }
        Ok(())
    }

    fn register_or_mark_hotkey_failure(hwnd: HWND, shared: &mut SharedState) {
        unregister_bound_hotkey(hwnd);
        if !shared.config_status.is_ready() {
            shared.hotkey = HotkeyBindingState::Unconfigured;
            return;
        }
        let Some(config) = shared.config.as_ref() else {
            shared.hotkey = HotkeyBindingState::Unconfigured;
            return;
        };
        let Some(primary_spec) = shared.hotkey.spec().cloned() else {
            return;
        };
        let shortcut_label = desktop_action_binding_label(&shared.desktop_actions);
        shared.hotkey =
            match register_bound_hotkey(hwnd, config.trigger.mode, &shared.desktop_actions) {
                Ok(()) => HotkeyBindingState::active_with_label(primary_spec, shortcut_label),
                Err(error) => {
                    HotkeyBindingState::registration_failed(shortcut_label, error.to_string())
                }
            };
    }

    fn update_tray_icon(hwnd: HWND, tooltip: &str) -> Result<()> {
        let mut data = unsafe { zeroed_notify_data(hwnd) };
        data.uFlags = NIF_MESSAGE | NIF_TIP | NIF_ICON;
        data.uCallbackMessage = TRAY_MESSAGE;
        data.hIcon = unsafe { LoadIconW(ptr::null_mut(), IDI_APPLICATION) };
        write_wide_fixed(tooltip, &mut data.szTip);

        let message = unsafe {
            if tray_icon_exists(hwnd) {
                NIM_MODIFY
            } else {
                NIM_ADD
            }
        };
        let ok = unsafe { Shell_NotifyIconW(message, &data) };
        if ok == 0 {
            anyhow::bail!("update Talk desktop tray icon");
        }
        Ok(())
    }

    unsafe fn tray_icon_exists(hwnd: HWND) -> bool {
        let mut data = zeroed_notify_data(hwnd);
        data.uFlags = NIF_TIP;
        Shell_NotifyIconW(NIM_MODIFY, &data) != 0
    }

    unsafe fn zeroed_notify_data(hwnd: HWND) -> NOTIFYICONDATAW {
        let mut data: NOTIFYICONDATAW = mem::zeroed();
        data.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ICON_ID;
        data
    }

    fn remove_tray_icon(hwnd: HWND) {
        let data = unsafe { zeroed_notify_data(hwnd) };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
    }

    /// Returns the Talk window state stored in `GWLP_USERDATA`.
    ///
    /// # Safety
    /// `hwnd` must still own the `WindowState` pointer installed at window
    /// creation. Mutable fields use interior mutability so nested lookups on
    /// the UI thread do not create aliased `&mut WindowState` references.
    unsafe fn get_window_state(hwnd: HWND) -> Result<&'static WindowState> {
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if pointer.is_null() {
            anyhow::bail!("Talk desktop window state is unavailable");
        }
        Ok(&*pointer)
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCCREATE => {
                let create_struct = &*(lparam as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create_struct.lpCreateParams as isize);
                1
            }
            WM_HOTKEY => {
                if wparam as i32 == HOTKEY_ID {
                    handle_desktop_action(hwnd, 0);
                    return 0;
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_COMMAND => {
                handle_menu_command(hwnd, loword(wparam));
                0
            }
            WM_TIMER => {
                if wparam == TIMER_HIDE_HUD {
                    let _ = hide_hud(hwnd);
                    return 0;
                }
                if wparam == TIMER_SHORTCUT_HELP_HOLD {
                    let _ = maybe_show_pending_shortcut_help(hwnd);
                    return 0;
                }
                if wparam == TIMER_RECORDING_LEVEL {
                    let _ = refresh_recording_hud_level(hwnd);
                    return 0;
                }
                if wparam == TIMER_THINKING_PROGRESS {
                    let _ = refresh_thinking_hud_progress(hwnd);
                    return 0;
                }
                if wparam == TIMER_RECORDING_TIMEOUT {
                    handle_recording_timeout_timer(hwnd);
                    return 0;
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            TRAY_MESSAGE => {
                if lparam as u32 == WM_RBUTTONUP {
                    let _ = show_tray_menu(hwnd);
                    return 0;
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            STOP_MESSAGE => {
                request_stop_recording(hwnd, wparam as u64);
                0
            }
            LOW_LEVEL_HOTKEY_RELEASE_MESSAGE => {
                let _ = cancel_pending_shortcut_help(hwnd);
                handle_low_level_hotkey_release(hwnd);
                0
            }
            HOTKEY_PENDING_HOLD_START_MESSAGE => {
                let _ = schedule_pending_shortcut_help(hwnd);
                0
            }
            HOTKEY_PENDING_HOLD_CANCEL_MESSAGE => {
                let _ = cancel_pending_shortcut_help(hwnd);
                0
            }
            HOTKEY_ACTION_MESSAGE => {
                let _ = cancel_pending_shortcut_help(hwnd);
                handle_desktop_action(hwnd, wparam);
                0
            }
            PHASE_MESSAGE => {
                apply_runtime_phase(hwnd, runtime_phase_from_code(wparam as u32), lparam as u64);
                0
            }
            WORKER_DONE_MESSAGE => {
                handle_worker_done(hwnd, wparam as u64);
                0
            }
            MODEL_BOOTSTRAP_MESSAGE => {
                handle_model_bootstrap_status(hwnd, wparam as u64);
                0
            }
            LOCAL_ASR_PREPARE_DONE_MESSAGE => {
                handle_local_asr_prepare_done(hwnd, wparam as u64);
                0
            }
            AUDIO_START_DONE_MESSAGE => {
                handle_recording_audio_start_done(hwnd, wparam as u64);
                0
            }
            CONFIG_RELOAD_DONE_MESSAGE => {
                handle_config_reload_done(hwnd, wparam as u64);
                0
            }
            CORRECTION_COPY_POPUP_MESSAGE => {
                handle_correction_copy_popup(hwnd, wparam as u64);
                0
            }
            CORRECTED_HUD_MESSAGE => {
                handle_corrected_hud(hwnd, wparam as u64);
                0
            }
            STREAMING_CORRECTED_HUD_MESSAGE => {
                handle_streaming_corrected_hud(hwnd, wparam as u64);
                0
            }
            STREAMING_PUMP_DONE_MESSAGE => {
                let _ = refresh_recording_hud_level(hwnd);
                0
            }
            WM_DESTROY => {
                unsafe {
                    KillTimer(hwnd, TIMER_SHORTCUT_HELP_HOLD);
                    KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                    KillTimer(hwnd, TIMER_THINKING_PROGRESS);
                }
                if let Ok(state) = get_window_state(hwnd) {
                    let foreground_apply_gate = state
                        .shared
                        .lock()
                        .ok()
                        .map(|shared| shared.foreground_apply_gate.clone());
                    let foreground_apply_lease = foreground_apply_gate
                        .as_ref()
                        .map(ForegroundApplyGate::acquire);
                    let shutdown_state = state.shared.lock().ok().map(|mut shared| {
                        shared.shutting_down = true;
                        shared.next_generation = shared.next_generation.saturating_add(1);
                        shared.product_bootstrap_generation =
                            shared.product_bootstrap_generation.saturating_add(1);
                        shared.pending_product_bootstrap_prepare_generation = None;
                        shared.config_reload_generation =
                            shared.config_reload_generation.saturating_add(1);
                        shared.pending_config_reload = None;
                        advance_local_asr_daemon_epoch(&mut shared);
                        let pending_recording_begin = shared.pending_recording_begin.take();
                        shared.worker_generation = None;
                        (
                            shared.active_recording.take(),
                            pending_recording_begin,
                            shared.local_asr_daemon.take(),
                            shared.runtime_handle.clone(),
                            shared.config.clone(),
                            shared.pending_document_correction_task.take(),
                            shared.pending_worker_task.take(),
                            shared.pending_stop_live_correction_tracker.take(),
                            shared.pending_model_bootstrap_task.take(),
                        )
                    });
                    drop(foreground_apply_lease);
                    if let Some((
                        active,
                        pending,
                        daemon,
                        runtime_handle,
                        config,
                        mut pending_correction,
                        mut pending_worker,
                        stop_tracker,
                        mut pending_model_bootstrap,
                    )) = shutdown_state
                    {
                        if let Some(tracker) = stop_tracker {
                            tracker.cancel();
                        }
                        abort_pending_task(&mut pending_correction);
                        abort_pending_task(&mut pending_worker);
                        abort_pending_join_handle(&mut pending_model_bootstrap);
                        runtime_handle.spawn_blocking(move || {
                            cleanup_shutdown_resources(active, pending, config, daemon);
                        });
                    }
                    unregister_bound_hotkey(hwnd);
                    if !state.hud_hwnd.get().is_null() {
                        let _ = DestroyWindow(state.hud_hwnd.get());
                    }
                    if !state.copy_popup_hwnd.get().is_null() {
                        let _ = DestroyWindow(state.copy_popup_hwnd.get());
                        state.copy_popup_hwnd.set(ptr::null_mut());
                        state.copy_popup_edit_hwnd.set(ptr::null_mut());
                        state.copy_popup_pane_edit_hwnds.borrow_mut().clear();
                    }
                    if !state.shortcut_help_hwnd.get().is_null() {
                        let _ = DestroyWindow(state.shortcut_help_hwnd.get());
                    }
                }
                remove_tray_icon(hwnd);
                PostQuitMessage(0);
                0
            }
            WM_NCDESTROY => {
                let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                if !pointer.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    drop(Box::from_raw(pointer));
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    unsafe extern "system" fn hud_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_PAINT => {
                paint_hud_window(hwnd);
                0
            }
            WM_LBUTTONDOWN => {
                handle_listening_hud_mouse_down(hwnd, point_from_lparam(lparam));
                0
            }
            WM_MOUSEMOVE => {
                handle_listening_hud_mouse_move(hwnd, point_from_lparam(lparam));
                0
            }
            WM_MOUSEWHEEL => {
                handle_listening_hud_mouse_wheel(hwnd, wheel_delta_from_wparam(wparam));
                0
            }
            WM_LBUTTONUP => {
                handle_listening_hud_mouse_up(hwnd, point_from_lparam(lparam));
                0
            }
            WM_CAPTURECHANGED => {
                if let Ok(mut overlay) = overlay_ui_state().lock() {
                    overlay.hud_streaming_scroll_dragging = false;
                }
                0
            }
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    fn copy_popup_keyboard_tab_target(
        current: CopyPopupHoveredControl,
        reverse: bool,
    ) -> CopyPopupHoveredControl {
        match (current, reverse) {
            (CopyPopupHoveredControl::None, false) => CopyPopupHoveredControl::Copy,
            (CopyPopupHoveredControl::Copy, false) => CopyPopupHoveredControl::Close,
            (CopyPopupHoveredControl::Close, false) => CopyPopupHoveredControl::None,
            (CopyPopupHoveredControl::None, true) => CopyPopupHoveredControl::Close,
            (CopyPopupHoveredControl::Close, true) => CopyPopupHoveredControl::Copy,
            (CopyPopupHoveredControl::Copy, true) => CopyPopupHoveredControl::None,
        }
    }

    fn copy_popup_keyboard_focused_control() -> CopyPopupHoveredControl {
        overlay_ui_state()
            .lock()
            .ok()
            .and_then(|overlay| {
                overlay
                    .copy_popup
                    .as_ref()
                    .map(|popup| popup.keyboard_focused_control)
            })
            .unwrap_or_default()
    }

    fn focus_copy_popup_keyboard_target(
        owner_hwnd: HWND,
        copy_popup_hwnd: HWND,
        target: CopyPopupHoveredControl,
    ) {
        let mut should_invalidate = false;
        if let Ok(mut overlay) = overlay_ui_state().lock() {
            if let Some(popup) = overlay.copy_popup.as_mut() {
                if popup.keyboard_focused_control != target {
                    popup.keyboard_focused_control = target;
                    should_invalidate = true;
                }
            }
        }

        let edit_hwnd = unsafe { get_window_state(owner_hwnd) }
            .ok()
            .map(|state| state.copy_popup_edit_hwnd.get())
            .unwrap_or(ptr::null_mut());
        unsafe {
            if should_invalidate {
                InvalidateRect(copy_popup_hwnd, ptr::null(), 0);
            }
            if target == CopyPopupHoveredControl::None && !edit_hwnd.is_null() {
                let _ = SetFocus(edit_hwnd);
            } else if target != CopyPopupHoveredControl::None {
                let _ = SetFocus(copy_popup_hwnd);
            }
        }
    }

    fn clear_copy_popup_keyboard_focus(copy_popup_hwnd: HWND) {
        let mut should_invalidate = false;
        if let Ok(mut overlay) = overlay_ui_state().lock() {
            if let Some(popup) = overlay.copy_popup.as_mut() {
                if popup.keyboard_focused_control != CopyPopupHoveredControl::None {
                    popup.keyboard_focused_control = CopyPopupHoveredControl::None;
                    should_invalidate = true;
                }
            }
        }
        unsafe {
            if should_invalidate {
                InvalidateRect(copy_popup_hwnd, ptr::null(), 0);
            }
        }
    }

    fn handle_copy_popup_virtual_key(
        owner_hwnd: HWND,
        _copy_popup_hwnd: HWND,
        virtual_key: u32,
    ) -> bool {
        match desktop_copy_popup_action_for_virtual_key(virtual_key) {
            DesktopCopyPopupAction::CopyToClipboard => {
                if copy_popup_keyboard_focused_control() == CopyPopupHoveredControl::Close {
                    let _ = hide_copy_popup(owner_hwnd);
                } else {
                    let _ = copy_popup_text_to_clipboard(owner_hwnd);
                }
                true
            }
            DesktopCopyPopupAction::Close => {
                let _ = hide_copy_popup(owner_hwnd);
                true
            }
            DesktopCopyPopupAction::Ignore => false,
        }
    }

    unsafe extern "system" fn copy_popup_edit_subclass_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        _reference_data: usize,
    ) -> LRESULT {
        match message {
            WM_CHAR | WM_SYSCHAR if matches!(wparam as u32, VK_TAB_KEY | 0x0D | 0x1B) => {
                return 0;
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let copy_popup_hwnd = GetParent(hwnd);
                if let Some(owner_hwnd) = copy_popup_owner(copy_popup_hwnd) {
                    if wparam as u32 == VK_TAB_KEY {
                        let reverse = GetAsyncKeyState(VK_SHIFT as i32) < 0;
                        let target =
                            copy_popup_keyboard_tab_target(CopyPopupHoveredControl::None, reverse);
                        focus_copy_popup_keyboard_target(owner_hwnd, copy_popup_hwnd, target);
                        return 0;
                    }
                    if handle_copy_popup_virtual_key(owner_hwnd, copy_popup_hwnd, wparam as u32) {
                        return 0;
                    }
                }
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(
                    hwnd,
                    Some(copy_popup_edit_subclass_proc),
                    COPY_POPUP_EDIT_SUBCLASS_ID,
                );
            }
            _ => {}
        }

        DefSubclassProc(hwnd, message, wparam, lparam)
    }

    unsafe extern "system" fn copy_popup_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_PAINT => {
                paint_copy_popup_window(hwnd);
                0
            }
            WM_LBUTTONDOWN => {
                clear_copy_popup_keyboard_focus(hwnd);
                let dpi = overlay_dpi_for_window(hwnd);
                let pressed =
                    copy_popup_hovered_control_for_point(hwnd, dpi, point_from_lparam(lparam));
                set_copy_popup_pressed_control(hwnd, pressed);
                if pressed != CopyPopupHoveredControl::None {
                    SetCapture(hwnd);
                }
                0
            }
            WM_MOUSEMOVE => {
                track_copy_popup_mouse_leave(hwnd);
                refresh_copy_popup_hover(hwnd, point_from_lparam(lparam));
                0
            }
            WM_MOUSELEAVE => {
                clear_copy_popup_hover(hwnd);
                0
            }
            WM_LBUTTONUP => {
                clear_copy_popup_pressed_control(hwnd);
                ReleaseCapture();
                if let Some(owner_hwnd) = copy_popup_owner(hwnd) {
                    let click = point_from_lparam(lparam);
                    let dpi = overlay_dpi_for_window(hwnd);
                    if point_in_rect(click, copy_popup_copy_button_rect(hwnd, dpi)) {
                        let _ = copy_popup_text_to_clipboard(owner_hwnd);
                    } else if point_in_rect(click, copy_popup_close_button_rect(hwnd, dpi)) {
                        let _ = hide_copy_popup(owner_hwnd);
                    } else if point_in_rect(click, copy_popup_editor_frame_rect(hwnd, dpi)) {
                        clear_copy_popup_hover(hwnd);
                        let _ = activate_copy_popup_for_interaction(owner_hwnd);
                    }
                }
                0
            }
            WM_CAPTURECHANGED => {
                clear_copy_popup_pressed_control(hwnd);
                0
            }
            WM_COMMAND => {
                let control_id = (wparam & 0xFFFF) as isize;
                let notify_code = ((wparam >> 16) & 0xFFFF) as u32;
                if copy_popup_edit_control_index(control_id).is_some() && notify_code == EN_CHANGE {
                    if let Some(owner_hwnd) = copy_popup_owner(hwnd) {
                        let _ = update_copy_popup_edit_layout(owner_hwnd, hwnd);
                        InvalidateRect(hwnd, ptr::null(), 0);
                    }
                }
                0
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(owner_hwnd) = copy_popup_owner(hwnd) {
                    if wparam as u32 == VK_TAB_KEY {
                        let reverse = GetAsyncKeyState(VK_SHIFT as i32) < 0;
                        let target = copy_popup_keyboard_tab_target(
                            copy_popup_keyboard_focused_control(),
                            reverse,
                        );
                        focus_copy_popup_keyboard_target(owner_hwnd, hwnd, target);
                    } else {
                        let _ = handle_copy_popup_virtual_key(owner_hwnd, hwnd, wparam as u32);
                    }
                }
                0
            }
            WM_KILLFOCUS => {
                clear_copy_popup_keyboard_focus(hwnd);
                0
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
                let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
                SetTextColor(hdc, typeless_popup_editor_text_color());
                SetBkColor(hdc, typeless_popup_editor_fill_color());
                SetBkMode(hdc, copy_popup_edit_background_mode());
                copy_popup_edit_brush()
            }
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    unsafe extern "system" fn shortcut_help_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_PAINT => {
                paint_shortcut_help_window(hwnd);
                0
            }
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    fn handle_desktop_action(hwnd: HWND, action_index: usize) {
        let state = match unsafe { get_window_state(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let (
            can_start,
            can_stop,
            active_generation,
            active_action_index,
            pending_generation,
            pending_action_index,
        ) = {
            let shared = lock_recovering(&state.shared, "Talk desktop shared state");
            (
                shared.shell_state.can_start_session(),
                shared.shell_state.can_stop_session(),
                shared
                    .active_recording
                    .as_ref()
                    .map(|active| active.generation),
                shared
                    .active_recording
                    .as_ref()
                    .map(|active| active.action_index),
                shared
                    .pending_recording_begin
                    .as_ref()
                    .map(|pending| pending.generation),
                shared
                    .pending_recording_begin
                    .as_ref()
                    .map(|pending| pending.action_index),
            )
        };

        if can_start {
            if let Err(error) = begin_recording(hwnd, state, ActivationSource::Hotkey, action_index)
            {
                let _ = show_hud_text(
                    hwnd,
                    &compose_hud_message("Talk: unavailable", Some(&error.to_string())),
                    Some(1800),
                );
            }
        } else if let Some(generation) =
            pending_generation.filter(|_| pending_action_index == Some(action_index))
        {
            let _ = cancel_pending_recording_begin(hwnd, Some(generation));
        } else if can_stop {
            if let Some(generation) =
                active_generation.filter(|_| active_action_index == Some(action_index))
            {
                request_stop_recording(hwnd, generation);
            } else {
                let _ = show_hud_text(hwnd, "Talk: busy", Some(900));
            }
        } else {
            let _ = show_hud_text(hwnd, "Talk: busy", Some(900));
        }
    }

    fn handle_low_level_hotkey_release(hwnd: HWND) {
        let generation = unsafe { get_window_state(hwnd) }.ok().and_then(|state| {
            state.shared.lock().ok().and_then(|shared| {
                shared
                    .active_recording
                    .as_ref()
                    .map(|active| active.generation)
                    .or_else(|| {
                        shared
                            .pending_recording_begin
                            .as_ref()
                            .map(|pending| pending.generation)
                    })
            })
        });

        if let Some(generation) = generation {
            request_stop_recording(hwnd, generation);
        }
    }

    fn local_asr_bootstrap_allows_packaged_daemon(
        status: &LocalAsrBootstrapStatus,
        has_explicit_model: bool,
        has_installed_model: bool,
    ) -> bool {
        match status {
            LocalAsrBootstrapStatus::Downloading
            | LocalAsrBootstrapStatus::NotStarted
            | LocalAsrBootstrapStatus::FallbackCloud(_) => {
                has_explicit_model || has_installed_model
            }
            LocalAsrBootstrapStatus::EngineeringFallback | LocalAsrBootstrapStatus::Ready => true,
        }
    }

    fn recording_streaming_unavailable_hint(
        configured_streaming_speculative_asr: bool,
        use_streaming_speculative_asr: bool,
        status: &LocalAsrBootstrapStatus,
    ) -> Option<&'static str> {
        if !configured_streaming_speculative_asr || use_streaming_speculative_asr {
            return None;
        }

        match status {
            LocalAsrBootstrapStatus::Downloading => Some("downloading local ASR model..."),
            LocalAsrBootstrapStatus::NotStarted => Some("preparing local ASR..."),
            LocalAsrBootstrapStatus::FallbackCloud(_)
            | LocalAsrBootstrapStatus::EngineeringFallback => Some("live transcript unavailable"),
            LocalAsrBootstrapStatus::Ready => None,
        }
    }

    fn recording_streaming_preflight_message(
        configured_streaming_speculative_asr: bool,
    ) -> Option<(&'static str, &'static str)> {
        configured_streaming_speculative_asr.then_some((
            "Talk: preparing local ASR",
            "starting local live transcript runtime...",
        ))
    }

    struct ProductLocalAsrDaemonPrewarmRequest {
        endpoint: String,
        startup_timeout_ms: u64,
        plan: DesktopLocalAsrDaemonLaunchPlan,
    }

    fn product_local_asr_daemon_prewarm_request(
        config: &TalkConfig,
        status: &LocalAsrBootstrapStatus,
        worker_path: Option<&Path>,
        model_root: Option<&Path>,
    ) -> Option<ProductLocalAsrDaemonPrewarmRequest> {
        let service = config.speculative.streaming_service.as_ref()?;
        if desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(config))
            != DesktopSpeculativeLocalAsrRoute::StreamingService
        {
            return None;
        }

        let has_explicit_model = service.local_daemon.is_some();
        let has_installed_model = model_root.is_some_and(desktop_product_local_asr_model_available);
        if !local_asr_bootstrap_allows_packaged_daemon(
            status,
            has_explicit_model,
            has_installed_model,
        ) {
            return None;
        }

        let worker_path = worker_path?;
        let model_root = model_root?;
        let plan = desktop_product_local_asr_daemon_launch_plan_with_config(
            worker_path,
            model_root,
            &service.endpoint,
            service.local_daemon.as_ref(),
        )
        .ok()
        .flatten()?;

        Some(ProductLocalAsrDaemonPrewarmRequest {
            endpoint: service.endpoint.clone(),
            startup_timeout_ms: desktop_product_local_asr_startup_timeout_ms(
                service.connect_timeout_ms,
            ),
            plan,
        })
    }

    fn start_managed_local_asr_daemon(
        plan: &DesktopLocalAsrDaemonLaunchPlan,
        endpoint: String,
        startup_timeout_ms: u64,
    ) -> Result<ManagedLocalAsrDaemon> {
        let mut child = std::process::Command::new(&plan.executable_path)
            .args(&plan.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW_FLAG)
            .spawn()
            .with_context(|| {
                format!(
                    "start packaged local ASR daemon {}",
                    plan.executable_path.display()
                )
            })?;

        let deadline = Instant::now() + Duration::from_millis(startup_timeout_ms);
        loop {
            if local_asr_endpoint_accepts_tcp(&endpoint, Duration::from_millis(40)) {
                return Ok(ManagedLocalAsrDaemon {
                    endpoint,
                    launch_plan: plan.clone(),
                    child,
                });
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    anyhow::bail!("packaged local ASR daemon exited before readiness: {status}");
                }
                Ok(None) => {}
                Err(error) => {
                    if let Err(kill_error) = child.kill() {
                        eprintln!(
                            "Talk packaged local ASR daemon cleanup kill failed after status error: {kill_error}"
                        );
                    }
                    if let Err(wait_error) = child.wait() {
                        eprintln!(
                            "Talk packaged local ASR daemon cleanup wait failed after status error: {wait_error}"
                        );
                    }
                    anyhow::bail!("check packaged local ASR daemon status: {error}");
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!(
                    "packaged local ASR daemon did not become ready within {startup_timeout_ms} ms"
                );
            }
            thread::sleep(Duration::from_millis(40));
        }
    }

    fn packaged_local_asr_daemon_request(
        config: &TalkConfig,
        status: &LocalAsrBootstrapStatus,
        worker_path: Option<&Path>,
        model_root: Option<&Path>,
    ) -> Result<Option<ProductLocalAsrDaemonPrewarmRequest>> {
        let Some(service) = config.speculative.streaming_service.as_ref() else {
            return Ok(None);
        };
        if desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(config))
            != DesktopSpeculativeLocalAsrRoute::StreamingService
        {
            return Ok(None);
        }
        let endpoint = service.endpoint.clone();

        let has_explicit_model = service.local_daemon.is_some();
        let has_installed_model = model_root.is_some_and(desktop_product_local_asr_model_available);
        if !local_asr_bootstrap_allows_packaged_daemon(
            status,
            has_explicit_model,
            has_installed_model,
        ) {
            return Ok(None);
        }

        let plan = if let (Some(worker_path), Some(model_root)) = (worker_path, model_root) {
            desktop_product_local_asr_daemon_launch_plan_with_config(
                worker_path,
                model_root,
                &endpoint,
                service.local_daemon.as_ref(),
            )
            .map_err(anyhow::Error::msg)?
        } else {
            let executable_path =
                std::env::current_exe().context("resolve Talk desktop executable path")?;
            desktop_packaged_local_asr_daemon_launch_plan_with_config(
                &executable_path,
                &endpoint,
                service.local_daemon.as_ref(),
            )
            .map_err(anyhow::Error::msg)?
        };
        Ok(plan.map(|plan| ProductLocalAsrDaemonPrewarmRequest {
            endpoint,
            startup_timeout_ms: desktop_product_local_asr_startup_timeout_ms(
                service.connect_timeout_ms,
            ),
            plan,
        }))
    }

    fn local_asr_daemon_operation_allowed(
        shutting_down: bool,
        current_epoch: u64,
        operation_epoch: u64,
    ) -> bool {
        !shutting_down && current_epoch == operation_epoch
    }

    fn advance_local_asr_daemon_epoch(shared: &mut SharedState) -> u64 {
        shared.local_asr_daemon_epoch = shared.local_asr_daemon_epoch.saturating_add(1);
        shared.local_asr_daemon_epoch
    }

    fn local_asr_daemon_config_matches(current: &TalkConfig, requested: &TalkConfig) -> bool {
        current.speculative.enabled == requested.speculative.enabled
            && current.speculative.local_asr == requested.speculative.local_asr
            && current.speculative.streaming_service == requested.speculative.streaming_service
    }

    fn local_asr_daemon_operation_is_current(
        shared: &Arc<Mutex<SharedState>>,
        operation_epoch: u64,
    ) -> bool {
        shared.lock().ok().is_some_and(|shared_state| {
            local_asr_daemon_operation_allowed(
                shared_state.shutting_down,
                shared_state.local_asr_daemon_epoch,
                operation_epoch,
            )
        })
    }

    fn reserve_local_asr_daemon_operation(
        shared: &Arc<Mutex<SharedState>>,
        observed_epoch: u64,
    ) -> Result<Option<(u64, Option<ManagedLocalAsrDaemon>)>> {
        let mut shared_state = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Talk desktop shared state is poisoned"))?;
        if !local_asr_daemon_operation_allowed(
            shared_state.shutting_down,
            shared_state.local_asr_daemon_epoch,
            observed_epoch,
        ) {
            return Ok(None);
        }
        let operation_epoch = advance_local_asr_daemon_epoch(&mut shared_state);
        Ok(Some((
            operation_epoch,
            shared_state.local_asr_daemon.take(),
        )))
    }

    fn install_managed_local_asr_daemon_if_current(
        shared: &Arc<Mutex<SharedState>>,
        operation_epoch: u64,
        daemon: ManagedLocalAsrDaemon,
    ) -> bool {
        let mut candidate = Some(daemon);
        let displaced = shared.lock().ok().and_then(|mut shared_state| {
            if !local_asr_daemon_operation_allowed(
                shared_state.shutting_down,
                shared_state.local_asr_daemon_epoch,
                operation_epoch,
            ) {
                return None;
            }
            let displaced = shared_state.local_asr_daemon.take();
            shared_state.local_asr_daemon = candidate.take();
            displaced
        });
        if let Some(displaced) = displaced {
            stop_managed_local_asr_daemon(displaced);
        }
        if let Some(stale) = candidate {
            stop_managed_local_asr_daemon(stale);
            false
        } else {
            true
        }
    }

    fn reconcile_managed_local_asr_daemon(
        shared: &Arc<Mutex<SharedState>>,
        operation_epoch: u64,
        request: &ProductLocalAsrDaemonPrewarmRequest,
        existing: Option<ManagedLocalAsrDaemon>,
    ) -> Result<bool> {
        if !local_asr_daemon_operation_is_current(shared, operation_epoch) {
            if let Some(existing) = existing {
                stop_managed_local_asr_daemon(existing);
            }
            return Ok(false);
        }

        if let Some(mut daemon) = existing {
            if managed_local_asr_daemon_launch_is_current(
                &daemon.endpoint,
                &daemon.launch_plan,
                &request.endpoint,
                &request.plan,
            ) && managed_local_asr_daemon_is_running(&mut daemon)
            {
                return Ok(install_managed_local_asr_daemon_if_current(
                    shared,
                    operation_epoch,
                    daemon,
                ));
            }
            stop_managed_local_asr_daemon(daemon);
        }

        if !local_asr_daemon_operation_is_current(shared, operation_epoch) {
            return Ok(false);
        }
        if local_asr_endpoint_accepts_tcp(&request.endpoint, Duration::from_millis(80)) {
            return Ok(local_asr_daemon_operation_is_current(
                shared,
                operation_epoch,
            ));
        }
        if !local_asr_daemon_operation_is_current(shared, operation_epoch) {
            return Ok(false);
        }

        let daemon = start_managed_local_asr_daemon(
            &request.plan,
            request.endpoint.clone(),
            request.startup_timeout_ms,
        )?;
        Ok(install_managed_local_asr_daemon_if_current(
            shared,
            operation_epoch,
            daemon,
        ))
    }

    fn run_product_local_asr_daemon_prewarm(shared: &Arc<Mutex<SharedState>>) -> Result<()> {
        let lifecycle = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Talk desktop shared state is poisoned"))?
            .local_asr_daemon_lifecycle
            .clone();
        let _lifecycle_guard = lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Talk local ASR daemon lifecycle is poisoned"))?;
        let (observed_epoch, config, status, worker_path, model_root) = {
            let shared_state = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("Talk desktop shared state is poisoned"))?;
            if shared_state.shutting_down {
                return Ok(());
            }
            let Some(config) = shared_state.config.clone() else {
                return Ok(());
            };
            (
                shared_state.local_asr_daemon_epoch,
                config,
                shared_state.local_asr_bootstrap_status.clone(),
                shared_state.product_runtime_worker.clone(),
                shared_state.product_model_root.clone(),
            )
        };
        let Some(request) = product_local_asr_daemon_prewarm_request(
            &config,
            &status,
            worker_path.as_deref(),
            model_root.as_deref(),
        ) else {
            return Ok(());
        };
        let Some((operation_epoch, existing)) =
            reserve_local_asr_daemon_operation(shared, observed_epoch)?
        else {
            return Ok(());
        };
        let _ = reconcile_managed_local_asr_daemon(shared, operation_epoch, &request, existing)?;
        Ok(())
    }

    fn spawn_product_local_asr_daemon_prewarm(shared: Arc<Mutex<SharedState>>) {
        thread::spawn(move || {
            if let Err(error) = run_product_local_asr_daemon_prewarm(&shared) {
                eprintln!("Talk product local ASR prewarm failed: {error:#}");
            }
        });
    }

    fn ensure_packaged_local_asr_daemon(
        shared: &Arc<Mutex<SharedState>>,
        config: &TalkConfig,
    ) -> Result<bool> {
        let lifecycle = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Talk desktop shared state is poisoned"))?
            .local_asr_daemon_lifecycle
            .clone();
        let _lifecycle_guard = lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Talk local ASR daemon lifecycle is poisoned"))?;
        let (observed_epoch, status, worker_path, model_root) = {
            let shared_state = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("Talk desktop shared state is poisoned"))?;
            if shared_state.shutting_down {
                return Ok(false);
            }
            (
                shared_state.local_asr_daemon_epoch,
                shared_state.local_asr_bootstrap_status.clone(),
                shared_state.product_runtime_worker.clone(),
                shared_state.product_model_root.clone(),
            )
        };
        let Some(request) = packaged_local_asr_daemon_request(
            config,
            &status,
            worker_path.as_deref(),
            model_root.as_deref(),
        )?
        else {
            return Ok(false);
        };
        let Some((operation_epoch, existing)) =
            reserve_local_asr_daemon_operation(shared, observed_epoch)?
        else {
            return Ok(false);
        };
        reconcile_managed_local_asr_daemon(shared, operation_epoch, &request, existing)
    }

    fn managed_local_asr_daemon_launch_is_current(
        daemon_endpoint: &str,
        daemon_plan: &DesktopLocalAsrDaemonLaunchPlan,
        requested_endpoint: &str,
        requested_plan: &DesktopLocalAsrDaemonLaunchPlan,
    ) -> bool {
        daemon_endpoint == requested_endpoint && daemon_plan == requested_plan
    }

    fn local_asr_endpoint_accepts_tcp(endpoint: &str, timeout: Duration) -> bool {
        let Some(address) = local_asr_socket_addr_from_endpoint(endpoint) else {
            return false;
        };
        TcpStream::connect_timeout(&address, timeout).is_ok()
    }

    fn local_asr_socket_addr_from_endpoint(endpoint: &str) -> Option<SocketAddr> {
        let bind = desktop_local_asr_daemon_bind_from_endpoint(endpoint).ok()??;
        bind.parse().ok()
    }

    fn managed_local_asr_daemon_is_running(daemon: &mut ManagedLocalAsrDaemon) -> bool {
        match daemon.child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) => {
                eprintln!("Talk packaged local ASR daemon exited: {status}");
                false
            }
            Err(error) => {
                eprintln!("Talk packaged local ASR daemon status check failed: {error}");
                false
            }
        }
    }

    fn stop_managed_local_asr_daemon(mut daemon: ManagedLocalAsrDaemon) {
        match daemon.child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(error) => {
                eprintln!(
                    "Talk packaged local ASR daemon status check failed before stop: {error}"
                );
            }
        }
        if let Err(error) = daemon.child.kill() {
            eprintln!("Talk packaged local ASR daemon kill failed: {error}");
        }
        if let Err(error) = daemon.child.wait() {
            eprintln!("Talk packaged local ASR daemon wait failed: {error}");
        }
    }

    fn recording_begin_completion_allowed(
        shutting_down: bool,
        shell_state: ShellState,
        next_generation: u64,
        generation: u64,
    ) -> bool {
        !shutting_down
            && shell_state.is_preparing_local_asr()
            && next_generation == generation.saturating_add(1)
    }

    fn cleanup_prepared_recording_audio(
        runtime_handle: &tokio::runtime::Handle,
        result: Option<std::result::Result<PreparedRecordingAudio, RecordingAudioStartFailure>>,
        reason: &'static str,
    ) {
        if let Some(Ok(prepared)) = result {
            spawn_recording_source_cancel_worker(runtime_handle, prepared.source, reason);
        }
    }

    fn cleanup_pending_recording_begin(mut pending: PendingRecordingBegin, reason: &'static str) {
        let audio_start_result = pending.audio_start_result.take();
        cleanup_prepared_recording_audio(&pending.runtime_handle, audio_start_result, reason);
    }

    fn release_recording_begin_reservation(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
    ) -> bool {
        let pending = {
            let Ok(mut shared) = shared.lock() else {
                return false;
            };
            if !recording_begin_completion_allowed(
                shared.shutting_down,
                shared.shell_state,
                shared.next_generation,
                generation,
            ) {
                return false;
            }
            if shared
                .pending_recording_begin
                .as_ref()
                .is_some_and(|pending| pending.generation != generation)
            {
                return false;
            }
            let pending = shared.pending_recording_begin.take();
            shared.shell_state = shared.shell_state.complete();
            shared.current_phase = None;
            pending
        };
        if let Some(pending) = pending {
            cleanup_pending_recording_begin(pending, "recording preparation release");
        }
        true
    }

    fn cancel_pending_recording_begin(
        hwnd: HWND,
        expected_generation: Option<u64>,
    ) -> Result<bool> {
        let state = unsafe { get_window_state(hwnd)? };
        let pending = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            let Some(pending) = shared.pending_recording_begin.as_ref() else {
                return Ok(false);
            };
            if expected_generation.is_some_and(|generation| generation != pending.generation) {
                return Ok(false);
            }
            let pending = shared
                .pending_recording_begin
                .take()
                .expect("validated pending recording begin");
            shared.shell_state = shared.shell_state.complete();
            shared.current_phase = None;
            pending
        };
        cleanup_pending_recording_begin(pending, "recording preparation cancellation");
        let _ = update_tray_icon(hwnd, "Talk: cancelled");
        let _ = hide_hud(hwnd);
        let _ = refresh_idle_tray_status(hwnd);
        Ok(true)
    }

    fn spawn_local_asr_recording_prepare(
        shared: Arc<Mutex<SharedState>>,
        hwnd: HWND,
        generation: u64,
        config: Arc<TalkConfig>,
    ) {
        let hwnd_value = hwnd as usize;
        thread::spawn(move || {
            let result = ensure_packaged_local_asr_daemon(&shared, &config)
                .map_err(|error| error.to_string());
            let should_post = shared.lock().ok().is_some_and(|mut shared| {
                if !recording_begin_completion_allowed(
                    shared.shutting_down,
                    shared.shell_state,
                    shared.next_generation,
                    generation,
                ) {
                    return false;
                }
                let Some(pending) = shared.pending_recording_begin.as_mut() else {
                    return false;
                };
                if pending.generation != generation || pending.local_asr_result.is_some() {
                    return false;
                }
                pending.local_asr_result = Some(result);
                true
            });
            if should_post {
                let posted = unsafe {
                    PostMessageW(
                        hwnd_value as HWND,
                        LOCAL_ASR_PREPARE_DONE_MESSAGE,
                        generation as usize,
                        0,
                    )
                };
                if posted == 0 {
                    let _ = release_recording_begin_reservation(&shared, generation);
                }
            }
        });
    }

    fn recording_streaming_enabled_for_source(
        streaming_route_ready: bool,
        is_live_recording_source: bool,
    ) -> bool {
        streaming_route_ready && is_live_recording_source
    }

    fn recording_streaming_route_prepare_needed(
        configured_streaming_route: bool,
        explicit_audio_override_present: bool,
    ) -> bool {
        configured_streaming_route && !explicit_audio_override_present
    }

    fn prepare_recording_audio(
        shared: Arc<Mutex<SharedState>>,
        hwnd: HWND,
        generation: u64,
        config: Arc<TalkConfig>,
        runtime_handle: tokio::runtime::Handle,
        config_path: PathBuf,
        session_id: String,
        local_asr_ready: bool,
    ) -> std::result::Result<PreparedRecordingAudio, RecordingAudioStartFailure> {
        let speculative_local_asr_route =
            desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(&config));
        let streaming_route_ready =
            desktop_effective_streaming_asr_enabled(speculative_local_asr_route, local_asr_ready);
        let audio_override_raw = std::env::var(TALK_DESKTOP_AUDIO_FILE_OVERRIDE_ENV).ok();
        let audio_override =
            resolve_desktop_audio_file_override(audio_override_raw.as_deref(), &config_path)
                .map_err(|error| RecordingAudioStartFailure {
                    hud_detail: error.clone(),
                    message: error,
                    reason: "audio override resolution failure",
                })?;
        let use_streaming_speculative_asr =
            recording_streaming_enabled_for_source(streaming_route_ready, audio_override.is_none());
        let source = match audio_override {
            Some(audio_path) => ActiveRecordingSource::ExplicitAudioFile(audio_path),
            None => {
                let recording_request = AudioCaptureRequest {
                    backend: config.audio.backend,
                    temp_dir: config.audio.temp_dir.clone(),
                    session_id: session_id.clone(),
                    input_device: config.audio.input_device.clone(),
                    wav_settings: WavSettings {
                        sample_rate_hz: config.audio.sample_rate_hz,
                        channels: config.audio.channels,
                    },
                    max_recording_seconds: config.audio.max_recording_seconds,
                    silent_samples: 320,
                };
                let recording = start_recording(&recording_request).map_err(|error| {
                    let message = error.to_string();
                    RecordingAudioStartFailure {
                        hud_detail: message.clone(),
                        message,
                        reason: "recording start failure",
                    }
                })?;
                let streaming_pump = if use_streaming_speculative_asr {
                    let streaming_source = match recording.streaming_pcm_source() {
                        Ok(streaming_source) => streaming_source,
                        Err(error) => {
                            let message = error.to_string();
                            spawn_recording_source_cancel_worker(
                                &runtime_handle,
                                ActiveRecordingSource::Live {
                                    recording,
                                    streaming_pump: None,
                                },
                                "streaming PCM source failure",
                            );
                            return Err(RecordingAudioStartFailure {
                                message,
                                reason: "streaming PCM source failure",
                                hud_detail: "local streaming ASR unavailable".to_string(),
                            });
                        }
                    };
                    Some(spawn_local_streaming_asr_pump(
                        &runtime_handle,
                        shared,
                        hwnd,
                        generation,
                        streaming_source,
                        config,
                        session_id,
                    ))
                } else {
                    None
                };
                ActiveRecordingSource::Live {
                    recording,
                    streaming_pump,
                }
            }
        };

        Ok(PreparedRecordingAudio {
            source,
            use_streaming_speculative_asr,
        })
    }

    fn spawn_recording_audio_prepare(
        shared: Arc<Mutex<SharedState>>,
        hwnd: HWND,
        generation: u64,
        config: Arc<TalkConfig>,
        runtime_handle: tokio::runtime::Handle,
        config_path: PathBuf,
        session_id: String,
        local_asr_ready: bool,
    ) {
        let hwnd_value = hwnd as usize;
        thread::spawn(move || {
            let mut result = Some(prepare_recording_audio(
                Arc::clone(&shared),
                hwnd_value as HWND,
                generation,
                config,
                runtime_handle.clone(),
                config_path,
                session_id,
                local_asr_ready,
            ));
            let should_post = shared.lock().ok().is_some_and(|mut shared| {
                if !recording_begin_completion_allowed(
                    shared.shutting_down,
                    shared.shell_state,
                    shared.next_generation,
                    generation,
                ) {
                    return false;
                }
                let Some(pending) = shared.pending_recording_begin.as_mut() else {
                    return false;
                };
                if pending.generation != generation || pending.audio_start_result.is_some() {
                    return false;
                }
                pending.audio_start_result = result.take();
                true
            });
            if !should_post {
                cleanup_prepared_recording_audio(
                    &runtime_handle,
                    result.take(),
                    "stale recording audio preparation",
                );
                return;
            }
            let posted = unsafe {
                PostMessageW(
                    hwnd_value as HWND,
                    AUDIO_START_DONE_MESSAGE,
                    generation as usize,
                    0,
                )
            };
            if posted == 0 {
                let _ = release_recording_begin_reservation(&shared, generation);
            }
        });
    }

    fn handle_local_asr_prepare_done(hwnd: HWND, generation: u64) {
        let state = match unsafe { get_window_state(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let (result, config, runtime_handle, config_path, session_id) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if !recording_begin_completion_allowed(
                shared.shutting_down,
                shared.shell_state,
                shared.next_generation,
                generation,
            ) {
                return;
            }
            let Some(pending) = shared.pending_recording_begin.as_mut() else {
                return;
            };
            if pending.generation != generation || pending.local_asr_result.is_none() {
                return;
            }
            (
                pending
                    .local_asr_result
                    .take()
                    .expect("validated local ASR prepare result"),
                pending.config.clone(),
                pending.runtime_handle.clone(),
                pending.config_path.clone(),
                pending.session_id.clone(),
            )
        };
        let local_asr_ready = match result {
            Ok(ready) => ready,
            Err(error) => {
                if let Ok(mut shared) = state.shared.lock() {
                    if recording_begin_completion_allowed(
                        shared.shutting_down,
                        shared.shell_state,
                        shared.next_generation,
                        generation,
                    ) {
                        advance_local_asr_daemon_epoch(&mut shared);
                        shared.local_asr_bootstrap_status =
                            LocalAsrBootstrapStatus::FallbackCloud(error);
                    }
                }
                false
            }
        };
        spawn_recording_audio_prepare(
            Arc::clone(&state.shared),
            hwnd,
            generation,
            config,
            runtime_handle,
            config_path,
            session_id,
            local_asr_ready,
        );
    }

    fn handle_recording_audio_start_done(hwnd: HWND, generation: u64) {
        let state = match unsafe { get_window_state(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let (pending, result) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if !recording_begin_completion_allowed(
                shared.shutting_down,
                shared.shell_state,
                shared.next_generation,
                generation,
            ) {
                return;
            }
            let Some(pending) = shared.pending_recording_begin.as_ref() else {
                return;
            };
            if pending.generation != generation || pending.audio_start_result.is_none() {
                return;
            }
            let mut pending = shared
                .pending_recording_begin
                .take()
                .expect("validated pending recording begin");
            let result = pending
                .audio_start_result
                .take()
                .expect("validated recording audio start result");
            (pending, result)
        };

        match result {
            Ok(prepared) => {
                if let Err(error) = finish_recording_begin(hwnd, state, pending, prepared) {
                    eprintln!("Talk recording preparation completion failed: {error:#}");
                    let _ = release_recording_begin_reservation(&state.shared, generation);
                    let _ = show_hud_text(
                        hwnd,
                        &compose_hud_message("Talk: unavailable", Some(&error.to_string())),
                        Some(1800),
                    );
                }
            }
            Err(failure) => {
                set_failed_last_session_if_current(
                    &state.shared,
                    generation,
                    failure.message.clone(),
                );
                spawn_failed_session_persistence_worker(
                    &pending.runtime_handle,
                    Some(pending.config.clone()),
                    pending.session,
                    pending.trigger_events,
                    pending.mode_override,
                    anyhow::anyhow!(failure.message),
                    failure.reason,
                );
                let _ = release_recording_begin_reservation(&state.shared, generation);
                let _ = refresh_idle_tray_status(hwnd);
                let _ = show_hud_text(
                    hwnd,
                    &compose_hud_message("Talk: failed", Some(&failure.hud_detail)),
                    Some(1800),
                );
            }
        }
    }

    fn begin_recording(
        hwnd: HWND,
        state: &WindowState,
        source: ActivationSource,
        action_index: usize,
    ) -> Result<()> {
        let _ = cancel_pending_shortcut_help(hwnd);
        let _ = hide_copy_popup(hwnd);
        let (
            config,
            runtime_handle,
            hotkey,
            mode_override,
            generation,
            trigger_mode,
            max_recording_seconds,
            session_id,
            config_path,
            pending_hotkey_origin_insert_target,
            pending_document_correction_task,
        ) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if shared.shutting_down {
                anyhow::bail!("Talk desktop is shutting down");
            }
            if shared.pending_config_reload.is_some() {
                anyhow::bail!("Talk config reload is in progress");
            }
            if !shared.shell_state.can_start_session() {
                anyhow::bail!("Talk desktop is already busy");
            }
            shared.pending_copy_popup = None;
            shared.pending_corrected_hud = None;
            let Some(config) = shared.config.clone() else {
                anyhow::bail!("Talk config is unavailable; fix and reload the config first");
            };
            let action = shared
                .desktop_actions
                .get(action_index)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Talk desktop action index is invalid"))?;
            let mode_override = if action.route == DesktopActionRoute::Primary {
                Some(shared.selected_voice_mode)
            } else {
                action.mode_override
            };
            let generation = shared.next_generation;
            shared.next_generation = shared.next_generation.saturating_add(1);
            shared.shell_state = shared
                .shell_state
                .begin_local_asr_preparation()
                .expect("idle shell state should begin local ASR preparation");
            if let Some(mode) = mode_override {
                if action.route != DesktopActionRoute::Primary {
                    shared.selected_voice_mode = mode;
                }
            }
            (
                config.clone(),
                shared.runtime_handle.clone(),
                shared.hotkey.spec().cloned(),
                mode_override,
                generation,
                config.trigger.mode,
                config.audio.max_recording_seconds,
                Uuid::new_v4().to_string(),
                shared.config_path.clone(),
                if source == ActivationSource::Hotkey {
                    shared.pending_hotkey_origin_insert_target.take()
                } else {
                    None
                },
                shared.pending_document_correction_task.take(),
            )
        };
        if let Some((_, task)) = pending_document_correction_task {
            task.abort();
        }

        let mut session = VoiceSession::new(session_id.clone());
        if let Err(error) = session.apply(VoiceEvent::TriggerStart) {
            let _ = release_recording_begin_reservation(&state.shared, generation);
            return Err(anyhow::anyhow!(error.to_string()))
                .context("start Talk desktop voice session");
        }
        let trigger_events = vec!["trigger_start"];
        let (origin_insert_target, origin_insert_target_source, release_time_origin_target) =
            if source == ActivationSource::Hotkey {
                // HWND-only snapshot keeps the hotkey activation path free of
                // blocking UI Automation calls; spawn_hotkey_origin_enrichment
                // upgrades the captured target on a background thread.
                let release_time_origin_target =
                    capture_foreground_insert_target_context_snapshot(hwnd, state.hud_hwnd.get());
                let origin_insert_target = resolve_hotkey_origin_insert_target(
                    pending_hotkey_origin_insert_target.as_ref(),
                    release_time_origin_target.as_ref(),
                );
                let origin_insert_target_source =
                    origin_insert_target.as_ref().and_then(|target| {
                        if pending_hotkey_origin_insert_target.as_ref() == Some(target) {
                            Some("hotkey_pending_pretrigger".to_string())
                        } else if release_time_origin_target.as_ref() == Some(target) {
                            Some("hotkey_release_time".to_string())
                        } else {
                            None
                        }
                    });
                (
                    origin_insert_target,
                    origin_insert_target_source,
                    release_time_origin_target,
                )
            } else {
                (
                    capture_foreground_insert_target_context(hwnd, state.hud_hwnd.get()),
                    Some("record_start_capture".to_string()),
                    None,
                )
            };
        let speculative_local_asr_route =
            desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(&config));
        let configured_streaming_speculative_asr =
            speculative_local_asr_route == DesktopSpeculativeLocalAsrRoute::StreamingService;
        let explicit_audio_override_present =
            std::env::var_os(TALK_DESKTOP_AUDIO_FILE_OVERRIDE_ENV).is_some();
        let prepare_streaming_route = recording_streaming_route_prepare_needed(
            configured_streaming_speculative_asr,
            explicit_audio_override_present,
        );
        if let Some((summary, detail)) =
            recording_streaming_preflight_message(prepare_streaming_route)
        {
            let _ = update_tray_icon(hwnd, summary);
            let _ = show_hud_text(hwnd, &compose_hud_message(summary, Some(detail)), None);
        }
        if trigger_mode == TriggerMode::PushToTalk && source == ActivationSource::Hotkey {
            if let Some(hotkey) = hotkey.as_ref() {
                if select_windows_hotkey_binding_strategy(hotkey)
                    == WindowsHotkeyBindingStrategy::RegisterHotKey
                {
                    spawn_preparation_release_watcher(hwnd, generation, hotkey.clone());
                }
            }
        }

        let pending = PendingRecordingBegin {
            action_index,
            source,
            config: config.clone(),
            runtime_handle,
            hotkey,
            mode_override,
            generation,
            trigger_mode,
            max_recording_seconds,
            session_id,
            config_path,
            session,
            trigger_events,
            origin_insert_target,
            origin_insert_target_source,
            pending_hotkey_origin_insert_target,
            release_time_origin_target,
            local_asr_result: None,
            audio_start_result: None,
        };
        let audio_prepare_config = pending.config.clone();
        let audio_prepare_runtime_handle = pending.runtime_handle.clone();
        let audio_prepare_config_path = pending.config_path.clone();
        let audio_prepare_session_id = pending.session_id.clone();
        let stored = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if recording_begin_completion_allowed(
                shared.shutting_down,
                shared.shell_state,
                shared.next_generation,
                generation,
            ) {
                shared.pending_recording_begin = Some(pending);
                true
            } else {
                false
            }
        };
        if !stored {
            let _ = release_recording_begin_reservation(&state.shared, generation);
            return Ok(());
        }
        if prepare_streaming_route {
            spawn_local_asr_recording_prepare(Arc::clone(&state.shared), hwnd, generation, config);
        } else {
            spawn_recording_audio_prepare(
                Arc::clone(&state.shared),
                hwnd,
                generation,
                audio_prepare_config,
                audio_prepare_runtime_handle,
                audio_prepare_config_path,
                audio_prepare_session_id,
                false,
            );
        }
        Ok(())
    }

    fn finish_recording_begin(
        hwnd: HWND,
        state: &WindowState,
        pending: PendingRecordingBegin,
        prepared: PreparedRecordingAudio,
    ) -> Result<()> {
        let PendingRecordingBegin {
            action_index,
            source,
            config,
            runtime_handle,
            hotkey,
            mode_override,
            generation,
            trigger_mode,
            max_recording_seconds,
            session_id: _,
            config_path: _,
            session,
            trigger_events,
            origin_insert_target,
            origin_insert_target_source,
            pending_hotkey_origin_insert_target,
            release_time_origin_target,
            local_asr_result: _,
            audio_start_result: _,
        } = pending;
        let PreparedRecordingAudio {
            source: recording_source,
            use_streaming_speculative_asr,
        } = prepared;
        let preparation_is_current = state.shared.lock().ok().is_some_and(|shared| {
            recording_begin_completion_allowed(
                shared.shutting_down,
                shared.shell_state,
                shared.next_generation,
                generation,
            )
        });
        if !preparation_is_current {
            spawn_recording_source_cancel_worker(
                &runtime_handle,
                recording_source,
                "stale recording audio completion",
            );
            return Ok(());
        }
        if trigger_mode == TriggerMode::PushToTalk
            && source == ActivationSource::Hotkey
            && hotkey.as_ref().is_some_and(|hotkey| !hotkey.is_pressed())
        {
            spawn_recording_source_cancel_worker(
                &runtime_handle,
                recording_source,
                "push-to-talk release during audio preparation",
            );
            let _ = release_recording_begin_reservation(&state.shared, generation);
            let _ = hide_hud(hwnd);
            let _ = refresh_idle_tray_status(hwnd);
            return Ok(());
        }

        let speculative_local_asr_route =
            desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(&config));
        let configured_streaming_speculative_asr =
            speculative_local_asr_route == DesktopSpeculativeLocalAsrRoute::StreamingService;
        let streaming_unavailable_hint = {
            let shared = lock_recovering(&state.shared, "Talk desktop shared state");
            recording_streaming_unavailable_hint(
                configured_streaming_speculative_asr,
                use_streaming_speculative_asr,
                &shared.local_asr_bootstrap_status,
            )
            .map(str::to_string)
        };
        if configured_streaming_speculative_asr && !use_streaming_speculative_asr {
            let _ = update_tray_icon(hwnd, "Talk: cloud ASR fallback");
            if !matches!(
                lock_recovering(&state.shared, "Talk desktop shared state")
                    .local_asr_bootstrap_status,
                LocalAsrBootstrapStatus::Downloading
            ) {
                let _ = show_hud_text(
                    hwnd,
                    &compose_hud_message(
                        "Talk: cloud ASR fallback",
                        Some("local streaming ASR is unavailable"),
                    ),
                    Some(1800),
                );
            }
        }
        let correction_policy = desktop_live_correction_worker_policy();
        let (live_correction_sender, mut live_correction_receiver) =
            tokio::sync::mpsc::channel::<SpeculativeCloudCorrectionJob>(
                correction_policy.queue_capacity,
            );
        let live_correction_tracker = Arc::new(LiveCorrectionTracker::new(generation));
        let live_correction_shared = Arc::clone(&state.shared);
        let live_correction_worker_tracker = Arc::clone(&live_correction_tracker);
        let correction_worker_gate = lock_recovering(&state.shared, "Talk desktop shared state")
            .correction_worker_gate
            .clone();
        runtime_handle.spawn(async move {
            while let Some(job) = live_correction_receiver.recv().await {
                let segment_id = job.segment_id.clone();
                let permit = match correction_worker_gate.acquire().await {
                    Some(permit) => permit,
                    None => break,
                };
                let shared = Arc::clone(&live_correction_shared);
                let tracker = Arc::clone(&live_correction_worker_tracker);
                tokio::spawn(async move {
                    let _permit = permit;
                    run_speculative_cloud_correction(shared, Some(Arc::clone(&tracker)), job).await;
                    tracker.complete_job(&segment_id, None, None);
                });
            }
        });

        {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            shared.shell_state = shared
                .shell_state
                .begin_recording()
                .expect("preparing shell state should begin recording");
            shared.pending_recording_begin = None;
            shared.current_phase = Some(RuntimePhase::Recording);
            shared.active_recording = Some(ActiveRecording {
                action_index,
                mode_override,
                live_smart_routed_mode: None,
                generation,
                session,
                trigger_events,
                origin_insert_target,
                origin_insert_target_source,
                pending_hotkey_origin_insert_target,
                release_time_origin_insert_target: release_time_origin_target,
                source: recording_source,
                use_streaming_speculative_asr,
                speculative_runtime_state: SpeculativeRuntimeState::default(),
                speculative_segmenter_config: desktop_live_streaming_segmenter_config(),
                live_streaming_inserted_anchors: HashMap::new(),
                live_streaming_inserted_segment_ids: Vec::new(),
                hud_streaming_segments: Arc::new(Vec::new()),
                hud_streaming_segments_revision: 0,
                live_streaming_correction_sender: Some(live_correction_sender),
                live_streaming_correction_tracker: live_correction_tracker,
                live_correction_snapshot_revision: 0,
                live_correction_snapshot: Vec::new(),
                last_streaming_asr_event: None,
                last_streaming_asr_event_at: None,
                last_streaming_idle_evaluated_ms: 0,
                hud_waveform: [0.0; HUD_WAVEFORM_BUCKET_COUNT],
                pending_streaming_pump_results: VecDeque::new(),
                streaming_unavailable_hint,
            });
        }

        if source == ActivationSource::Hotkey {
            spawn_hotkey_origin_enrichment(
                hwnd,
                state.hud_hwnd.get(),
                generation,
                Arc::clone(&state.shared),
            );
        }

        if let Err(error) = update_tray_icon(hwnd, "Talk: listening") {
            eprintln!("Talk listening tray status update failed: {error:#}");
        }
        if let Err(error) =
            show_hud_text(hwnd, hud_message_for_phase(RuntimePhase::Recording), None)
        {
            eprintln!("Talk listening HUD update failed: {error:#}");
        }

        let stop_watcher_policy =
            recording_stop_watcher_policy(trigger_mode, max_recording_seconds);
        if trigger_mode == TriggerMode::PushToTalk && source == ActivationSource::Hotkey {
            if let Some(hotkey) = hotkey {
                match select_windows_hotkey_binding_strategy(&hotkey) {
                    WindowsHotkeyBindingStrategy::RegisterHotKey => {
                        if let DesktopRecordingStopWatcherPolicy::TimeoutAfterSeconds(seconds) =
                            stop_watcher_policy
                        {
                            arm_recording_timeout_timer(hwnd, generation, seconds);
                        }
                    }
                    WindowsHotkeyBindingStrategy::LowLevelHook => {
                        if let DesktopRecordingStopWatcherPolicy::TimeoutAfterSeconds(seconds) =
                            stop_watcher_policy
                        {
                            arm_recording_timeout_timer(hwnd, generation, seconds);
                        }
                    }
                }
            } else if let DesktopRecordingStopWatcherPolicy::TimeoutAfterSeconds(seconds) =
                stop_watcher_policy
            {
                arm_recording_timeout_timer(hwnd, generation, seconds);
            }
        } else {
            match stop_watcher_policy {
                DesktopRecordingStopWatcherPolicy::ManualOnly => {}
                DesktopRecordingStopWatcherPolicy::TimeoutAfterSeconds(seconds) => {
                    arm_recording_timeout_timer(hwnd, generation, seconds);
                }
            }
        }

        Ok(())
    }

    fn desktop_speculative_pipeline_config(
        config: &TalkConfig,
    ) -> DesktopSpeculativePipelineConfig {
        DesktopSpeculativePipelineConfig {
            enabled: config.speculative.enabled,
            local_asr: config.speculative.local_asr.clone(),
            cloud_correction: config.speculative.cloud_correction.clone(),
        }
    }

    fn show_mock_speculative_preview(hwnd: HWND, config: &TalkConfig) -> Result<()> {
        let preview_text = config
            .provider
            .mock_transcript
            .clone()
            .unwrap_or_else(|| "local ASR preview".to_string());
        let events = run_mock_speculative_session(vec![
            (false, "mock-preview", preview_text.as_str()),
            (true, "mock-preview", preview_text.as_str()),
        ])?;

        for event in events {
            match event {
                SpeculativeRuntimeEvent::DraftUpdated { text, .. }
                | SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. } => {
                    show_hud_text(hwnd, &text, None)?;
                }
                SpeculativeRuntimeEvent::CorrectionRequested { .. }
                | SpeculativeRuntimeEvent::LocalSegmentsInvalidated { .. } => {}
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn apply_speculative_correction_patch_if_safe(
        hwnd: HWND,
        hud_hwnd: HWND,
        anchor: &SpeculativeInsertAnchor,
        origin_insert_target: Option<&DesktopInsertTargetContext>,
        segment_id: &str,
        corrected_text: &str,
        received_at_ms: u64,
        max_age_ms: u64,
        max_edit_ratio: f32,
        restore_clipboard: bool,
    ) -> Result<bool> {
        let current_context = capture_foreground_insert_target_context(hwnd, hud_hwnd);
        let Some(current_target) = current_context.as_ref().and_then(|context| context.target)
        else {
            return Ok(false);
        };
        if desktop_output_plan(
            OutputMode::ClipboardPaste,
            origin_insert_target,
            current_context.as_ref(),
        )
        .insert_target
            != Some(current_target)
        {
            return Ok(false);
        }
        let candidate = SpeculativePatchCandidate::new(
            current_target.window_handle,
            current_target.focus_handle,
            segment_id,
            corrected_text,
            received_at_ms,
        )
        .map_err(|error| anyhow::anyhow!(error))?;

        if decide_speculative_patch_application(anchor, &candidate, max_age_ms, max_edit_ratio)
            != SpeculativePatchApplication::Apply
        {
            return Ok(false);
        }

        send_shift_left_selection(desktop_speculative_replacement_selection_count(
            &anchor.inserted_text,
        ))?;
        let restore_policy = if restore_clipboard {
            ClipboardRestorePolicy::RestoreOriginal
        } else {
            ClipboardRestorePolicy::LeaveInsertedText
        };
        let inserter = ClipboardPasteInserter::with_settle_delay(
            WindowsClipboardBackend,
            configured_windows_paste_shortcut(
                None,
                desktop_direct_control_paste_focus_handle(current_context.as_ref()),
            ),
            restore_policy,
            Duration::from_millis(desktop_live_clipboard_settle_delay_ms()),
        )
        .with_deferred_restore();
        inserter.insert_text(corrected_text)?;
        Ok(true)
    }

    fn normalize_recorrection_target_text(value: &str) -> String {
        value.replace("\r\n", "\n").replace('\r', "\n")
    }

    fn automation_element_current_text(element: &UIElement) -> Option<String> {
        if let Ok(value_pattern) = element.get_pattern::<UIValuePattern>() {
            if let Ok(value) = value_pattern.get_value() {
                return Some(value);
            }
        }

        let text_pattern = element.get_pattern::<UITextPattern>().ok()?;
        let document_range = text_pattern.get_document_range().ok()?;
        document_range.get_text(-1).ok()
    }

    fn capture_current_insert_target_text(context: &DesktopInsertTargetContext) -> Option<String> {
        let target = context.target?;
        ensure_uia_com_initialized_for_current_thread();

        if let Some(focus_handle) = target.focus_handle {
            let focus_hwnd = focus_handle as HWND;
            if !focus_hwnd.is_null() {
                if let Ok(automation) = UIAutomation::new() {
                    if let Ok(element) = automation
                        .element_from_handle(UiAutomationHandle::from(WinHwnd(focus_hwnd)))
                    {
                        if let Some(text) = automation_element_current_text(&element) {
                            return Some(text);
                        }
                    }
                }
                if let Some(text) = window_text(focus_hwnd) {
                    return Some(text);
                }
            }
        }

        if let Ok(automation) = UIAutomation::new() {
            if let Ok(element) = automation.get_focused_element() {
                if let Some(text) = automation_element_current_text(&element) {
                    return Some(text);
                }
            }

            let candidate_hwnd = target.window_handle as HWND;
            if !candidate_hwnd.is_null() {
                if let Ok(element) = automation
                    .element_from_handle(UiAutomationHandle::from(WinHwnd(candidate_hwnd)))
                {
                    if let Some(text) = automation_element_current_text(&element) {
                        return Some(text);
                    }
                }
            }
        }

        window_text(target.window_handle as HWND)
    }

    fn apply_document_recorrection_patch_if_safe(
        hwnd: HWND,
        hud_hwnd: HWND,
        origin_insert_target: Option<&DesktopInsertTargetContext>,
        inserted_segments: &[String],
        corrected_text: &str,
        restore_clipboard: bool,
    ) -> Result<bool> {
        if inserted_segments.is_empty() {
            return Ok(false);
        }

        let current_context = capture_foreground_insert_target_context(hwnd, hud_hwnd);
        let Some(current_context) = current_context.as_ref() else {
            return Ok(false);
        };
        let Some(current_target) = current_context.target else {
            return Ok(false);
        };
        let target_still_safe = desktop_output_plan(
            OutputMode::ClipboardPaste,
            origin_insert_target,
            Some(current_context),
        )
        .insert_target
            == Some(current_target);
        if !target_still_safe {
            return Ok(false);
        }

        let Some(current_target_text) = capture_current_insert_target_text(current_context) else {
            return Ok(false);
        };
        let normalized_current_text = normalize_recorrection_target_text(&current_target_text);
        let normalized_inserted_segments = inserted_segments
            .iter()
            .map(|segment| normalize_recorrection_target_text(segment))
            .collect::<Vec<_>>();
        if desktop_document_recorrection_session_decision(
            &normalized_inserted_segments,
            &normalized_current_text,
            target_still_safe,
        ) != DesktopDocumentRecorrectionDecision::AutoApplyToTarget
        {
            return Ok(false);
        }

        send_ctrl_a_selection()?;
        let restore_policy = if restore_clipboard {
            ClipboardRestorePolicy::RestoreOriginal
        } else {
            ClipboardRestorePolicy::LeaveInsertedText
        };
        let inserter = ClipboardPasteInserter::with_settle_delay(
            WindowsClipboardBackend,
            configured_windows_paste_shortcut(
                None,
                desktop_direct_control_paste_focus_handle(Some(current_context)),
            ),
            restore_policy,
            Duration::from_millis(desktop_live_clipboard_settle_delay_ms()),
        )
        .with_deferred_restore();
        inserter.insert_text(corrected_text)?;
        Ok(true)
    }

    fn document_recorrection_generation_is_current_for_shared(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
    ) -> bool {
        shared.lock().ok().is_some_and(|shared| {
            desktop_document_recorrection_generation_is_current(shared.next_generation, generation)
        })
    }

    fn apply_document_recorrection_patch_if_current(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
        hwnd: HWND,
        hud_hwnd: HWND,
        origin_insert_target: Option<&DesktopInsertTargetContext>,
        inserted_segments: &[String],
        corrected_text: &str,
        restore_clipboard: bool,
    ) -> Result<bool> {
        // Keep the generation lease through the target mutation. begin_recording also
        // takes this mutex, so a newer session cannot start between the check and Ctrl+A.
        let generation_lease = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Talk shared state is poisoned"))?;
        if !desktop_document_recorrection_generation_is_current(
            generation_lease.next_generation,
            generation,
        ) {
            return Ok(false);
        }
        apply_document_recorrection_patch_if_safe(
            hwnd,
            hud_hwnd,
            origin_insert_target,
            inserted_segments,
            corrected_text,
            restore_clipboard,
        )
    }

    enum SpeculativeProviderWait<T> {
        Completed(T),
        TimedOut,
        Cancelled,
    }

    async fn wait_for_speculative_provider<T, F>(
        live_tracker: Option<&LiveCorrectionTracker>,
        timeout: Duration,
        provider: F,
    ) -> SpeculativeProviderWait<T>
    where
        F: Future<Output = T>,
    {
        if let Some(tracker) = live_tracker {
            tokio::select! {
                _ = tracker.wait_until_cancelled() => SpeculativeProviderWait::Cancelled,
                result = tokio::time::timeout(timeout, provider) => match result {
                    Ok(result) => SpeculativeProviderWait::Completed(result),
                    Err(_) => SpeculativeProviderWait::TimedOut,
                },
            }
        } else {
            match tokio::time::timeout(timeout, provider).await {
                Ok(result) => SpeculativeProviderWait::Completed(result),
                Err(_) => SpeculativeProviderWait::TimedOut,
            }
        }
    }

    async fn run_speculative_cloud_correction(
        shared: Arc<Mutex<SharedState>>,
        live_tracker: Option<Arc<LiveCorrectionTracker>>,
        job: SpeculativeCloudCorrectionJob,
    ) {
        let mut timing = LiveCorrectionTiming::new(&job);
        let foreground_apply_gate = shared
            .lock()
            .ok()
            .map(|shared| shared.foreground_apply_gate.clone());
        if live_tracker
            .as_ref()
            .is_some_and(|tracker| !tracker.can_process(job.segment_id.as_str()))
        {
            timing.outcome("stale_or_cancelled");
            return;
        }
        if live_tracker.is_none()
            && job.anchor.is_none()
            && !live_streaming_unanchored_correction_is_current(
                &shared,
                job.generation,
                job.segment_id.as_str(),
            )
        {
            timing.outcome("stale_unanchored");
            return;
        }
        if live_tracker.is_none()
            && !job.full_document_inserted_segments.is_empty()
            && !document_recorrection_generation_is_current_for_shared(&shared, job.generation)
        {
            timing.outcome("stale_document");
            return;
        }

        let mut front_context = FrontContext::default();
        let corrected_context = live_tracker.as_ref().map(|tracker| {
            tracker.corrected_context_before(
                job.segment_id.as_str(),
                desktop_live_streaming_segmenter_config().correction_context_chars,
            )
        });
        if let Some(context_before) = corrected_context
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or(job.context_before.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            front_context.extra.insert(
                "contextBefore".to_string(),
                Value::String(context_before.to_string()),
            );
        }
        timing.provider_started();
        let provider_timeout =
            Duration::from_millis(desktop_live_correction_worker_policy().provider_timeout_ms);
        let RuntimeProcessedOutput {
            text: corrected_text,
            faithful_validation,
            ..
        } = match wait_for_speculative_provider(
            live_tracker.as_deref(),
            provider_timeout,
            process_voice_transcript_text_with_diagnostics(
                &job.config,
                job.transcript.clone(),
                Some(job.processing_mode),
                front_context,
            ),
        )
        .await
        {
            SpeculativeProviderWait::Completed(Ok(output)) => {
                timing.provider_finished();
                output
            }
            SpeculativeProviderWait::Completed(Err(error)) => {
                timing.provider_finished();
                eprintln!("Talk speculative cloud correction failed: {error:#}");
                let Some(output) =
                    live_correction_local_fallback_output(live_tracker.as_ref(), &job)
                else {
                    timing.outcome("provider_error");
                    return;
                };
                output
            }
            SpeculativeProviderWait::TimedOut => {
                timing.provider_finished();
                let Some(output) =
                    live_correction_local_fallback_output(live_tracker.as_ref(), &job)
                else {
                    timing.outcome("provider_timeout");
                    return;
                };
                output
            }
            SpeculativeProviderWait::Cancelled => {
                timing.provider_finished();
                timing.outcome("cancelled");
                return;
            }
        };
        if let Some(tracker) = live_tracker.as_ref() {
            if !tracker.wait_until_apply_turn(job.segment_id.as_str()).await {
                timing.outcome("cancelled_before_apply");
                return;
            }
        }
        let corrected_text =
            desktop_correction_text_with_local_boundary(&job.transcript, &corrected_text);
        timing.apply_started();

        if corrected_text.trim().is_empty() {
            timing.outcome("empty_result");
            return;
        }
        if let Some(session_log_path) = job.session_log_path.as_deref() {
            if let Err(error) = update_session_log_after_text_processing(
                session_log_path,
                &corrected_text,
                faithful_validation,
            ) {
                eprintln!(
                    "Talk final text processing log update failed for {}: {error:#}",
                    session_log_path.display()
                );
            }
        }
        if job.anchor.is_none() {
            if let Some(tracker) = live_tracker.as_ref() {
                let Some(_apply_lease) = tracker.acquire_apply_lease(job.segment_id.as_str())
                else {
                    timing.outcome("cancelled_before_apply");
                    return;
                };
                tracker.record_result(job.segment_id.as_str(), &corrected_text, None);

                let current_smart_routed_mode = shared.lock().ok().and_then(|shared| {
                    shared
                        .active_recording
                        .as_ref()
                        .filter(|active| active.generation == job.generation)
                        .and_then(|active| active.live_smart_routed_mode)
                });
                let target_apply_allowed = live_correction_job_target_apply_allowed(
                    job.allow_target_apply,
                    job.latest_live_segment_guard.is_some(),
                    job.requested_mode,
                    current_smart_routed_mode,
                );
                if !target_apply_allowed {
                    if tracker.should_show_live_feedback() {
                        queue_live_streaming_corrected_hud(
                            job.hwnd_value as HWND,
                            job.generation,
                            &corrected_text,
                        );
                    }
                    timing.outcome("recorded_for_backlog");
                    return;
                }

                let Some(foreground_apply_gate) = foreground_apply_gate else {
                    if tracker.should_show_live_feedback() {
                        queue_live_streaming_corrected_hud(
                            job.hwnd_value as HWND,
                            job.generation,
                            &corrected_text,
                        );
                    }
                    timing.outcome("foreground_gate_unavailable");
                    return;
                };
                let foreground_apply_lease = foreground_apply_gate.acquire();
                if !correction_foreground_side_effects_allowed_for_shared(&shared, job.generation) {
                    if tracker.should_show_live_feedback() {
                        queue_live_streaming_corrected_hud(
                            job.hwnd_value as HWND,
                            job.generation,
                            &corrected_text,
                        );
                    }
                    timing.outcome("stale_before_foreground_apply");
                    return;
                }

                for anchor in insert_live_streaming_corrected_backlog_if_safe(
                    &job.config,
                    job.hwnd_value as HWND,
                    job.hud_hwnd_value as HWND,
                    job.origin_insert_target.as_ref(),
                    tracker,
                ) {
                    mirror_live_streaming_inserted_anchor(&shared, job.generation, anchor);
                }
                let inserted = tracker.has_insert_anchor(&job.segment_id);
                drop(foreground_apply_lease);

                if tracker.should_show_live_feedback() {
                    queue_live_streaming_corrected_hud(
                        job.hwnd_value as HWND,
                        job.generation,
                        &corrected_text,
                    );
                    timing.outcome(if inserted { "patched" } else { "hud_refreshed" });
                } else {
                    timing.outcome(if inserted { "patched" } else { "recorded" });
                }
                return;
            }
        }
        let current_smart_routed_mode = shared.lock().ok().and_then(|shared| {
            shared
                .active_recording
                .as_ref()
                .filter(|active| active.generation == job.generation)
                .and_then(|active| active.live_smart_routed_mode)
        });
        let target_apply_allowed = live_correction_job_target_apply_allowed(
            job.allow_target_apply,
            job.latest_live_segment_guard.is_some(),
            job.requested_mode,
            current_smart_routed_mode,
        );
        if live_tracker.is_some() && !target_apply_allowed {
            if let Some(tracker) = live_tracker.as_ref() {
                tracker.record_result(job.segment_id.as_str(), &corrected_text, job.anchor.clone());
                if tracker.should_show_live_feedback() {
                    queue_live_streaming_corrected_hud(
                        job.hwnd_value as HWND,
                        job.generation,
                        &corrected_text,
                    );
                }
            }
            timing.outcome("recorded_for_backlog");
            return;
        }
        let _apply_lease = if let Some(tracker) = live_tracker.as_ref() {
            let Some(lease) = tracker.acquire_apply_lease(job.segment_id.as_str()) else {
                timing.outcome("cancelled_before_apply");
                return;
            };
            Some(lease)
        } else {
            None
        };
        if speculative_correction_unchanged_fast_path_allowed(&job, &corrected_text) {
            if let (Some(tracker), Some(anchor)) = (live_tracker.as_ref(), job.anchor.clone()) {
                tracker.record_result(job.segment_id.as_str(), &corrected_text, Some(anchor));
                if matches!(
                    desktop_live_correction_presentation(
                        true,
                        tracker.should_show_live_feedback(),
                        false,
                    ),
                    DesktopLiveCorrectionPresentation::RefreshHud
                ) {
                    queue_live_streaming_corrected_hud(
                        job.hwnd_value as HWND,
                        job.generation,
                        &corrected_text,
                    );
                }
            }
            timing.outcome("unchanged");
            return;
        }

        let received_at_ms = job.started_at.elapsed().as_millis() as u64;
        let Some(foreground_apply_gate) = foreground_apply_gate else {
            timing.outcome("foreground_gate_unavailable");
            return;
        };
        let foreground_apply_lease = foreground_apply_gate.acquire();
        if !correction_foreground_side_effects_allowed_for_shared(&shared, job.generation) {
            timing.outcome("stale_before_foreground_apply");
            return;
        }
        let patched = if let Some(anchor) = job.anchor.as_ref() {
            if !job.full_document_inserted_segments.is_empty() {
                match apply_document_recorrection_patch_if_current(
                    &shared,
                    job.generation,
                    job.hwnd_value as HWND,
                    job.hud_hwnd_value as HWND,
                    job.origin_insert_target.as_ref(),
                    &job.full_document_inserted_segments,
                    &corrected_text,
                    job.config.output.restore_clipboard,
                ) {
                    Ok(patched) => patched,
                    Err(error) => {
                        eprintln!("Talk document recorrection patch failed: {error:#}");
                        false
                    }
                }
            } else {
                let patch_allowed = job.allow_target_apply
                    && job.latest_live_segment_guard.is_none_or(|guard| {
                        live_streaming_correction_anchor_still_latest(
                            &shared,
                            guard,
                            anchor.segment_id.as_str(),
                        )
                    });
                match desktop_live_correction_anchor_policy(live_tracker.is_some(), patch_allowed) {
                    DesktopLiveCorrectionAnchorPolicy::PatchAndRecord => {
                        match apply_speculative_correction_patch_if_safe(
                            job.hwnd_value as HWND,
                            job.hud_hwnd_value as HWND,
                            anchor,
                            job.origin_insert_target.as_ref(),
                            anchor.segment_id.as_str(),
                            &corrected_text,
                            received_at_ms,
                            job.config.speculative.max_patch_age_ms,
                            job.config.speculative.max_auto_patch_edit_ratio,
                            job.config.output.restore_clipboard,
                        ) {
                            Ok(patched) => {
                                record_live_tracker_result_for_anchor(
                                    &shared,
                                    live_tracker.as_ref(),
                                    &job,
                                    anchor,
                                    &corrected_text,
                                    patched,
                                );
                                patched
                            }
                            Err(error) => {
                                eprintln!("Talk speculative correction patch failed: {error:#}");
                                record_live_tracker_result_for_anchor(
                                    &shared,
                                    live_tracker.as_ref(),
                                    &job,
                                    anchor,
                                    &corrected_text,
                                    false,
                                );
                                false
                            }
                        }
                    }
                    DesktopLiveCorrectionAnchorPolicy::RecordOnly => {
                        record_live_tracker_result_for_anchor(
                            &shared,
                            live_tracker.as_ref(),
                            &job,
                            anchor,
                            &corrected_text,
                            false,
                        );
                        false
                    }
                    DesktopLiveCorrectionAnchorPolicy::Ignore => false,
                }
            }
        } else {
            false
        };
        drop(foreground_apply_lease);

        match desktop_live_correction_presentation(
            live_tracker.is_some(),
            live_tracker
                .as_ref()
                .is_some_and(|tracker| tracker.should_show_live_feedback()),
            patched,
        ) {
            DesktopLiveCorrectionPresentation::RefreshHud => {
                queue_live_streaming_corrected_hud(
                    job.hwnd_value as HWND,
                    job.generation,
                    &corrected_text,
                );
                timing.outcome(if patched { "patched" } else { "hud_refreshed" });
                return;
            }
            DesktopLiveCorrectionPresentation::None => {
                timing.outcome(if patched { "patched" } else { "recorded" });
                return;
            }
            DesktopLiveCorrectionPresentation::ShowCopyPopup => {}
        }

        if job
            .latest_live_segment_guard
            .is_some_and(|guard| !live_streaming_correction_generation_is_active(&shared, guard))
        {
            timing.outcome("stale_before_popup");
            return;
        }

        let queued = if let Ok(mut shared) = shared.lock() {
            if desktop_document_recorrection_generation_is_current(
                shared.next_generation,
                job.generation,
            ) {
                shared.pending_copy_popup = Some(PendingCopyPopup {
                    generation: job.generation,
                    model: desktop_copy_popup_model(&corrected_text),
                });
                true
            } else {
                false
            }
        } else {
            false
        };
        if queued {
            unsafe {
                let _ = PostMessageW(
                    job.hwnd_value as HWND,
                    CORRECTION_COPY_POPUP_MESSAGE,
                    job.generation as usize,
                    0,
                );
            }
            timing.outcome("popup_queued");
        } else {
            timing.outcome("popup_skipped");
        }
    }

    fn live_correction_local_fallback_output(
        live_tracker: Option<&Arc<LiveCorrectionTracker>>,
        job: &SpeculativeCloudCorrectionJob,
    ) -> Option<RuntimeProcessedOutput> {
        let tracker = live_tracker?;
        let text = tracker.local_fallback_text(job.segment_id.as_str())?;
        Some(RuntimeProcessedOutput {
            provider_output_text: text.clone(),
            text,
            faithful_validation: None,
        })
    }

    fn record_live_tracker_result_for_anchor(
        shared: &Arc<Mutex<SharedState>>,
        live_tracker: Option<&Arc<LiveCorrectionTracker>>,
        job: &SpeculativeCloudCorrectionJob,
        anchor: &SpeculativeInsertAnchor,
        corrected_text: &str,
        patched: bool,
    ) {
        if patched {
            if let Some(guard) = job.latest_live_segment_guard {
                update_live_streaming_inserted_anchor_text(
                    shared,
                    guard.generation,
                    anchor.segment_id.as_str(),
                    corrected_text,
                );
            }
        }
        if let Some(tracker) = live_tracker {
            let mut tracker_anchor = anchor.clone();
            if patched {
                tracker_anchor.inserted_text = corrected_text.to_string();
            }
            tracker.record_result(
                job.segment_id.as_str(),
                corrected_text,
                Some(tracker_anchor),
            );
        }
    }

    fn spawn_speculative_cloud_correction(
        runtime_handle: tokio::runtime::Handle,
        shared: Arc<Mutex<SharedState>>,
        job: SpeculativeCloudCorrectionJob,
    ) {
        let correction_worker_gate = match shared.lock() {
            Ok(shared) => shared.correction_worker_gate.clone(),
            Err(_) => return,
        };
        let generation = job.generation;
        let task_shared = Arc::clone(&shared);
        let task = runtime_handle.spawn(async move {
            let Some(_permit) = correction_worker_gate.acquire().await else {
                return;
            };
            run_speculative_cloud_correction(task_shared, None, job).await;
        });
        let mut task = Some(task);

        if let Ok(mut shared) = shared.lock() {
            if !shared.shutting_down
                && desktop_document_recorrection_generation_is_current(
                    shared.next_generation,
                    generation,
                )
            {
                if let Some((_, previous_task)) = shared.pending_document_correction_task.take() {
                    previous_task.abort();
                }
                shared.pending_document_correction_task =
                    Some((generation, task.take().expect("document correction task")));
            }
        }

        if let Some(task) = task {
            task.abort();
        }
    }

    fn live_streaming_correction_anchor_still_latest(
        shared: &Arc<Mutex<SharedState>>,
        guard: LatestLiveSegmentGuard,
        segment_id: &str,
    ) -> bool {
        let Ok(shared) = shared.lock() else {
            return false;
        };
        let Some(active) = shared.active_recording.as_ref() else {
            return false;
        };
        active.generation == guard.generation
            && desktop_streaming_latest_segment_allows_auto_patch(
                &active.live_streaming_inserted_segment_ids,
                segment_id,
            )
    }

    fn update_live_streaming_inserted_anchor_text(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
        segment_id: &str,
        corrected_text: &str,
    ) {
        let Ok(mut shared) = shared.lock() else {
            return;
        };
        let Some(active) = shared.active_recording.as_mut() else {
            return;
        };
        if active.generation != generation {
            return;
        }
        if let Some(anchor) = active.live_streaming_inserted_anchors.get_mut(segment_id) {
            anchor.inserted_text = corrected_text.to_string();
        }
    }

    fn live_streaming_correction_generation_is_active(
        shared: &Arc<Mutex<SharedState>>,
        guard: LatestLiveSegmentGuard,
    ) -> bool {
        let Ok(shared) = shared.lock() else {
            return false;
        };
        shared
            .active_recording
            .as_ref()
            .is_some_and(|active| active.generation == guard.generation)
    }

    fn live_streaming_unanchored_correction_is_current(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
        segment_id: &str,
    ) -> bool {
        let Ok(shared) = shared.lock() else {
            return false;
        };
        let active_generation = shared
            .active_recording
            .as_ref()
            .map(|active| active.generation);
        let already_inserted = shared.active_recording.as_ref().is_some_and(|active| {
            active
                .live_streaming_inserted_anchors
                .contains_key(segment_id)
        });
        desktop_live_correction_eligibility(active_generation, generation, already_inserted)
            == DesktopLiveCorrectionEligibility::Process
    }

    fn mirror_live_streaming_inserted_anchor(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
        anchor: SpeculativeInsertAnchor,
    ) {
        let Ok(mut shared) = shared.lock() else {
            return;
        };
        let Some(active) = shared.active_recording.as_mut() else {
            return;
        };
        if active.generation != generation {
            return;
        }
        if remove_hud_streaming_segments(
            Arc::make_mut(&mut active.hud_streaming_segments),
            std::slice::from_ref(&anchor.segment_id),
        ) {
            active.hud_streaming_segments_revision =
                active.hud_streaming_segments_revision.wrapping_add(1);
        }
        record_live_streaming_inserted_anchor(
            &mut active.live_streaming_inserted_segment_ids,
            &mut active.live_streaming_inserted_anchors,
            anchor,
        );
    }

    fn record_live_streaming_inserted_anchor(
        inserted_segment_ids: &mut Vec<String>,
        inserted_anchors: &mut HashMap<String, SpeculativeInsertAnchor>,
        anchor: SpeculativeInsertAnchor,
    ) {
        let segment_id = anchor.segment_id.clone();
        match inserted_anchors.entry(segment_id) {
            Entry::Occupied(mut entry) => {
                debug_assert!(inserted_segment_ids
                    .iter()
                    .any(|segment_id| segment_id == entry.key()));
                entry.insert(anchor);
            }
            Entry::Vacant(entry) => {
                debug_assert!(!inserted_segment_ids
                    .iter()
                    .any(|segment_id| segment_id == entry.key()));
                inserted_segment_ids.push(entry.key().clone());
                entry.insert(anchor);
            }
        }
    }

    fn insert_live_streaming_corrected_backlog_if_safe(
        config: &TalkConfig,
        hwnd: HWND,
        hud_hwnd: HWND,
        origin_insert_target: Option<&DesktopInsertTargetContext>,
        tracker: &Arc<LiveCorrectionTracker>,
    ) -> Vec<SpeculativeInsertAnchor> {
        let backlog = tracker.ordered_backlog();
        let mut anchors = Vec::new();
        for item in backlog {
            let anchor = match insert_live_streaming_segment_if_safe(
                config,
                hwnd,
                hud_hwnd,
                origin_insert_target,
                item.segment_id.as_str(),
                item.corrected_text.as_str(),
                DesktopTextLifecycleState::Corrected,
            ) {
                Ok(Some(anchor)) => anchor,
                Ok(None) => break,
                Err(error) => {
                    eprintln!(
                        "Talk corrected streaming backlog insert failed for {}: {error:#}",
                        item.segment_id
                    );
                    break;
                }
            };
            tracker.record_result(
                item.segment_id.as_str(),
                item.corrected_text.as_str(),
                Some(anchor.clone()),
            );
            anchors.push(anchor);
        }
        anchors
    }

    fn queue_live_streaming_corrected_hud(hwnd: HWND, generation: u64, _corrected_text: &str) {
        unsafe {
            let _ = PostMessageW(
                hwnd,
                STREAMING_CORRECTED_HUD_MESSAGE,
                generation as usize,
                0,
            );
        }
    }

    fn insert_live_streaming_segment_if_safe(
        config: &TalkConfig,
        hwnd: HWND,
        hud_hwnd: HWND,
        origin_insert_target: Option<&DesktopInsertTargetContext>,
        segment_id: &str,
        text: &str,
        lifecycle_state: DesktopTextLifecycleState,
    ) -> Result<Option<SpeculativeInsertAnchor>> {
        if config.output.mode != OutputMode::ClipboardPaste {
            return Ok(None);
        }
        if config.output.clipboard_backend != ClipboardBackendMode::NativeWindows {
            return Ok(None);
        }

        let current_context = capture_foreground_insert_target_context(hwnd, hud_hwnd);
        let event = SpeculativeRuntimeEvent::LocalSegmentCommitted {
            segment_id: segment_id.to_string(),
            text: text.to_string(),
        };
        let plan = live_streaming_segment_plan_for_lifecycle(
            config.output.mode,
            &event,
            origin_insert_target,
            current_context.as_ref(),
            lifecycle_state,
        );
        let DesktopLiveStreamingLocalSegmentPlan::Insert {
            insert_target,
            text,
            ..
        } = plan
        else {
            return Ok(None);
        };

        let process_name = resolve_window_process_base_name(insert_target.window_handle);
        let preferred_mode = desktop_preferred_paste_shortcut_for_target(
            &config.desktop.paste.shortcut_overrides,
            process_name.as_deref(),
            current_context.as_ref(),
        );
        let focus_handle = desktop_direct_control_paste_focus_handle(current_context.as_ref());

        if should_prepare_paste_shortcut_modifiers(focus_handle.is_some()) {
            let _modifier_preparation = prepare_paste_shortcut_modifier_state();
        }
        let restore_policy = if config.output.restore_clipboard {
            ClipboardRestorePolicy::RestoreOriginal
        } else {
            ClipboardRestorePolicy::LeaveInsertedText
        };
        let inserter = ClipboardPasteInserter::with_settle_delay(
            WindowsClipboardBackend,
            configured_windows_paste_shortcut(preferred_mode, focus_handle),
            restore_policy,
            Duration::from_millis(desktop_live_clipboard_settle_delay_ms()),
        )
        .with_deferred_restore();
        inserter.insert_text(&text)?;
        let anchor = SpeculativeInsertAnchor::new(
            insert_target.window_handle,
            insert_target.focus_handle,
            segment_id,
            text,
            0,
        )
        .map_err(|error| anyhow::anyhow!(error))?;
        Ok(Some(anchor))
    }

    fn speculative_correction_job_for_live_segment(
        config: &Arc<TalkConfig>,
        pipeline_config: &DesktopSpeculativePipelineConfig,
        event: &SpeculativeRuntimeEvent,
        requested_mode: VoiceMode,
        anchor: Option<&SpeculativeInsertAnchor>,
        origin_insert_target: Option<&DesktopInsertTargetContext>,
        latest_live_segment_guard: Option<LatestLiveSegmentGuard>,
        allow_target_apply: bool,
        generation: u64,
        hwnd_value: usize,
        hud_hwnd_value: usize,
    ) -> Option<SpeculativeCloudCorrectionJob> {
        let insert_target = anchor.map(|anchor| ForegroundInsertTarget {
            window_handle: anchor.window_handle,
            focus_handle: anchor.focus_handle,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        });
        let model = desktop_speculative_correction_job_model(
            pipeline_config,
            event,
            insert_target,
            anchor.map(|anchor| anchor.inserted_at_ms).unwrap_or(0),
        )?;
        let anchor = match model.output_target {
            DesktopSpeculativeCorrectionOutputTarget::PatchInsertedText(anchor) => Some(anchor),
            DesktopSpeculativeCorrectionOutputTarget::InsertCorrectedText => None,
            DesktopSpeculativeCorrectionOutputTarget::CopyPopupOnly => return None,
        };
        Some(SpeculativeCloudCorrectionJob {
            config: Arc::clone(config),
            segment_id: model.segment_id,
            transcript: model.local_text,
            context_before: Some(model.context_before),
            processing_mode: desktop_live_correction_processing_mode(),
            requested_mode,
            origin_insert_target: origin_insert_target.cloned(),
            anchor,
            full_document_inserted_segments: Vec::new(),
            session_log_path: None,
            latest_live_segment_guard,
            allow_target_apply,
            generation,
            started_at: Instant::now(),
            hwnd_value,
            hud_hwnd_value,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn speculative_correction_job_for_final_document(
        config: &Arc<TalkConfig>,
        segment_id: &str,
        transcript: String,
        requested_mode: VoiceMode,
        smart_routed_mode: Option<VoiceMode>,
        origin_insert_target: Option<&DesktopInsertTargetContext>,
        target: ForegroundInsertTarget,
        full_document_inserted_segments: Vec<String>,
        session_log_path: PathBuf,
        generation: u64,
        hwnd_value: usize,
        hud_hwnd_value: usize,
    ) -> Result<SpeculativeCloudCorrectionJob> {
        let anchor = SpeculativeInsertAnchor::new(
            target.window_handle,
            target.focus_handle,
            segment_id,
            transcript.clone(),
            0,
        )
        .map_err(|error| anyhow::anyhow!(error))?;

        Ok(SpeculativeCloudCorrectionJob {
            config: Arc::clone(config),
            segment_id: segment_id.to_string(),
            transcript,
            context_before: None,
            processing_mode: desktop_final_correction_processing_mode(
                requested_mode,
                smart_routed_mode,
            ),
            requested_mode,
            origin_insert_target: origin_insert_target.cloned(),
            anchor: Some(anchor),
            full_document_inserted_segments,
            session_log_path: Some(session_log_path),
            latest_live_segment_guard: None,
            allow_target_apply: true,
            generation,
            started_at: Instant::now(),
            hwnd_value,
            hud_hwnd_value,
        })
    }

    fn speculative_correction_unchanged_fast_path_allowed(
        job: &SpeculativeCloudCorrectionJob,
        corrected_text: &str,
    ) -> bool {
        if job.anchor.is_none() {
            return false;
        }
        if job.full_document_inserted_segments.is_empty() {
            return corrected_text == job.transcript;
        }

        corrected_text == job.full_document_inserted_segments.concat()
    }

    fn send_shift_left_selection(count: usize) -> Result<()> {
        if count == 0 {
            return Ok(());
        }

        let mut inputs = Vec::with_capacity(2 + count.saturating_mul(2));
        inputs.push(keyboard_input(VK_SHIFT, 0));
        for _ in 0..count {
            inputs.push(keyboard_input(VK_LEFT, 0));
            inputs.push(keyboard_input(VK_LEFT, KEYEVENTF_KEYUP));
        }
        inputs.push(keyboard_input(VK_SHIFT, KEYEVENTF_KEYUP));

        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                mem::size_of::<INPUT>() as i32,
            )
        };
        if sent != inputs.len() as u32 {
            anyhow::bail!(
                "SendInput(Shift+Left selection) selected {sent}/{} key events",
                inputs.len()
            );
        }
        Ok(())
    }

    fn send_ctrl_a_selection() -> Result<()> {
        let mut inputs = vec![
            keyboard_input(VK_CONTROL_KEY, 0),
            keyboard_input(VK_A_KEY, 0),
            keyboard_input(VK_A_KEY, KEYEVENTF_KEYUP),
            keyboard_input(VK_CONTROL_KEY, KEYEVENTF_KEYUP),
        ];
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                mem::size_of::<INPUT>() as i32,
            )
        };
        if sent != inputs.len() as u32 {
            anyhow::bail!(
                "SendInput(Ctrl+A selection) selected {sent}/{} key events",
                inputs.len()
            );
        }
        Ok(())
    }

    fn keyboard_input(vk: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    async fn finish_recording_in_blocking_worker(
        recording: RecordingSession,
    ) -> Result<talk_audio::AudioArtifact> {
        Ok(tokio::task::spawn_blocking(move || recording.finish())
            .await
            .map_err(|error| anyhow::anyhow!("Talk recording finalize worker failed: {error}"))??)
    }

    fn request_stop_recording(hwnd: HWND, generation: u64) {
        if cancel_pending_recording_begin(hwnd, Some(generation)).unwrap_or(false) {
            return;
        }
        let state = match unsafe { get_window_state(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let (config, runtime_handle, mut active, foreground_apply_gate) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            let Some(active) = shared.active_recording.take() else {
                return;
            };
            if active.generation != generation {
                shared.active_recording = Some(active);
                return;
            }
            shared.shell_state = shared.shell_state.set_busy();
            shared.worker_generation = Some(generation);
            shared.current_phase = Some(RuntimePhase::Transcribing);
            let config = shared.config.clone();
            if config.is_none() {
                shared.shell_state = shared.shell_state.complete();
                shared.current_phase = None;
                shared.worker_generation = None;
            }
            (
                config,
                shared.runtime_handle.clone(),
                active,
                shared.foreground_apply_gate.clone(),
            )
        };
        cancel_recording_timeout_timer(hwnd);
        let live_correction_tracker = Arc::clone(&active.live_streaming_correction_tracker);
        let Some(config) = config else {
            let error_message = "Talk config unavailable while stopping recording".to_string();
            live_correction_tracker.cancel();
            active.live_streaming_correction_sender.take();
            set_failed_last_session_if_current(&state.shared, generation, error_message.clone());
            spawn_recording_source_cancel_worker(
                &runtime_handle,
                active.source,
                "missing stop configuration",
            );
            let _ = show_hud_text(
                hwnd,
                &compose_hud_message("Talk: failed", Some(&error_message)),
                Some(1800),
            );
            let _ = refresh_idle_tray_status(hwnd);
            return;
        };

        if let Err(error) = active.session.apply(VoiceEvent::TriggerStop) {
            let error_message = error.to_string();
            live_correction_tracker.cancel();
            active.live_streaming_correction_sender.take();
            let ActiveRecording {
                session,
                trigger_events,
                mode_override,
                source,
                ..
            } = active;
            spawn_recording_source_cancel_worker(
                &runtime_handle,
                source,
                "stop transition failure",
            );
            set_failed_last_session_if_current(&state.shared, generation, error_message.clone());
            spawn_failed_session_persistence_worker(
                &runtime_handle,
                Some(config.clone()),
                session,
                trigger_events,
                mode_override,
                anyhow::Error::new(error),
                "stop transition failure",
            );
            let _ = mark_idle_after_terminal_state(hwnd);
            return;
        }
        active.trigger_events.push("trigger_stop");
        let live_correction_sender = active.live_streaming_correction_sender.take();
        let mut speculative_runtime_state = std::mem::take(&mut active.speculative_runtime_state);
        let speculative_segmenter_config = active.speculative_segmenter_config;
        let existing_live_streaming_anchors =
            std::mem::take(&mut active.live_streaming_inserted_anchors);
        let mut hud_streaming_segments = std::mem::take(&mut active.hud_streaming_segments);

        if show_hud_text(
            hwnd,
            hud_message_for_phase(RuntimePhase::Transcribing),
            None,
        )
        .is_ok()
        {
            let _ = update_tray_icon(hwnd, "Talk: transcribing");
        }

        let speculative_pipeline_config = desktop_speculative_pipeline_config(&config);
        let speculative_local_asr_route =
            desktop_speculative_local_asr_route(&speculative_pipeline_config);
        let use_external_speculative_asr =
            speculative_local_asr_route == DesktopSpeculativeLocalAsrRoute::ExternalCommand;
        let use_streaming_speculative_asr = active.use_streaming_speculative_asr;

        let stopped_source = match active.source {
            ActiveRecordingSource::Live {
                recording,
                streaming_pump,
            } if use_streaming_speculative_asr => StoppedRecordingSource::StreamingRecording {
                recording,
                streaming_pump,
            },
            ActiveRecordingSource::Live {
                recording,
                streaming_pump,
            } => StoppedRecordingSource::LiveRecording {
                recording,
                streaming_pump,
            },
            ActiveRecordingSource::ExplicitAudioFile(audio_path)
                if use_streaming_speculative_asr =>
            {
                let error = "streaming_service local ASR requires a live recording source";
                live_correction_tracker.cancel();
                set_failed_last_session_if_current(&state.shared, generation, error.to_string());
                spawn_failed_session_persistence_worker(
                    &runtime_handle,
                    Some(config.clone()),
                    active.session,
                    active.trigger_events,
                    active.mode_override,
                    anyhow::anyhow!(error),
                    "explicit audio streaming route rejection",
                );
                let _ = show_hud_text(
                    hwnd,
                    &compose_hud_message("Talk: failed", Some(error)),
                    Some(1800),
                );
                let _ = mark_idle_after_terminal_state(hwnd);
                drop(audio_path);
                return;
            }
            ActiveRecordingSource::ExplicitAudioFile(audio_path) => {
                StoppedRecordingSource::AudioFile(audio_path)
            }
        };

        if speculative_local_asr_route == DesktopSpeculativeLocalAsrRoute::MockPreview {
            if let Err(error) = show_mock_speculative_preview(hwnd, &config) {
                eprintln!("Talk mock speculative preview failed: {error:#}");
            }
        } else if speculative_local_asr_route == DesktopSpeculativeLocalAsrRoute::Unsupported {
            live_correction_tracker.cancel();
            let shared = Arc::clone(&state.shared);
            let worker_registration_shared = Arc::clone(&shared);
            let hwnd_value = hwnd as usize;
            let error_message = format!(
                "unsupported speculative.local_asr value: {}",
                speculative_pipeline_config.local_asr
            );
            let worker_task = runtime_handle.spawn(async move {
                match stopped_source {
                    StoppedRecordingSource::LiveRecording {
                        recording,
                        streaming_pump,
                    }
                    | StoppedRecordingSource::StreamingRecording {
                        recording,
                        streaming_pump,
                    } => {
                        if let Some(streaming_pump) = streaming_pump {
                            if let Err(cancel_error) = streaming_pump.cancel().await {
                                eprintln!(
                                    "Talk local streaming ASR cancel failed for unsupported route: {cancel_error:#}"
                                );
                            }
                        }
                        match tokio::task::spawn_blocking(move || recording.cancel()).await {
                            Ok(Ok(())) => {}
                            Ok(Err(cancel_error)) => eprintln!(
                                "Talk recording cancel failed for unsupported route: {cancel_error}"
                            ),
                            Err(cancel_error) => eprintln!(
                                "Talk recording cancel worker failed for unsupported route: {cancel_error}"
                            ),
                        }
                    }
                    StoppedRecordingSource::AudioFile(_)
                    | StoppedRecordingSource::StreamingEvents { .. }
                    | StoppedRecordingSource::RecordingFinalizeFailed(_) => {}
                }
                let result = complete_failed_session_with_mode_override(
                    &config,
                    active.session,
                    active.trigger_events,
                    active.mode_override,
                    anyhow::anyhow!(error_message.clone()),
                    false,
                    |phase| unsafe {
                        let _ = PostMessageW(
                            hwnd_value as HWND,
                            PHASE_MESSAGE,
                            runtime_phase_to_code(phase) as usize,
                            generation as isize,
                        );
                    },
                );

                if let Ok(mut shared) = shared.lock() {
                    if !stop_worker_side_effects_allowed(
                        shared.shutting_down,
                        shared.worker_generation,
                        shared.next_generation,
                        generation,
                    ) {
                        return;
                    }
                    match result {
                        Ok(report) => {
                            set_last_session(
                                &mut shared,
                                "failed",
                                report.session.error().map(str::to_string),
                            );
                            shared.pending_worker_error = Some((generation, error_message));
                        }
                        Err(error) => {
                            shared.pending_worker_error = Some((generation, error.to_string()));
                            set_last_session(&mut shared, "failed", Some(error.to_string()));
                        }
                    }
                }

                unsafe {
                    let _ = PostMessageW(
                        hwnd_value as HWND,
                        WORKER_DONE_MESSAGE,
                        generation as usize,
                        0,
                    );
                }
            });
            register_pending_worker_task(&worker_registration_shared, generation, worker_task);
            return;
        }
        let external_asr_command = if use_external_speculative_asr {
            Some(
                config
                    .speculative
                    .external_asr_command
                    .clone()
                    .unwrap_or_default(),
            )
        } else {
            None
        };
        let cloud_correction_after_local_insert = (use_external_speculative_asr
            || use_streaming_speculative_asr)
            && desktop_speculative_cloud_correction_enabled(&speculative_pipeline_config)
            && provider_text_processing_credentials_available(&config);
        let local_asr_correction_segment_id = if use_streaming_speculative_asr {
            "streaming-service-final"
        } else {
            "external-asr-final"
        };

        if let Ok(mut shared) = state.shared.lock() {
            if shared.worker_generation == Some(generation) {
                shared.pending_stop_live_correction_tracker =
                    Some(Arc::clone(&live_correction_tracker));
            }
        }

        let shared = Arc::clone(&state.shared);
        let worker_registration_shared = Arc::clone(&shared);
        let correction_runtime_handle = runtime_handle.clone();
        let hwnd_value = hwnd as usize;
        let hud_hwnd_value = state.hud_hwnd.get() as usize;
        let output_mode = config.output.mode;
        let mode_override = active.mode_override;
        let origin_insert_target = resolve_hotkey_origin_insert_target(
            active.origin_insert_target.as_ref(),
            active.release_time_origin_insert_target.as_ref(),
        );
        let paste_shortcut_overrides = config.desktop.paste.shortcut_overrides.clone();
        let origin_insert_target_for_before_hook = origin_insert_target.clone();
        let origin_insert_target_for_report = origin_insert_target.clone();
        let origin_insert_target_source = if origin_insert_target != active.origin_insert_target
            && origin_insert_target == active.release_time_origin_insert_target
        {
            Some("hotkey_release_time_upgrade".to_string())
        } else {
            active.origin_insert_target_source.clone()
        };
        let pending_hotkey_origin_insert_target =
            active.pending_hotkey_origin_insert_target.clone();
        let release_time_origin_insert_target = active.release_time_origin_insert_target.clone();
        let restore_diagnostic = Arc::new(Mutex::new(None::<DesktopInsertTargetRestoreDiagnostic>));
        let restored_insert_target = Arc::new(Mutex::new(None::<ForegroundInsertTarget>));
        let selected_output_strategy = Arc::new(Mutex::new(None::<DesktopOutputStrategy>));
        let selected_show_result_in_gui = Arc::new(Mutex::new(false));
        let captured_insert_target_context =
            Arc::new(Mutex::new(None::<DesktopInsertTargetContext>));
        let paste_override_restore = Arc::new(Mutex::new(None::<WindowsPasteOverrides>));
        let foreground_apply_lease = Arc::new(Mutex::new(None::<ForegroundApplyLease>));
        let runtime_voice_mode = mode_override.unwrap_or_else(|| config.default_voice_mode());
        let streaming_session_id = active.session.id().to_string();
        let restore_diagnostic_for_before_hook = Arc::clone(&restore_diagnostic);
        let restore_diagnostic_for_after_hook = Arc::clone(&restore_diagnostic);
        let restored_insert_target_for_before_hook = Arc::clone(&restored_insert_target);
        let restored_insert_target_for_after_hook = Arc::clone(&restored_insert_target);
        let selected_output_strategy_for_before_hook = Arc::clone(&selected_output_strategy);
        let selected_output_strategy_for_report = Arc::clone(&selected_output_strategy);
        let selected_show_result_in_gui_for_before_hook = Arc::clone(&selected_show_result_in_gui);
        let selected_show_result_in_gui_for_report = Arc::clone(&selected_show_result_in_gui);
        let captured_insert_target_context_for_before_hook =
            Arc::clone(&captured_insert_target_context);
        let captured_insert_target_context_for_report = Arc::clone(&captured_insert_target_context);
        let paste_override_restore_for_before_hook = Arc::clone(&paste_override_restore);
        let paste_override_restore_for_after_hook = Arc::clone(&paste_override_restore);
        let foreground_apply_gate_for_before_hook = foreground_apply_gate.clone();
        let foreground_apply_gate_for_stop_dispatch = foreground_apply_gate.clone();
        let foreground_apply_gate_for_stop_tail = foreground_apply_gate.clone();
        let foreground_apply_lease_for_before_hook = Arc::clone(&foreground_apply_lease);
        let foreground_apply_lease_for_after_hook = Arc::clone(&foreground_apply_lease);
        let shared_for_before_hook = Arc::clone(&shared);
        let worker_task = runtime_handle.spawn(async move {
            let mut final_runtime_segment_id = None::<String>;
            let mut recording_finalize_error = None::<String>;
            let stopped_source = match stopped_source {
                StoppedRecordingSource::LiveRecording {
                    recording,
                    streaming_pump,
                } => {
                    if let Some(streaming_pump) = streaming_pump {
                        if let Err(cancel_error) = streaming_pump.cancel().await {
                            eprintln!(
                                "Talk local streaming ASR cancel failed after route change: {cancel_error:#}"
                            );
                        }
                    }
                    match finish_recording_in_blocking_worker(recording).await {
                        Ok(artifact) => StoppedRecordingSource::AudioFile(artifact.path),
                        Err(error) => {
                            let error_message = error.to_string();
                            live_correction_tracker.cancel();
                            recording_finalize_error = Some(error_message.clone());
                            StoppedRecordingSource::RecordingFinalizeFailed(error_message)
                        }
                    }
                }
                StoppedRecordingSource::StreamingRecording {
                    recording,
                    streaming_pump,
                } => {
                    let events_result = if let Some(streaming_pump) = streaming_pump {
                        streaming_pump.stop().await
                    } else {
                        run_local_streaming_asr_service_from_recording(
                            &config,
                            &streaming_session_id,
                            &recording,
                            None,
                        )
                        .await
                    };
                    let audio_path = match finish_recording_in_blocking_worker(recording).await {
                        Ok(artifact) => Some(artifact.path),
                        Err(error) => {
                            eprintln!(
                                "Talk final streaming recording finalize failed; provider audio fallback disabled for this stop: {error}"
                            );
                            None
                        }
                    };

                    if let Ok(events) = &events_result {
                        let mut runtime_events = Vec::new();
                        for event in events.iter().cloned() {
                            match speculative_runtime_state.accept_asr_event_with_segmentation(
                                event,
                                0,
                                &speculative_segmenter_config,
                            ) {
                                Ok(events) => runtime_events.extend(events),
                                Err(error) => {
                                    eprintln!(
                                        "Talk final streaming ASR event segmentation failed: {error}"
                                    );
                                }
                            }
                        }
                        for event in &runtime_events {
                            match event {
                                SpeculativeRuntimeEvent::DraftUpdated { segment_id, text }
                                | SpeculativeRuntimeEvent::LocalSegmentCommitted {
                                    segment_id,
                                    text,
                                } => {
                                    upsert_hud_streaming_segment(
                                        Arc::make_mut(&mut hud_streaming_segments),
                                        segment_id,
                                        text,
                                    );
                                    if matches!(
                                        event,
                                        SpeculativeRuntimeEvent::LocalSegmentCommitted { .. }
                                    ) {
                                        final_runtime_segment_id = Some(segment_id.clone());
                                    }
                                }
                                SpeculativeRuntimeEvent::LocalSegmentsInvalidated { segment_ids } => {
                                    remove_hud_streaming_segments(
                                        Arc::make_mut(&mut hud_streaming_segments),
                                        segment_ids,
                                    );
                                }
                                SpeculativeRuntimeEvent::CorrectionRequested { .. } => {}
                            }
                        }
                        if !runtime_events.is_empty() {
                            if let Some(sender) = live_correction_sender.as_ref() {
                                let _foreground_apply_lease =
                                    foreground_apply_gate_for_stop_dispatch.acquire();
                                if !stop_worker_side_effects_allowed_for_shared(
                                    &shared,
                                    generation,
                                ) {
                                    return;
                                }
                                let _ = dispatch_live_streaming_events(PendingLiveStreamingDispatch {
                                    config: config.clone(),
                                    pipeline_config: speculative_pipeline_config.clone(),
                                    generation,
                                    origin_insert_target: origin_insert_target.clone(),
                                    existing_anchors: existing_live_streaming_anchors.clone(),
                                    live_correction_sender: sender.clone(),
                                    live_correction_tracker: Arc::clone(
                                        &live_correction_tracker,
                                    ),
                                    events: runtime_events,
                                    requested_mode: runtime_voice_mode,
                                    smart_routed_mode: None,
                                    flush_corrected_backlog: false,
                                    latest_live_segment_guard: None,
                                    allow_target_apply: false,
                                    hwnd_value,
                                    hud_hwnd_value,
                                });
                            }
                        }
                    }

                    StoppedRecordingSource::StreamingEvents {
                        events: events_result,
                        audio_path,
                    }
                }
                source => source,
            };

            live_correction_tracker.mark_stopping();
            drop(live_correction_sender);
            let correction_policy = desktop_live_correction_worker_policy();
            if tokio::time::timeout(
                Duration::from_millis(correction_policy.drain_timeout_ms),
                live_correction_tracker.wait_until_idle(),
            )
            .await
            .is_err()
            {
                live_correction_tracker.cancel();
                eprintln!(
                    "Talk live correction drain exceeded {} ms; continuing with completed segments",
                    correction_policy.drain_timeout_ms
                );
            }
            let tracker_snapshot = live_correction_tracker.snapshot();
            let mut live_inserted_anchors_for_stop = tracker_snapshot
                .iter()
                .filter_map(|segment| segment.insert_anchor.clone())
                .collect::<Vec<_>>();
            let mut live_inserted_baseline_for_stop =
                desktop_live_correction_inserted_baseline(&tracker_snapshot);
            let streaming_stop_policy =
                desktop_streaming_stop_policy(live_inserted_anchors_for_stop.len());
            let insert_final_transcript_at_stop = streaming_stop_policy.insert_final_transcript;

            let before_insert = move |insert_context: &talk_runtime::RuntimeInsertContext| {
                if !insert_final_transcript_at_stop {
                    return RuntimeInsertDirective::DryRunOnly;
                }
                let lease = foreground_apply_gate_for_before_hook.acquire();
                let Ok(mut lease_slot) = foreground_apply_lease_for_before_hook.lock() else {
                    return RuntimeInsertDirective::DryRunOnly;
                };
                *lease_slot = Some(lease);
                drop(lease_slot);
                if !stop_worker_side_effects_allowed_for_shared(
                    &shared_for_before_hook,
                    generation,
                ) {
                    return RuntimeInsertDirective::DryRunOnly;
                }

                let current_context = capture_foreground_insert_target_context(
                    hwnd_value as HWND,
                    hud_hwnd_value as HWND,
                );
                if let Ok(mut slot) = captured_insert_target_context_for_before_hook.lock() {
                    *slot = current_context.clone();
                }
                let resolved_origin_target_for_insert = resolve_hotkey_recording_origin_enrichment(
                    origin_insert_target_for_before_hook.as_ref(),
                    current_context.as_ref(),
                );
                let output_plan = desktop_output_plan(
                    output_mode,
                    resolved_origin_target_for_insert.as_ref(),
                    current_context.as_ref(),
                );
                if let Ok(mut slot) = selected_output_strategy_for_before_hook.lock() {
                    *slot = Some(output_plan.strategy);
                }
                let insert_plan = desktop_runtime_insert_directive_for_mode(
                    runtime_voice_mode,
                    insert_context.smart_routed_mode,
                    output_plan.strategy,
                    DesktopTextLifecycleState::Corrected,
                );
                if let Ok(mut slot) = selected_show_result_in_gui_for_before_hook.lock() {
                    *slot = insert_plan.show_result_in_gui;
                }

                if insert_plan.directive == DesktopRuntimeInsertDirective::DryRunOnly {
                    return RuntimeInsertDirective::DryRunOnly;
                }

                if let Ok(mut shared) = shared_for_before_hook.lock() {
                    shared.pending_corrected_hud = Some(PendingCorrectedHud {
                        generation,
                        text: insert_context.output_text.clone(),
                    });
                }
                unsafe {
                    let _ = PostMessageW(
                        hwnd_value as HWND,
                        CORRECTED_HUD_MESSAGE,
                        generation as usize,
                        0,
                    );
                }

                let mut restored_context = None::<DesktopInsertTargetContext>;
                if output_mode == OutputMode::ClipboardPaste
                    && output_plan.strategy == DesktopOutputStrategy::HonorConfiguredOutput
                {
                    if let Some(target) = output_plan.insert_target {
                        if desktop_insert_target_restore_requested(target, current_context.as_ref())
                        {
                            let (effective_target, diagnostic) =
                                begin_restore_foreground_insert_target(
                                    target,
                                    hwnd_value as HWND,
                                    hud_hwnd_value as HWND,
                                );
                            if let Ok(mut slot) = restored_insert_target_for_before_hook.lock() {
                                *slot = Some(effective_target);
                            }
                            if let Ok(mut slot) = restore_diagnostic_for_before_hook.lock() {
                                *slot = Some(diagnostic);
                            }
                            restored_context = capture_foreground_insert_target_context(
                                hwnd_value as HWND,
                                hud_hwnd_value as HWND,
                            );
                        }
                    }

                    let matched_insert_context = desktop_target_matched_context_for_paste(
                        output_plan.insert_target,
                        current_context.as_ref(),
                        restored_context.as_ref(),
                    );
                    let preferred_mode = desktop_preferred_paste_shortcut_for_target(
                        &paste_shortcut_overrides,
                        output_plan
                            .insert_target
                            .and_then(|target| {
                                resolve_window_process_base_name(target.window_handle)
                            })
                            .as_deref(),
                        matched_insert_context,
                    );
                    let focus_handle =
                        desktop_direct_control_paste_focus_handle(matched_insert_context);
                    let overrides =
                        windows_paste_overrides_for_target(preferred_mode, focus_handle);
                    if overrides != WindowsPasteOverrides::default() {
                        if let Ok(mut slot) = paste_override_restore_for_before_hook.lock() {
                            if slot.is_none() {
                                *slot = Some(set_windows_paste_thread_overrides(overrides));
                            } else {
                                let _ = set_windows_paste_thread_overrides(overrides);
                            }
                        } else {
                            let _ = set_windows_paste_thread_overrides(overrides);
                        }
                    }

                    if should_prepare_paste_shortcut_modifiers(focus_handle.is_some()) {
                        let _modifier_preparation = prepare_paste_shortcut_modifier_state();
                    }
                }
                RuntimeInsertDirective::UseConfiguredOutput
            };
            let after_insert = move || {
                if let Ok(mut slot) = paste_override_restore_for_after_hook.lock() {
                    if let Some(previous) = slot.take() {
                        let _ = set_windows_paste_thread_overrides(previous);
                    }
                }

                let effective_target = restored_insert_target_for_after_hook
                    .lock()
                    .ok()
                    .and_then(|slot| *slot);
                if let Some(target) = effective_target {
                    let restore_applied = restore_diagnostic_for_after_hook
                        .lock()
                        .ok()
                        .and_then(|slot| slot.as_ref().map(|diagnostic| diagnostic.restore_applied))
                        .unwrap_or(true);
                    let post_insert_hold =
                        end_restore_foreground_insert_target(target, restore_applied);
                    if let Ok(mut slot) = restore_diagnostic_for_after_hook.lock() {
                        if let Some(diagnostic) = slot.as_mut() {
                            diagnostic.post_insert_release_reason =
                                Some(post_insert_hold.release_reason);
                            diagnostic.post_insert_wait_duration_ms =
                                Some(post_insert_hold.wait_duration_ms);
                            diagnostic.post_insert_poll_count =
                                Some(post_insert_hold.progress.poll_count);
                            diagnostic.post_insert_target_foreground_poll_count =
                                Some(post_insert_hold.progress.target_foreground_poll_count);
                            diagnostic.post_insert_trailing_target_foreground_poll_count = Some(
                                post_insert_hold
                                    .progress
                                    .trailing_target_foreground_poll_count,
                            );
                            diagnostic.post_insert_required_stable_foreground_polls =
                                Some(post_insert_hold.required_stable_foreground_polls);
                        }
                    }
                }

                if let Ok(mut lease_slot) = foreground_apply_lease_for_after_hook.lock() {
                    lease_slot.take();
                }
            };
            let phase_callback = |phase| unsafe {
                let _ = PostMessageW(
                    hwnd_value as HWND,
                    PHASE_MESSAGE,
                    runtime_phase_to_code(phase) as usize,
                    generation as isize,
                );
            };
            let session = active.session;
            let trigger_events = active.trigger_events;
            let result = match (
                external_asr_command,
                use_streaming_speculative_asr,
                stopped_source,
            ) {
                (Some(external_asr_command), _, StoppedRecordingSource::AudioFile(audio_path)) => {
                    run_voice_session_from_external_asr_command_with_insert_hooks(
                        &config,
                        session,
                        trigger_events,
                        audio_path,
                        external_asr_command,
                        mode_override,
                        FrontContext::default(),
                        before_insert,
                        after_insert,
                        phase_callback,
                    )
                    .await
                }
                (
                    None,
                    true,
                    StoppedRecordingSource::StreamingEvents { events, audio_path },
                ) => {
                    match events {
                        Ok(events) => {
                            let selected_event = events
                                .iter()
                                .rev()
                                .find(|event| event.is_final())
                                .or_else(|| events.last())
                                .cloned();
                            if let Some(selected_event) = selected_event {
                                let transcript =
                                    final_transcript_from_streaming_asr_events(&events)
                                        .unwrap_or_else(|_| selected_event.text().to_string());
                                let pending_segments = hud_streaming_segments
                                    .iter()
                                    .map(|(segment_id, text)| (segment_id.as_str(), text.as_str()))
                                    .collect::<Vec<_>>();
                                let route_evidence = SmartRouteEvidence {
                                    committed_streaming_segment_count:
                                        desktop_streaming_effective_segment_count(
                                            &tracker_snapshot,
                                            &pending_segments,
                                        ),
                                };
                                let mut session_transcript =
                                    desktop_streaming_stop_aggregate_with_pending(
                                    &tracker_snapshot,
                                    &pending_segments,
                                    final_runtime_segment_id
                                        .as_deref()
                                        .unwrap_or(selected_event.segment_id()),
                                    selected_event.text(),
                                );
                                if session_transcript.trim().is_empty() {
                                    session_transcript = transcript.clone();
                                }
                                let provider_text_processing_available =
                                    provider_text_processing_credentials_available(&config);
                                if !streaming_stop_policy.insert_final_transcript {
                                    let reconciliation_plan =
                                        desktop_streaming_stop_reconciliation_plan(
                                            &tracker_snapshot,
                                            &session_transcript,
                                        );
                                    match reconciliation_plan {
                                        DesktopStreamingStopReconciliationPlan::InsertTail(
                                            tail_text,
                                        ) => {
                                            let _foreground_apply_lease =
                                                foreground_apply_gate_for_stop_tail.acquire();
                                            if !stop_worker_side_effects_allowed_for_shared(
                                                &shared,
                                                generation,
                                            ) {
                                                return;
                                            }
                                            let current_context =
                                                capture_foreground_insert_target_context(
                                                    hwnd_value as HWND,
                                                    hud_hwnd_value as HWND,
                                                );
                                            let target_still_safe = desktop_output_plan(
                                                output_mode,
                                                origin_insert_target.as_ref(),
                                                current_context.as_ref(),
                                            )
                                            .insert_target
                                            .is_some();
                                            let current_target_text = current_context
                                                .as_ref()
                                                .and_then(capture_current_insert_target_text);
                                            if !desktop_streaming_stop_tail_target_unchanged(
                                                &live_inserted_baseline_for_stop,
                                                current_target_text.as_deref(),
                                                target_still_safe,
                                            ) {
                                                if let Ok(mut shared) = shared.lock() {
                                                    shared.pending_copy_popup =
                                                        Some(PendingCopyPopup {
                                                            generation,
                                                            model: desktop_copy_popup_model(
                                                                &session_transcript,
                                                            ),
                                                        });
                                                }
                                                unsafe {
                                                    let _ = PostMessageW(
                                                        hwnd_value as HWND,
                                                        CORRECTION_COPY_POPUP_MESSAGE,
                                                        generation as usize,
                                                        0,
                                                    );
                                                }
                                            } else {
                                                match insert_live_streaming_segment_if_safe(
                                                &config,
                                                hwnd_value as HWND,
                                                hud_hwnd_value as HWND,
                                                origin_insert_target.as_ref(),
                                                selected_event.segment_id(),
                                                &tail_text,
                                                DesktopTextLifecycleState::Corrected,
                                                ) {
                                                    Ok(Some(anchor)) => {
                                                        live_inserted_baseline_for_stop
                                                            .push(anchor.inserted_text.clone());
                                                        live_inserted_anchors_for_stop.push(anchor);
                                                    }
                                                    Ok(None) => {
                                                        if let Ok(mut shared) = shared.lock() {
                                                            shared.pending_copy_popup =
                                                                Some(PendingCopyPopup {
                                                                    generation,
                                                                    model: desktop_copy_popup_model(
                                                                        &session_transcript,
                                                                    ),
                                                                });
                                                        }
                                                        unsafe {
                                                            let _ = PostMessageW(
                                                                hwnd_value as HWND,
                                                                CORRECTION_COPY_POPUP_MESSAGE,
                                                                generation as usize,
                                                                0,
                                                            );
                                                        }
                                                    }
                                                    Err(error) => {
                                                        eprintln!(
                                                            "Talk streaming stop tail insert failed: {error:#}"
                                                        );
                                                        if let Ok(mut shared) = shared.lock() {
                                                            shared.pending_copy_popup =
                                                                Some(PendingCopyPopup {
                                                                    generation,
                                                                    model: desktop_copy_popup_model(
                                                                        &session_transcript,
                                                                    ),
                                                                });
                                                        }
                                                        unsafe {
                                                            let _ = PostMessageW(
                                                                hwnd_value as HWND,
                                                                CORRECTION_COPY_POPUP_MESSAGE,
                                                                generation as usize,
                                                                0,
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        DesktopStreamingStopReconciliationPlan::ReconcileWholeDocument {
                                            inserted_segments,
                                            replacement_text,
                                        } => {
                                            let _foreground_apply_lease =
                                                foreground_apply_gate_for_stop_tail.acquire();
                                            if !stop_worker_side_effects_allowed_for_shared(
                                                &shared,
                                                generation,
                                            ) {
                                                return;
                                            }
                                            let reconciled = match apply_document_recorrection_patch_if_current(
                                                &shared,
                                                generation,
                                                hwnd_value as HWND,
                                                hud_hwnd_value as HWND,
                                                origin_insert_target.as_ref(),
                                                &inserted_segments,
                                                &replacement_text,
                                                config.output.restore_clipboard,
                                            ) {
                                                Ok(reconciled) => reconciled,
                                                Err(error) => {
                                                    eprintln!(
                                                        "Talk streaming stop document reconciliation failed: {error:#}"
                                                    );
                                                    false
                                                }
                                            };
                                            if reconciled {
                                                live_inserted_baseline_for_stop =
                                                    vec![replacement_text];
                                            } else {
                                                if let Ok(mut shared) = shared.lock() {
                                                    shared.pending_copy_popup =
                                                        Some(PendingCopyPopup {
                                                            generation,
                                                            model: desktop_copy_popup_model(
                                                                &session_transcript,
                                                            ),
                                                        });
                                                }
                                                unsafe {
                                                    let _ = PostMessageW(
                                                        hwnd_value as HWND,
                                                        CORRECTION_COPY_POPUP_MESSAGE,
                                                        generation as usize,
                                                        0,
                                                    );
                                                }
                                            }
                                        }
                                        DesktopStreamingStopReconciliationPlan::NoAction => {}
                                    }
                                }

                                if provider_text_processing_available {
                                    let mock_text = if config.provider.kind == ProviderKind::Mock {
                                        Some(session_transcript.clone())
                                    } else {
                                        None
                                    };
                                    if let Some(audio_path) = audio_path.clone() {
                                        run_voice_session_from_audio_artifact_with_route_evidence_and_insert_hooks(
                                            &config,
                                            session,
                                            trigger_events,
                                            audio_path,
                                            mock_text,
                                            mode_override,
                                            route_evidence,
                                            FrontContext::default(),
                                            before_insert,
                                            after_insert,
                                            phase_callback,
                                        )
                                        .await
                                    } else {
                                        run_voice_session_from_transcript_with_route_evidence_and_insert_hooks(
                                            &config,
                                            session,
                                            trigger_events,
                                            session_transcript,
                                            mode_override,
                                            route_evidence,
                                            FrontContext::default(),
                                            before_insert,
                                            after_insert,
                                            phase_callback,
                                        )
                                        .await
                                    }
                                } else {
                                    run_voice_session_from_local_transcript_with_route_evidence_and_insert_hooks(
                                        &config,
                                        session,
                                        trigger_events,
                                        session_transcript,
                                        mode_override,
                                        route_evidence,
                                        before_insert,
                                        after_insert,
                                        phase_callback,
                                    )
                                }
                            } else {
                                complete_failed_session_with_mode_override(
                                    &config,
                                    session,
                                    trigger_events,
                                    mode_override,
                                    anyhow::anyhow!(
                                        "external streaming ASR command produced no events"
                                    ),
                                    false,
                                    phase_callback,
                                )
                            }
                        }
                        Err(error) => complete_failed_session_with_mode_override(
                            &config,
                            session,
                            trigger_events,
                            mode_override,
                            error,
                            false,
                            phase_callback,
                        ),
                    }
                }
                (None, false, StoppedRecordingSource::AudioFile(audio_path)) => {
                    run_voice_session_from_audio_artifact_with_insert_hooks(
                        &config,
                        session,
                        trigger_events,
                        audio_path,
                        None,
                        mode_override,
                        FrontContext::default(),
                        before_insert,
                        after_insert,
                        phase_callback,
                    )
                    .await
                }
                (
                    _,
                    _,
                    StoppedRecordingSource::LiveRecording {
                        recording,
                        streaming_pump,
                    }
                    | StoppedRecordingSource::StreamingRecording {
                        recording,
                        streaming_pump,
                    },
                ) => {
                    if let Some(streaming_pump) = streaming_pump {
                        let _ = streaming_pump.cancel().await;
                    }
                    let _ = recording.cancel();
                    complete_failed_session_with_mode_override(
                        &config,
                        session,
                        trigger_events,
                        mode_override,
                        anyhow::anyhow!(
                            "streaming recording source was selected for a non-streaming ASR route"
                        ),
                        false,
                        phase_callback,
                    )
                }
                (_, _, StoppedRecordingSource::RecordingFinalizeFailed(error)) => {
                    complete_failed_session_with_mode_override(
                        &config,
                        session,
                        trigger_events,
                        mode_override,
                        anyhow::anyhow!(error),
                        false,
                        phase_callback,
                    )
                }
                (_, _, StoppedRecordingSource::StreamingEvents { .. }) => {
                    complete_failed_session_with_mode_override(
                        &config,
                        session,
                        trigger_events,
                        mode_override,
                        anyhow::anyhow!(
                            "streaming ASR events were received for a non-streaming ASR route"
                        ),
                        false,
                        phase_callback,
                    )
                }
                (_, true, StoppedRecordingSource::AudioFile(_)) =>
                    complete_failed_session_with_mode_override(
                    &config,
                    session,
                    trigger_events,
                    mode_override,
                    anyhow::anyhow!("streaming_service local ASR requires a live recording source"),
                    false,
                    phase_callback,
                ),
            };

            let mut correction_job = None;
            if let Ok(mut shared) = shared.lock() {
                if !stop_worker_side_effects_allowed(
                    shared.shutting_down,
                    shared.worker_generation,
                    shared.next_generation,
                    generation,
                ) {
                    return;
                }
                match result {
                    Ok(report) => {
                        if report.session.status() != talk_core::SessionStatus::Completed {
                            shared.pending_corrected_hud = None;
                        }
                        let captured_context = captured_insert_target_context_for_report
                            .lock()
                            .ok()
                            .and_then(|slot| slot.clone());
                        let persisted_insert_target = restored_insert_target
                            .lock()
                            .ok()
                            .and_then(|slot| *slot)
                            .or_else(|| {
                                captured_context.as_ref().and_then(|context| context.target)
                            })
                            .or_else(|| {
                                live_inserted_anchors_for_stop.last().map(|anchor| {
                                    ForegroundInsertTarget {
                                        window_handle: anchor.window_handle,
                                        focus_handle: anchor.focus_handle,
                                        primary_focus_handle: None,
                                        fallback_focus_handle: None,
                                        focus_capture_source: None,
                                    }
                                })
                            });
                        let output_strategy = selected_output_strategy_for_report
                            .lock()
                            .ok()
                            .and_then(|slot| *slot)
                            .unwrap_or(DesktopOutputStrategy::HonorConfiguredOutput);
                        persist_insert_target_diagnostic_if_available(
                            report.log_path.as_path(),
                            persisted_insert_target,
                            origin_insert_target_for_report.as_ref(),
                            captured_context.as_ref(),
                            origin_insert_target_source.as_deref(),
                            pending_hotkey_origin_insert_target.as_ref(),
                            release_time_origin_insert_target.as_ref(),
                            Some(output_strategy),
                            restore_diagnostic.lock().ok().and_then(|slot| *slot),
                        );
                        if report.session.status() == talk_core::SessionStatus::Completed
                            && (output_strategy == DesktopOutputStrategy::ShowCopyPopupOnly
                                || selected_show_result_in_gui_for_report
                                    .lock()
                                    .ok()
                                    .is_some_and(|slot| *slot))
                        {
                            let text_result = runtime_voice_text_result(&report);
                            if let Some(output_text) = text_result.processed_output.as_deref() {
                                let result_model = desktop_mode_text_result_model(
                                    runtime_voice_mode,
                                    text_result.smart_routed_mode,
                                    text_result.transcript.as_deref().unwrap_or_default(),
                                    DesktopTextLifecycleState::Corrected,
                                    output_text,
                                    DesktopTextLifecycleState::Corrected,
                                );
                                shared.pending_copy_popup = Some(PendingCopyPopup {
                                    generation,
                                    model: desktop_copy_popup_model_for_mode_text_result(
                                        &result_model,
                                    ),
                                });
                            }
                        }
                        if report.session.status() == talk_core::SessionStatus::Completed
                            && output_strategy == DesktopOutputStrategy::HonorConfiguredOutput
                            && cloud_correction_after_local_insert
                            && desktop_streaming_final_correction_job_enabled(streaming_stop_policy)
                        {
                            if let (Some(target), Some(output_text)) =
                                (persisted_insert_target, report.session.output_text())
                            {
                                let local_text = output_text.to_string();
                                match speculative_correction_job_for_final_document(
                                    &config,
                                    local_asr_correction_segment_id,
                                    local_text,
                                    report.requested_mode,
                                    report.smart_routed_mode,
                                    origin_insert_target.as_ref(),
                                    target,
                                    live_inserted_baseline_for_stop.clone(),
                                    report.log_path.clone(),
                                    generation,
                                    hwnd_value,
                                    hud_hwnd_value,
                                ) {
                                    Ok(job) => correction_job = Some(job),
                                    Err(error) => {
                                        eprintln!(
                                            "Talk final document correction job skipped: {error}"
                                        );
                                    }
                                }
                            }
                        }
                        let summary = match report.session.status() {
                            talk_core::SessionStatus::Completed => "completed",
                            talk_core::SessionStatus::Failed => "failed",
                            talk_core::SessionStatus::Cancelled => "cancelled",
                            _ => "completed",
                        };
                        let detail = match report.session.status() {
                            talk_core::SessionStatus::Completed => {
                                report.session.output_text().map(str::to_string)
                            }
                            talk_core::SessionStatus::Failed => {
                                report.session.error().map(str::to_string)
                            }
                            talk_core::SessionStatus::Cancelled => {
                                Some("user cancelled during recording".to_string())
                            }
                            _ => None,
                        };
                        set_last_session(&mut shared, summary, detail);
                    }
                    Err(error) => {
                        shared.pending_corrected_hud = None;
                        shared.pending_worker_error = Some((generation, error.to_string()));
                        set_last_session(&mut shared, "failed", Some(error.to_string()));
                    }
                }
                if let Some(error_message) = recording_finalize_error.as_ref() {
                    shared.pending_worker_error = Some((generation, error_message.clone()));
                }
            }

            if let Some(job) = correction_job {
                spawn_speculative_cloud_correction(
                    correction_runtime_handle,
                    Arc::clone(&shared),
                    job,
                );
            }

            unsafe {
                let _ = PostMessageW(
                    hwnd_value as HWND,
                    WORKER_DONE_MESSAGE,
                    generation as usize,
                    0,
                );
            }
        });
        register_pending_worker_task(&worker_registration_shared, generation, worker_task);
    }

    fn apply_runtime_phase(hwnd: HWND, phase: RuntimePhase, generation: u64) {
        let state = match unsafe { get_window_state(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let active_generation = {
            let shared = lock_recovering(&state.shared, "Talk desktop shared state");
            shared.worker_generation
        };
        if active_generation != Some(generation) {
            return;
        }

        if let Ok(mut shared) = state.shared.lock() {
            shared.current_phase = Some(phase);
        }

        match desktop_hud_presentation_for_phase(phase) {
            DesktopHudPresentation::Hidden => {
                let _ = hide_hud(hwnd);
            }
            DesktopHudPresentation::Visible { auto_hide_ms } => {
                let _ = show_hud_model(hwnd, desktop_hud_view_model_for_phase(phase), auto_hide_ms);
            }
        }
        let _ = update_tray_icon(hwnd, hud_message_for_phase(phase));
    }

    fn handle_worker_done(hwnd: HWND, generation: u64) {
        let (unexpected_error, pending_copy_popup, pending_corrected_hud) = {
            let state = match unsafe { get_window_state(hwnd) } {
                Ok(state) => state,
                Err(_) => return,
            };
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if shared.worker_generation != Some(generation) {
                return;
            }
            if shared
                .pending_worker_task
                .as_ref()
                .is_some_and(|(task_generation, _)| *task_generation == generation)
            {
                shared.pending_worker_task.take();
            }
            shared.pending_stop_live_correction_tracker.take();
            shared.worker_generation = None;
            shared.shell_state = shared.shell_state.complete();
            shared.current_phase = None;
            let pending_copy_popup = match shared.pending_copy_popup.take() {
                Some(popup) if popup.generation == generation => Some(popup),
                Some(other) => {
                    shared.pending_copy_popup = Some(other);
                    None
                }
                None => None,
            };
            let pending_corrected_hud = match shared.pending_corrected_hud.take() {
                Some(pending) if pending.generation == generation => Some(pending),
                Some(other) => {
                    shared.pending_corrected_hud = Some(other);
                    None
                }
                None => None,
            };
            let unexpected_error = match shared.pending_worker_error.take() {
                Some((error_generation, error)) if error_generation == generation => Some(error),
                Some(other) => {
                    shared.pending_worker_error = Some(other);
                    None
                }
                None => None,
            };
            (unexpected_error, pending_copy_popup, pending_corrected_hud)
        };

        if unexpected_error.is_some() {
            let _ = show_hud_text(
                hwnd,
                &compose_hud_message("Talk: failed", unexpected_error.as_deref()),
                Some(1800),
            );
        } else if pending_copy_popup.is_none() {
            if let Some(corrected) = pending_corrected_hud {
                let _ = show_hud_model(
                    hwnd,
                    desktop_hud_view_model_for_corrected_text(&corrected.text),
                    Some(1800),
                );
            }
        }
        if let Some(popup) = pending_copy_popup {
            let _ = hide_hud(hwnd);
            let _ = show_copy_popup(hwnd, popup.model);
        }
        let _ = refresh_idle_tray_status(hwnd);
    }

    fn handle_corrected_hud(hwnd: HWND, generation: u64) {
        let corrected_text = {
            let state = match unsafe { get_window_state(hwnd) } {
                Ok(state) => state,
                Err(_) => return,
            };
            let shared = lock_recovering(&state.shared, "Talk desktop shared state");
            shared
                .pending_corrected_hud
                .as_ref()
                .filter(|pending| pending.generation == generation)
                .map(|pending| pending.text.clone())
        };

        if let Some(corrected_text) = corrected_text {
            let _ = show_hud_model(
                hwnd,
                desktop_hud_view_model_for_corrected_text(&corrected_text),
                None,
            );
        }
    }

    fn drain_active_live_correction_backlog(
        hwnd: HWND,
        generation: u64,
    ) -> Option<DesktopStreamingHudTranscriptParts> {
        let state = match unsafe { get_window_state(hwnd) } {
            Ok(state) => state,
            Err(_) => return None,
        };
        let (
            tracker,
            hud_streaming_segments,
            hud_streaming_segments_revision,
            latest_asr_text,
            fallback_text,
            route_was_requested,
            gate,
        ) = {
            let shared = match state.shared.lock() {
                Ok(shared) => shared,
                Err(_) => return None,
            };
            let default_voice_mode = shared
                .config
                .as_ref()
                .map(|config| config.default_voice_mode())?;
            let gate = shared.foreground_apply_gate.clone();
            let active = shared
                .active_recording
                .as_ref()
                .filter(|active| active.generation == generation)?;
            let requested_mode = active.mode_override.unwrap_or(default_voice_mode);
            (
                Arc::clone(&active.live_streaming_correction_tracker),
                Arc::clone(&active.hud_streaming_segments),
                active.hud_streaming_segments_revision,
                active
                    .last_streaming_asr_event
                    .as_ref()
                    .map(StreamingAsrEvent::text)
                    .map(str::to_string),
                active.streaming_unavailable_hint.clone(),
                requested_mode == VoiceMode::Smart && active.live_smart_routed_mode.is_none(),
                gate,
            )
        };
        let pending_transcript =
            tracker
                .snapshot_with_revision()
                .map(|(tracker_revision, tracker_snapshot)| {
                    let transcript_summary = desktop_streaming_hud_transcript_summary_owned(
                        &tracker_snapshot,
                        hud_streaming_segments.as_slice(),
                    );
                    let routed_mode = if !route_was_requested
                        || transcript_summary.parts.corrected_prefix.trim().is_empty()
                    {
                        None
                    } else {
                        let transcript =
                            desktop_streaming_hud_transcript_parts_text(&transcript_summary.parts);
                        live_smart_route_for_corrected_transcript(
                            VoiceMode::Smart,
                            None,
                            transcript.as_ref(),
                            transcript_summary.effective_segment_count,
                        )
                    };
                    let transcript_summary =
                        desktop_streaming_hud_transcript_summary_apply_fallback_text(
                            transcript_summary,
                            latest_asr_text.as_deref(),
                            fallback_text.as_deref(),
                        );
                    PendingLiveCorrectionTranscript {
                        tracker_revision,
                        tracker_snapshot,
                        hud_streaming_segments_revision,
                        latest_asr_text,
                        fallback_text,
                        transcript_parts: transcript_summary.parts,
                        route_was_requested,
                        routed_mode,
                    }
                });

        let (config, origin_insert_target, requested_mode, smart_routed_mode, transcript_parts) = {
            let mut shared = match state.shared.lock() {
                Ok(shared) => shared,
                Err(_) => return None,
            };
            let config = shared.config.clone()?;
            let active = shared
                .active_recording
                .as_mut()
                .filter(|active| active.generation == generation)?;
            if !Arc::ptr_eq(&active.live_streaming_correction_tracker, &tracker) {
                return None;
            }
            let requested_mode = active
                .mode_override
                .unwrap_or_else(|| config.default_voice_mode());
            let mut transcript_parts = None;
            if let Some(candidate) = pending_transcript {
                let candidate_is_current = active.hud_streaming_segments_revision
                    == candidate.hud_streaming_segments_revision
                    && active
                        .last_streaming_asr_event
                        .as_ref()
                        .map(StreamingAsrEvent::text)
                        == candidate.latest_asr_text.as_deref()
                    && active.streaming_unavailable_hint.as_deref()
                        == candidate.fallback_text.as_deref()
                    && tracker.current_revision() == Some(candidate.tracker_revision);
                if candidate_is_current {
                    active.live_correction_snapshot_revision = candidate.tracker_revision;
                    active.live_correction_snapshot = candidate.tracker_snapshot;
                    if candidate.route_was_requested {
                        active.live_smart_routed_mode = commit_live_smart_route_candidate(
                            requested_mode,
                            active.live_smart_routed_mode,
                            true,
                            candidate.routed_mode,
                        );
                    }
                    transcript_parts = Some(candidate.transcript_parts);
                }
            }
            (
                config,
                active.origin_insert_target.clone(),
                requested_mode,
                active.live_smart_routed_mode,
                transcript_parts,
            )
        };
        if !desktop_live_correction_target_apply_allowed(requested_mode, smart_routed_mode) {
            return transcript_parts;
        }

        let _foreground_apply_lease = gate.acquire();
        if !correction_foreground_side_effects_allowed_for_shared(&state.shared, generation) {
            return None;
        }
        let anchors = insert_live_streaming_corrected_backlog_if_safe(
            &config,
            hwnd,
            state.hud_hwnd.get(),
            origin_insert_target.as_ref(),
            &tracker,
        );
        for anchor in anchors {
            mirror_live_streaming_inserted_anchor(&state.shared, generation, anchor);
        }
        transcript_parts
    }

    fn handle_streaming_corrected_hud(hwnd: HWND, generation: u64) {
        let Some(transcript_parts) = drain_active_live_correction_backlog(hwnd, generation) else {
            return;
        };
        let state = match unsafe { get_window_state(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let generation_is_current = state.shared.lock().ok().is_some_and(|shared| {
            shared
                .active_recording
                .as_ref()
                .is_some_and(|active| active.generation == generation)
        });
        if !generation_is_current {
            return;
        }

        let updated_model = overlay_ui_state().lock().ok().and_then(|mut overlay| {
            let corrected_prefix = (!transcript_parts.corrected_prefix.trim().is_empty())
                .then_some(transcript_parts.corrected_prefix.as_str());
            let partial_tail = (!transcript_parts.pre_recognized_tail.trim().is_empty())
                .then_some(transcript_parts.pre_recognized_tail.as_str());
            let transcript_changed = overlay.hud_streaming_corrected_prefix.as_deref()
                != corrected_prefix
                || overlay.hud_streaming_partial_tail.as_deref() != partial_tail;
            if !transcript_changed {
                return None;
            }
            overlay.hud_streaming_corrected_prefix = corrected_prefix.map(str::to_string);
            overlay.hud_streaming_partial_tail = partial_tail.map(str::to_string);
            if !overlay.hud_streaming_scroll_user_scrolled {
                overlay.hud_streaming_scroll_line_offset = usize::MAX;
            }

            let presented_parts = listening_hud_presented_transcript_parts(
                &transcript_parts,
                overlay.hud_streaming_scroll_user_scrolled,
            );
            let (display_text, lifecycle) =
                listening_hud_transcript_text_and_lifecycle(&presented_parts);
            let meter_bins = overlay.hud_meter_bins;
            let hud_model = overlay.hud_model.as_mut()?;
            if hud_model.visual_state != DesktopHudVisualState::Listening {
                return None;
            }
            *hud_model = desktop_hud_view_model_for_listening_waveform_with_partial_and_lifecycle(
                meter_bins,
                display_text.as_deref(),
                lifecycle,
            );
            Some(hud_model.clone())
        });

        if let Some(updated_model) = updated_model {
            let _ = show_hud_model(hwnd, updated_model, None);
        }
    }

    fn handle_correction_copy_popup(hwnd: HWND, generation: u64) {
        let pending_copy_popup = {
            let state = match unsafe { get_window_state(hwnd) } {
                Ok(state) => state,
                Err(_) => return,
            };
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            match shared.pending_copy_popup.take() {
                Some(popup)
                    if popup.generation == generation
                        && desktop_document_recorrection_generation_is_current(
                            shared.next_generation,
                            generation,
                        ) =>
                {
                    Some(popup)
                }
                Some(other) => {
                    shared.pending_copy_popup = Some(other);
                    None
                }
                None => None,
            }
        };

        if let Some(popup) = pending_copy_popup {
            let _ = hide_hud(hwnd);
            let _ = show_copy_popup(hwnd, popup.model);
        }
    }

    fn mark_idle_after_terminal_state(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
        shared.shell_state = shared.shell_state.complete();
        shared.current_phase = None;
        shared.worker_generation = None;
        update_tray_icon(hwnd, current_idle_status(&shared))
    }

    fn handle_menu_command(hwnd: HWND, command: u16) {
        if let Some(mode) = voice_mode_from_menu_command(command) {
            let _ = select_voice_mode_from_menu(hwnd, mode);
            return;
        }

        match command {
            MENU_START => {
                if let Ok(state) = unsafe { get_window_state(hwnd) } {
                    if let Err(error) = begin_recording(hwnd, state, ActivationSource::Tray, 0) {
                        let _ = show_hud_text(
                            hwnd,
                            &compose_hud_message("Talk: unavailable", Some(&error.to_string())),
                            Some(1800),
                        );
                    }
                }
            }
            MENU_STOP => {
                let generation = unsafe { get_window_state(hwnd) }.ok().and_then(|state| {
                    state.shared.lock().ok().and_then(|shared| {
                        shared
                            .active_recording
                            .as_ref()
                            .map(|active| active.generation)
                            .or_else(|| {
                                shared
                                    .pending_recording_begin
                                    .as_ref()
                                    .map(|pending| pending.generation)
                            })
                    })
                });
                if let Some(generation) = generation {
                    request_stop_recording(hwnd, generation);
                }
            }
            MENU_CANCEL => {
                let _ = cancel_active_recording(hwnd);
            }
            MENU_SHOW_STATUS => {
                let _ = show_status_dialog(hwnd);
            }
            MENU_OPEN_LOGS => {
                let _ = open_logs_folder(hwnd);
            }
            MENU_OPEN_CONFIG => {
                let _ = open_config_file(hwnd);
            }
            MENU_RELOAD_CONFIG => {
                if let Err(error) = reload_config(hwnd) {
                    let _ = show_hud_text(
                        hwnd,
                        &compose_hud_message("Talk: unavailable", Some(&error.to_string())),
                        Some(1800),
                    );
                }
            }
            MENU_EXIT => unsafe {
                DestroyWindow(hwnd);
            },
            _ => {}
        }
    }

    fn voice_mode_from_menu_command(command: u16) -> Option<VoiceMode> {
        match command {
            MENU_MODE_SMART => Some(VoiceMode::Smart),
            MENU_MODE_TRANSCRIBE => Some(VoiceMode::Transcribe),
            MENU_MODE_DOCUMENT => Some(VoiceMode::Document),
            MENU_MODE_COMMAND => Some(VoiceMode::Command),
            MENU_MODE_GENERATE => Some(VoiceMode::Generate),
            _ => None,
        }
    }

    fn menu_command_for_voice_mode(mode: VoiceMode) -> Option<u16> {
        match mode {
            VoiceMode::Smart => Some(MENU_MODE_SMART),
            VoiceMode::Transcribe | VoiceMode::Dictate => Some(MENU_MODE_TRANSCRIBE),
            VoiceMode::Document | VoiceMode::Polish | VoiceMode::Translate => {
                Some(MENU_MODE_DOCUMENT)
            }
            VoiceMode::Command => Some(MENU_MODE_COMMAND),
            VoiceMode::Generate => Some(MENU_MODE_GENERATE),
        }
    }

    fn select_voice_mode_from_menu(hwnd: HWND, mode: VoiceMode) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let label = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if !shared.shell_state.can_start_session() {
                anyhow::bail!("Talk mode can only be changed while idle");
            }
            shared.selected_voice_mode = mode;
            desktop_mode_dropdown_model(mode).current_label
        };
        show_hud_text(
            hwnd,
            &compose_hud_message("Talk: mode", Some(&label)),
            Some(1200),
        )?;
        refresh_idle_tray_status(hwnd)?;
        Ok(())
    }

    fn open_logs_folder(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let logs_dir = {
            let shared = lock_recovering(&state.shared, "Talk desktop shared state");
            shared
                .config
                .as_ref()
                .map(|config| resolve_logs_dir(&shared.config_path, &config.logging.dir))
                .unwrap_or_else(|| {
                    shared
                        .config_path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(".runtime")
                        .join("talk")
                        .join("logs")
                })
        };
        spawn_external_open("explorer", logs_dir, "Talk logs folder")
    }

    fn open_config_file(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let config_path = {
            let shared = lock_recovering(&state.shared, "Talk desktop shared state");
            shared.config_path.clone()
        };
        spawn_external_open("notepad.exe", config_path, "Talk config")
    }

    fn spawn_external_open(
        program: &'static str,
        target: PathBuf,
        description: &'static str,
    ) -> Result<()> {
        thread::Builder::new()
            .name("talk-shell-open".to_string())
            .spawn(move || {
                if let Err(error) = std::process::Command::new(program).arg(&target).spawn() {
                    eprintln!("open {description} {} failed: {error}", target.display());
                }
            })
            .with_context(|| format!("schedule opening {description}"))?;
        Ok(())
    }

    fn show_status_dialog(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let snapshot = {
            let shared = lock_recovering(&state.shared, "Talk desktop shared state");
            status_snapshot(&shared)
        };
        let report = build_status_report(&snapshot);
        unsafe {
            MessageBoxW(
                hwnd,
                to_wide(&report).as_ptr(),
                to_wide("Talk status").as_ptr(),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        Ok(())
    }

    fn config_reload_completion_allowed(
        shutting_down: bool,
        current_generation: u64,
        generation: u64,
    ) -> bool {
        !shutting_down && current_generation == generation
    }

    fn prepare_config_reload(
        runtime_handle: &tokio::runtime::Handle,
        config_path: &Path,
    ) -> std::result::Result<PreparedConfigReload, String> {
        let config = runtime_handle
            .block_on(load_effective_config(config_path))
            .with_context(|| format!("reload Talk config {}", config_path.display()))
            .map_err(|error| format!("{error:#}"))?;
        let selected_voice_mode = config.default_voice_mode();
        let shortcut_label = desktop_shortcut_label_from_config(&config);
        let hotkey_binding = initial_hotkey_binding(&config);
        let native_readiness = configured_native_readiness(&config);
        let uses_streaming_service =
            desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(&config))
                == DesktopSpeculativeLocalAsrRoute::StreamingService;

        Ok(PreparedConfigReload {
            config,
            selected_voice_mode,
            shortcut_label,
            hotkey_binding,
            native_readiness,
            uses_streaming_service,
        })
    }

    fn release_config_reload_reservation(
        shared: &Arc<Mutex<SharedState>>,
        generation: u64,
    ) -> bool {
        let Ok(mut shared) = shared.lock() else {
            return false;
        };
        if !config_reload_completion_allowed(
            shared.shutting_down,
            shared.config_reload_generation,
            generation,
        ) || shared
            .pending_config_reload
            .as_ref()
            .is_none_or(|pending| pending.generation != generation)
        {
            return false;
        }
        shared.pending_config_reload = None;
        true
    }

    fn spawn_config_reload_prepare(
        shared: Arc<Mutex<SharedState>>,
        hwnd: HWND,
        generation: u64,
        config_path: PathBuf,
        runtime_handle: tokio::runtime::Handle,
    ) {
        let hwnd_value = hwnd as usize;
        thread::spawn(move || {
            let mut result = Some(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    prepare_config_reload(&runtime_handle, &config_path)
                }))
                .unwrap_or_else(|_| Err("Talk config reload preparation panicked".to_string())),
            );
            let should_post = shared.lock().ok().is_some_and(|mut shared| {
                if !config_reload_completion_allowed(
                    shared.shutting_down,
                    shared.config_reload_generation,
                    generation,
                ) {
                    return false;
                }
                let Some(pending) = shared.pending_config_reload.as_mut() else {
                    return false;
                };
                if pending.generation != generation || pending.result.is_some() {
                    return false;
                }
                pending.result = result.take();
                true
            });
            if !should_post {
                return;
            }
            let posted = unsafe {
                PostMessageW(
                    hwnd_value as HWND,
                    CONFIG_RELOAD_DONE_MESSAGE,
                    generation as usize,
                    0,
                )
            };
            if posted == 0 {
                let _ = release_config_reload_reservation(&shared, generation);
            }
        });
    }

    fn spawn_config_reload_daemon_reconcile(
        shared: Arc<Mutex<SharedState>>,
        daemon_to_stop: Option<ManagedLocalAsrDaemon>,
        should_prewarm: bool,
    ) {
        if daemon_to_stop.is_none() && !should_prewarm {
            return;
        }
        thread::spawn(move || {
            if let Some(daemon) = daemon_to_stop {
                stop_managed_local_asr_daemon(daemon);
            }
            if should_prewarm {
                if let Err(error) = run_product_local_asr_daemon_prewarm(&shared) {
                    eprintln!("Talk product local ASR prewarm failed after reload: {error:#}");
                }
            }
        });
    }

    fn handle_config_reload_done(hwnd: HWND, generation: u64) {
        let state = match unsafe { get_window_state(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let result = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if !config_reload_completion_allowed(
                shared.shutting_down,
                shared.config_reload_generation,
                generation,
            ) {
                return;
            }
            let Some(pending) = shared.pending_config_reload.as_ref() else {
                return;
            };
            if pending.generation != generation || pending.result.is_none() {
                return;
            }
            let mut pending = shared
                .pending_config_reload
                .take()
                .expect("validated pending config reload");
            pending
                .result
                .take()
                .expect("validated config reload result")
        };

        let (
            status_text,
            tray_status,
            daemon_to_stop,
            should_prewarm,
            should_start_product_bootstrap,
            model_task_to_abort,
        ) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            let mut daemon_to_stop = None;
            let mut should_prewarm = false;
            let mut should_start_product_bootstrap = false;
            let mut model_task_to_abort = None;
            match result {
                Ok(PreparedConfigReload {
                    config,
                    selected_voice_mode,
                    shortcut_label,
                    hotkey_binding,
                    native_readiness,
                    uses_streaming_service,
                }) => {
                    let daemon_config_changed = shared
                        .config
                        .as_ref()
                        .is_none_or(|current| !local_asr_daemon_config_matches(current, &config));
                    if daemon_config_changed || !uses_streaming_service {
                        advance_local_asr_daemon_epoch(&mut shared);
                        daemon_to_stop = shared.local_asr_daemon.take();
                    }
                    should_prewarm = uses_streaming_service;
                    should_start_product_bootstrap = uses_streaming_service;
                    if !uses_streaming_service {
                        shared.product_bootstrap_generation =
                            shared.product_bootstrap_generation.saturating_add(1);
                        shared.pending_product_bootstrap_prepare_generation = None;
                        model_task_to_abort = shared.pending_model_bootstrap_task.take();
                        shared.local_asr_bootstrap_status = LocalAsrBootstrapStatus::NotStarted;
                    }
                    shared.config = Some(Arc::new(config));
                    shared.config_status = ConfigAvailability::ready();
                    shared.selected_voice_mode = selected_voice_mode;
                    match hotkey_binding {
                        Ok((desktop_actions, hotkey)) => {
                            shared.desktop_actions = desktop_actions;
                            shared.hotkey = hotkey;
                        }
                        Err(error) => {
                            shared.desktop_actions = Vec::new();
                            shared.hotkey =
                                HotkeyBindingState::invalid_config(shortcut_label, error);
                        }
                    }
                    shared.native_readiness = Some(native_readiness);
                }
                Err(error) => {
                    eprintln!("Talk config reload failed: {error}");
                    if shared.config.is_none() {
                        shared.config_status = ConfigAvailability::unavailable(error);
                        shared.hotkey = HotkeyBindingState::Unconfigured;
                        shared.desktop_actions = Vec::new();
                        shared.native_readiness = None;
                    }
                }
            }
            register_or_mark_hotkey_failure(hwnd, &mut shared);
            let summary = current_idle_status(&shared);
            (
                compose_hud_message(summary, current_idle_detail(&shared).as_deref()),
                summary.to_string(),
                daemon_to_stop,
                should_prewarm,
                should_start_product_bootstrap,
                model_task_to_abort,
            )
        };

        if let Some(task) = model_task_to_abort {
            task.abort();
        }
        spawn_config_reload_daemon_reconcile(
            Arc::clone(&state.shared),
            daemon_to_stop,
            should_prewarm,
        );
        if should_start_product_bootstrap {
            start_product_bootstrap(hwnd, Arc::clone(&state.shared));
        }
        if let Err(error) = update_tray_icon(hwnd, &tray_status) {
            eprintln!("Talk config reload tray status update failed: {error:#}");
        }
        if let Err(error) = show_hud_text(hwnd, &status_text, Some(1800)) {
            eprintln!("Talk config reload HUD update failed: {error:#}");
        }
    }

    fn reload_config(hwnd: HWND) -> Result<()> {
        let _ = cancel_pending_recording_begin(hwnd, None);
        let state = unsafe { get_window_state(hwnd)? };
        let (generation, config_path, runtime_handle) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if shared.shutting_down {
                anyhow::bail!("Talk desktop is shutting down");
            }
            if !shared.shell_state.can_start_session() || shared.active_recording.is_some() {
                anyhow::bail!("Talk config can only be reloaded while idle");
            }
            shared.config_reload_generation = shared.config_reload_generation.saturating_add(1);
            let generation = shared.config_reload_generation;
            shared.pending_config_reload = Some(PendingConfigReload {
                generation,
                result: None,
            });
            (
                generation,
                shared.config_path.clone(),
                shared.runtime_handle.clone(),
            )
        };
        spawn_config_reload_prepare(
            Arc::clone(&state.shared),
            hwnd,
            generation,
            config_path,
            runtime_handle,
        );
        Ok(())
    }

    fn resolve_logs_dir(config_path: &Path, logs_dir: &Path) -> PathBuf {
        if logs_dir.is_absolute() {
            logs_dir.to_path_buf()
        } else {
            config_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(logs_dir)
        }
    }

    fn show_tray_menu(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let (shell_state, config_status, hotkey_state, native_readiness, selected_voice_mode) = {
            let shared = lock_recovering(&state.shared, "Talk desktop shared state");
            (
                shared.shell_state,
                shared.config_status.clone(),
                shared.hotkey.clone(),
                shared.native_readiness.clone(),
                shared.selected_voice_mode,
            )
        };
        let menu_model = tray_menu_model(
            &shell_state,
            &config_status,
            &hotkey_state,
            native_readiness.as_ref(),
        );
        let mode_dropdown = desktop_mode_dropdown_model(selected_voice_mode);

        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            anyhow::bail!("create Talk desktop tray menu");
        }

        let header_flags = MF_STRING | MF_GRAYED;
        let start_flags = if menu_model.start_enabled {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let stop_flags = if menu_model.stop_enabled {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let cancel_flags = if menu_model.cancel_enabled {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let reload_flags = if menu_model.reload_config_enabled {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let mode_flags = if menu_model.start_enabled {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        unsafe {
            AppendMenuW(
                menu,
                header_flags,
                0,
                to_wide(&menu_model.hotkey_label).as_ptr(),
            );
            if let Some(detail_label) = menu_model.detail_label.as_ref() {
                AppendMenuW(menu, header_flags, 0, to_wide(detail_label).as_ptr());
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(
                menu,
                header_flags,
                0,
                to_wide(&format!(
                    "{}: {} ▼",
                    mode_dropdown.title, mode_dropdown.current_label
                ))
                .as_ptr(),
            );
            for entry in &mode_dropdown.entries {
                if let Some(command) = menu_command_for_voice_mode(entry.mode) {
                    let label = match entry.shortcut_hint.as_deref() {
                        Some(shortcut) => format!("{}    {}", entry.label, shortcut),
                        None => entry.label.clone(),
                    };
                    let flags = if entry.selected {
                        mode_flags | MF_CHECKED
                    } else {
                        mode_flags
                    };
                    AppendMenuW(menu, flags, command as usize, to_wide(&label).as_ptr());
                }
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(
                menu,
                start_flags,
                MENU_START as usize,
                to_wide("Start dictation").as_ptr(),
            );
            AppendMenuW(
                menu,
                stop_flags,
                MENU_STOP as usize,
                to_wide("Stop recording").as_ptr(),
            );
            AppendMenuW(
                menu,
                cancel_flags,
                MENU_CANCEL as usize,
                to_wide("Cancel recording").as_ptr(),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_SHOW_STATUS as usize,
                to_wide("Show Talk status").as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_OPEN_LOGS as usize,
                to_wide("Open Talk logs folder").as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_OPEN_CONFIG as usize,
                to_wide("Open Talk config").as_ptr(),
            );
            AppendMenuW(
                menu,
                reload_flags,
                MENU_RELOAD_CONFIG as usize,
                to_wide("Reload Talk config").as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_EXIT as usize,
                to_wide("Exit Talk").as_ptr(),
            );
        }

        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point);
            SetForegroundWindow(hwnd);
            TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                hwnd,
                ptr::null(),
            );
            DestroyMenu(menu);
        }
        Ok(())
    }

    fn show_hud_model(
        hwnd: HWND,
        model: DesktopHudViewModel,
        auto_hide_ms: Option<u32>,
    ) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        if state.hud_hwnd.get().is_null() {
            anyhow::bail!("Talk desktop HUD is unavailable");
        }

        let dpi = overlay_dpi_for_window(state.hud_hwnd.get());
        let metrics = scale_hud_metrics_for_dpi(desktop_hud_metrics_for_view_model(&model), dpi);
        let (screen_width, screen_height) = current_screen_size();
        let x = ((screen_width - metrics.width).max(0)) / 2;
        let y = (screen_height - metrics.height - metrics.bottom_margin).max(0);
        let next_geometry = DesktopHudGeometry {
            x,
            y,
            width: metrics.width,
            height: metrics.height,
            corner_radius: metrics.corner_radius,
        };
        let visual_state = model.visual_state;
        let listening_layout = (visual_state == DesktopHudVisualState::Listening)
            .then(|| {
                desktop_listening_hud_partial_text_layout(
                    metrics.width,
                    metrics.height,
                    dpi,
                    model.detail.as_deref(),
                )
            })
            .flatten();
        unsafe {
            KillTimer(hwnd, TIMER_HIDE_HUD);
            if matches!(visual_state, DesktopHudVisualState::Listening) {
                SetTimer(
                    hwnd,
                    TIMER_RECORDING_LEVEL,
                    HUD_RECORDING_LEVEL_REFRESH_MS,
                    None,
                );
                KillTimer(hwnd, TIMER_THINKING_PROGRESS);
            } else if matches!(visual_state, DesktopHudVisualState::Thinking) {
                KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                SetTimer(
                    hwnd,
                    TIMER_THINKING_PROGRESS,
                    HUD_THINKING_PROGRESS_REFRESH_MS,
                    None,
                );
            } else {
                KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                KillTimer(hwnd, TIMER_THINKING_PROGRESS);
            }
        }

        let geometry_plan = if let Ok(mut overlay) = overlay_ui_state().lock() {
            let plan = desktop_hud_geometry_update_plan(overlay.hud_geometry, next_geometry);
            let was_listening = matches!(
                overlay.hud_model.as_ref().map(|hud| hud.visual_state),
                Some(DesktopHudVisualState::Listening)
            );
            let was_thinking = matches!(
                overlay.hud_model.as_ref().map(|hud| hud.visual_state),
                Some(DesktopHudVisualState::Thinking)
            );
            if visual_state != DesktopHudVisualState::Listening || !was_listening {
                overlay.hud_meter_bins = [0.0; 9];
                overlay.hud_streaming_corrected_prefix = None;
                overlay.hud_streaming_partial_tail = None;
                overlay.hud_streaming_partial_layout = None;
                overlay.hud_streaming_scroll_line_offset = 0;
                overlay.hud_streaming_scroll_dragging = false;
                overlay.hud_streaming_scroll_user_scrolled = false;
            }
            if visual_state != DesktopHudVisualState::Thinking || !was_thinking {
                overlay.hud_thinking_pulse_tick = 0;
            }
            overlay.hud_geometry = Some(next_geometry);
            if visual_state == DesktopHudVisualState::Listening {
                let reconciled_scroll_state = listening_layout.as_ref().map(|layout| {
                    desktop_listening_hud_reconcile_scroll_state(
                        overlay.hud_streaming_scroll_line_offset,
                        desktop_listening_hud_scroll_max_offset(layout),
                        overlay.hud_streaming_scroll_user_scrolled,
                    )
                });
                overlay.hud_streaming_partial_layout = listening_layout;
                if let Some((next_offset, next_user_scrolled)) = reconciled_scroll_state {
                    overlay.hud_streaming_scroll_line_offset = next_offset;
                    overlay.hud_streaming_scroll_user_scrolled = next_user_scrolled;
                }
            }
            overlay.hud_model = Some(model);
            plan
        } else {
            desktop_hud_geometry_update_plan(None, next_geometry)
        };

        unsafe {
            if geometry_plan.reposition {
                SetWindowPos(
                    state.hud_hwnd.get(),
                    (-1isize) as HWND,
                    x,
                    y,
                    metrics.width,
                    metrics.height,
                    SWP_NOACTIVATE,
                );
            }
            if geometry_plan.reshape {
                apply_rounded_window_region(
                    state.hud_hwnd.get(),
                    metrics.width,
                    metrics.height,
                    metrics.corner_radius,
                );
            }
            InvalidateRect(state.hud_hwnd.get(), ptr::null(), 0);
            ShowWindow(state.hud_hwnd.get(), SW_SHOWNOACTIVATE);
            if let Some(timeout) = auto_hide_ms {
                SetTimer(hwnd, TIMER_HIDE_HUD, timeout, None);
            }
        }
        Ok(())
    }

    fn show_hud_text(hwnd: HWND, text: &str, auto_hide_ms: Option<u32>) -> Result<()> {
        show_hud_model(hwnd, desktop_hud_view_model_for_text(text), auto_hide_ms)
    }

    fn dispatch_live_streaming_events(
        dispatch: PendingLiveStreamingDispatch,
    ) -> Vec<SpeculativeInsertAnchor> {
        let mut known_anchors = dispatch.existing_anchors;
        let mut newly_inserted_anchors = Vec::<SpeculativeInsertAnchor>::new();
        let mut correction_jobs = Vec::<SpeculativeCloudCorrectionJob>::new();
        let mut rejected_local_fallback = false;

        for event in &dispatch.events {
            match event {
                SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text } => {
                    if known_anchors.contains_key(segment_id) {
                        continue;
                    }
                    match insert_live_streaming_segment_if_safe(
                        &dispatch.config,
                        dispatch.hwnd_value as HWND,
                        dispatch.hud_hwnd_value as HWND,
                        dispatch.origin_insert_target.as_ref(),
                        segment_id,
                        text,
                        DesktopTextLifecycleState::PreRecognized,
                    ) {
                        Ok(Some(anchor)) => {
                            known_anchors.insert(segment_id.clone(), anchor.clone());
                            newly_inserted_anchors.push(anchor);
                        }
                        Ok(None) => {}
                        Err(error) => {
                            eprintln!("Talk live local segment insert failed: {error:#}");
                        }
                    }
                }
                SpeculativeRuntimeEvent::CorrectionRequested { segment_id, .. } => {
                    let allow_target_apply = dispatch.allow_target_apply
                        && desktop_live_correction_target_apply_allowed(
                            dispatch.requested_mode,
                            dispatch.smart_routed_mode,
                        );
                    if let Some(job) = speculative_correction_job_for_live_segment(
                        &dispatch.config,
                        &dispatch.pipeline_config,
                        event,
                        dispatch.requested_mode,
                        known_anchors.get(segment_id),
                        dispatch.origin_insert_target.as_ref(),
                        dispatch.latest_live_segment_guard,
                        allow_target_apply,
                        dispatch.generation,
                        dispatch.hwnd_value,
                        dispatch.hud_hwnd_value,
                    ) {
                        correction_jobs.push(job);
                    }
                }
                SpeculativeRuntimeEvent::DraftUpdated { .. }
                | SpeculativeRuntimeEvent::LocalSegmentsInvalidated { .. } => {}
            }
        }

        for job in correction_jobs {
            let segment_id = job.segment_id.clone();
            if !dispatch.live_correction_tracker.register_job(
                &segment_id,
                &job.transcript,
                known_anchors.get(&segment_id).cloned(),
            ) {
                continue;
            }
            if let Err(error) = dispatch.live_correction_sender.try_send(job) {
                let (outcome, rejected_job) = match error {
                    tokio::sync::mpsc::error::TrySendError::Full(job) => ("queue_full", job),
                    tokio::sync::mpsc::error::TrySendError::Closed(job) => ("queue_closed", job),
                };
                dispatch.live_correction_tracker.record_result(
                    &segment_id,
                    rejected_job.transcript.as_str(),
                    rejected_job.anchor.clone(),
                );
                dispatch
                    .live_correction_tracker
                    .complete_job(&segment_id, None, None);
                rejected_local_fallback = true;
                if dispatch.live_correction_tracker.should_show_live_feedback() {
                    queue_live_streaming_corrected_hud(
                        dispatch.hwnd_value as HWND,
                        dispatch.generation,
                        rejected_job.transcript.as_str(),
                    );
                }
                eprintln!(
                    "{}",
                    desktop_live_correction_timing_log(
                        &segment_id,
                        rejected_job.started_at.elapsed().as_millis(),
                        0,
                        0,
                        rejected_job.started_at.elapsed().as_millis(),
                        outcome,
                    )
                );
            }
        }

        if live_correction_backlog_drain_allowed(
            dispatch.allow_target_apply,
            dispatch.flush_corrected_backlog,
            rejected_local_fallback,
            dispatch.requested_mode,
            dispatch.smart_routed_mode,
        ) {
            for anchor in insert_live_streaming_corrected_backlog_if_safe(
                &dispatch.config,
                dispatch.hwnd_value as HWND,
                dispatch.hud_hwnd_value as HWND,
                dispatch.origin_insert_target.as_ref(),
                &dispatch.live_correction_tracker,
            ) {
                known_anchors.insert(anchor.segment_id.clone(), anchor.clone());
                newly_inserted_anchors.push(anchor);
            }
            if !newly_inserted_anchors.is_empty() {
                queue_live_streaming_corrected_hud(
                    dispatch.hwnd_value as HWND,
                    dispatch.generation,
                    "",
                );
            }
        }

        newly_inserted_anchors
    }

    fn invalidate_recording_hud_waveform(state: &WindowState) {
        if state.hud_hwnd.get().is_null() {
            return;
        }
        let dpi = overlay_dpi_for_window(state.hud_hwnd.get());
        let waveform = overlay_ui_state().lock().ok().and_then(|overlay| {
            overlay
                .hud_streaming_partial_layout
                .map(|layout| layout.waveform_rect)
                .or_else(|| {
                    overlay.hud_geometry.map(|geometry| {
                        desktop_listening_hud_waveform_rect(geometry.width, geometry.height, dpi)
                    })
                })
        });
        let waveform = waveform.unwrap_or_else(|| {
            let mut client_rect = RECT::default();
            unsafe {
                GetClientRect(state.hud_hwnd.get(), &mut client_rect);
            }
            desktop_listening_hud_waveform_rect(
                client_rect.right - client_rect.left,
                client_rect.bottom - client_rect.top,
                dpi,
            )
        });
        let waveform_rect = desktop_overlay_rect_to_rect(waveform);
        unsafe {
            InvalidateRect(state.hud_hwnd.get(), &waveform_rect, 0);
        }
    }

    fn latest_streaming_asr_text_changed(
        current: Option<&StreamingAsrEvent>,
        incoming: &[StreamingAsrEvent],
    ) -> bool {
        let Some(latest) = incoming.last() else {
            return false;
        };
        current.map(|event| event.text().trim()) != Some(latest.text().trim())
    }

    fn streaming_idle_segmentation_due(
        previous_silence_ms: u64,
        current_silence_ms: u64,
        config: &SegmenterConfig,
    ) -> bool {
        [config.punctuation_pause_ms, config.soft_pause_ms]
            .into_iter()
            .any(|threshold_ms| {
                previous_silence_ms < threshold_ms && current_silence_ms >= threshold_ms
            })
    }

    fn take_streaming_asr_events_for_hud_refresh(
        pending_results: &mut VecDeque<std::result::Result<Vec<StreamingAsrEvent>, String>>,
        max_events: usize,
    ) -> (Vec<StreamingAsrEvent>, Option<String>) {
        let mut events = Vec::with_capacity(max_events.min(16));
        let mut error = None;
        while events.len() < max_events {
            let Some(result) = pending_results.pop_front() else {
                break;
            };
            match result {
                Ok(mut available) => {
                    let remaining_capacity = max_events - events.len();
                    if available.len() <= remaining_capacity {
                        events.extend(available);
                        continue;
                    }

                    events.extend(available.drain(..remaining_capacity));
                    pending_results.push_front(Ok(available));
                    break;
                }
                Err(message) => {
                    error = Some(message);
                    break;
                }
            }
        }
        (events, error)
    }

    fn process_recording_hud_refresh(
        mut work: PendingRecordingHudRefresh,
    ) -> ProcessedRecordingHudRefresh {
        let (pumped_asr_events, streaming_pump_error) = take_streaming_asr_events_for_hud_refresh(
            &mut work.pending_streaming_pump_results,
            HUD_STREAMING_ASR_EVENTS_PER_REFRESH,
        );
        if streaming_pump_error.is_none() {
            if let Some(streaming_pump) = work.streaming_pump.as_ref() {
                streaming_pump.request_pump();
            }
        }

        if let Some(waveform_source) = work.waveform_source.as_ref() {
            if waveform_source
                .current_waveform_into(&mut work.raw_waveform)
                .is_err()
            {
                work.raw_waveform.fill(0.0);
            }
        } else {
            work.raw_waveform.fill(0.0);
        }

        let latest_asr_text_changed = latest_streaming_asr_text_changed(
            work.processing_state.last_streaming_asr_event.as_ref(),
            &pumped_asr_events,
        );
        let mut runtime_events = Vec::with_capacity(pumped_asr_events.len());
        let had_pumped_asr_events = !pumped_asr_events.is_empty();
        if let Some(latest_event) = pumped_asr_events.last().cloned() {
            work.processing_state.last_streaming_asr_event = Some(latest_event);
            work.processing_state.last_streaming_asr_event_at = Some(Instant::now());
            work.processing_state.last_streaming_idle_evaluated_ms = 0;
        }
        for event in pumped_asr_events {
            match work
                .processing_state
                .speculative_runtime_state
                .accept_asr_event_with_segmentation(event, 0, &work.segmenter_config)
            {
                Ok(events) => runtime_events.extend(events),
                Err(error) => eprintln!("Talk speculative ASR event rejected: {error}"),
            }
        }

        if !had_pumped_asr_events {
            if let (Some(last_event), Some(last_event_at)) = (
                work.processing_state.last_streaming_asr_event.as_ref(),
                work.processing_state.last_streaming_asr_event_at,
            ) {
                let trailing_silence_ms = last_event_at.elapsed().as_millis() as u64;
                let should_evaluate = !last_event.is_final()
                    && streaming_idle_segmentation_due(
                        work.processing_state.last_streaming_idle_evaluated_ms,
                        trailing_silence_ms,
                        &work.segmenter_config,
                    );
                work.processing_state.last_streaming_idle_evaluated_ms = trailing_silence_ms;
                if should_evaluate {
                    let last_event = last_event.clone();
                    match work
                        .processing_state
                        .speculative_runtime_state
                        .accept_asr_event_with_segmentation(
                            last_event,
                            trailing_silence_ms,
                            &work.segmenter_config,
                        ) {
                        Ok(events) => runtime_events.extend(events),
                        Err(error) => {
                            eprintln!("Talk speculative ASR idle event rejected: {error}")
                        }
                    }
                }
            }
        }

        for event in &runtime_events {
            match event {
                SpeculativeRuntimeEvent::DraftUpdated { segment_id, text }
                | SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text } => {
                    if upsert_hud_streaming_segment(
                        Arc::make_mut(&mut work.processing_state.hud_streaming_segments),
                        segment_id,
                        text,
                    ) {
                        work.processing_state.hud_streaming_segments_revision = work
                            .processing_state
                            .hud_streaming_segments_revision
                            .wrapping_add(1);
                    }
                }
                SpeculativeRuntimeEvent::LocalSegmentsInvalidated { segment_ids } => {
                    if remove_hud_streaming_segments(
                        Arc::make_mut(&mut work.processing_state.hud_streaming_segments),
                        segment_ids,
                    ) {
                        work.processing_state.hud_streaming_segments_revision = work
                            .processing_state
                            .hud_streaming_segments_revision
                            .wrapping_add(1);
                    }
                }
                SpeculativeRuntimeEvent::CorrectionRequested { .. } => {}
            }
        }

        let transcript_changed = !runtime_events.is_empty() || latest_asr_text_changed;
        let requested_mode = work.requested_mode;
        let previous_smart_route = work.processing_state.live_smart_routed_mode;
        let refreshed_hud_transcript_parts = if transcript_changed {
            work.live_correction_tracker.refresh_snapshot_if_changed(
                &mut work.processing_state.live_correction_snapshot_revision,
                &mut work.processing_state.live_correction_snapshot,
            );
            let transcript_summary =
                desktop_streaming_hud_transcript_summary_with_fallback_text_owned(
                    &work.processing_state.live_correction_snapshot,
                    &work.processing_state.hud_streaming_segments,
                    work.processing_state
                        .last_streaming_asr_event
                        .as_ref()
                        .map(StreamingAsrEvent::text),
                    work.streaming_unavailable_hint.as_deref(),
                );
            let transcript = desktop_streaming_hud_transcript_parts_text(&transcript_summary.parts);
            work.processing_state.live_smart_routed_mode = live_smart_route_for_transcript(
                requested_mode,
                previous_smart_route,
                transcript.as_ref(),
                transcript_summary.effective_segment_count,
            );
            Some(transcript_summary.parts)
        } else {
            None
        };
        let flush_corrected_backlog = previous_smart_route != Some(VoiceMode::Transcribe)
            && work.processing_state.live_smart_routed_mode == Some(VoiceMode::Transcribe);
        let live_dispatch_seed = if transcript_changed {
            Some(PendingLiveStreamingDispatchSeed {
                live_correction_tracker: work.live_correction_tracker,
                events: runtime_events,
                requested_mode,
                smart_routed_mode: work.processing_state.live_smart_routed_mode,
                flush_corrected_backlog,
            })
        } else {
            None
        };

        ProcessedRecordingHudRefresh {
            generation: work.generation,
            processing_state: work.processing_state,
            pending_streaming_pump_results: work.pending_streaming_pump_results,
            raw_waveform: work.raw_waveform,
            streaming_unavailable_hint: work.streaming_unavailable_hint,
            refreshed_hud_transcript_parts,
            live_dispatch_seed,
            streaming_pump_error,
        }
    }

    fn refresh_recording_hud_level(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let work = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if shared.shutting_down || shared.current_phase != Some(RuntimePhase::Recording) {
                unsafe {
                    KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                }
                return Ok(());
            }

            let default_voice_mode = shared
                .config
                .as_ref()
                .map(|config| config.default_voice_mode())
                .unwrap_or(VoiceMode::Smart);
            let Some(active) = shared.active_recording.as_mut() else {
                unsafe {
                    KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                }
                return Ok(());
            };

            let (waveform_source, streaming_pump) = match &active.source {
                ActiveRecordingSource::Live {
                    recording,
                    streaming_pump,
                } => (
                    recording.streaming_pcm_source().ok(),
                    streaming_pump.clone(),
                ),
                ActiveRecordingSource::ExplicitAudioFile(_) => (None, None),
            };
            PendingRecordingHudRefresh {
                generation: active.generation,
                requested_mode: active.mode_override.unwrap_or(default_voice_mode),
                segmenter_config: active.speculative_segmenter_config,
                processing_state: RecordingHudProcessingState {
                    speculative_runtime_state: std::mem::take(
                        &mut active.speculative_runtime_state,
                    ),
                    hud_streaming_segments: std::mem::take(&mut active.hud_streaming_segments),
                    hud_streaming_segments_revision: active.hud_streaming_segments_revision,
                    live_correction_snapshot_revision: active.live_correction_snapshot_revision,
                    live_correction_snapshot: std::mem::take(&mut active.live_correction_snapshot),
                    last_streaming_asr_event: active.last_streaming_asr_event.take(),
                    last_streaming_asr_event_at: active.last_streaming_asr_event_at.take(),
                    last_streaming_idle_evaluated_ms: std::mem::take(
                        &mut active.last_streaming_idle_evaluated_ms,
                    ),
                    live_smart_routed_mode: active.live_smart_routed_mode,
                },
                pending_streaming_pump_results: std::mem::take(
                    &mut active.pending_streaming_pump_results,
                ),
                waveform_source,
                raw_waveform: active.hud_waveform,
                streaming_pump,
                streaming_unavailable_hint: active.streaming_unavailable_hint.take(),
                live_correction_tracker: Arc::clone(&active.live_streaming_correction_tracker),
            }
        };
        let processed = process_recording_hud_refresh(work);
        let ProcessedRecordingHudRefresh {
            generation,
            processing_state,
            mut pending_streaming_pump_results,
            raw_waveform,
            streaming_unavailable_hint,
            refreshed_hud_transcript_parts,
            live_dispatch_seed,
            streaming_pump_error,
        } = processed;
        let (committed, live_dispatch_metadata) = {
            let mut shared = lock_recovering(&state.shared, "Talk desktop shared state");
            if shared.shutting_down
                || shared.current_phase != Some(RuntimePhase::Recording)
                || shared
                    .active_recording
                    .as_ref()
                    .is_none_or(|active| active.generation != generation)
            {
                (false, None)
            } else {
                let config = live_dispatch_seed
                    .as_ref()
                    .and_then(|_| shared.config.clone());
                let active = shared
                    .active_recording
                    .as_mut()
                    .expect("generation-checked active recording");
                active.speculative_runtime_state = processing_state.speculative_runtime_state;
                active.hud_streaming_segments = processing_state.hud_streaming_segments;
                active.hud_streaming_segments_revision =
                    processing_state.hud_streaming_segments_revision;
                active.live_correction_snapshot_revision =
                    processing_state.live_correction_snapshot_revision;
                active.live_correction_snapshot = processing_state.live_correction_snapshot;
                active.last_streaming_asr_event = processing_state.last_streaming_asr_event;
                active.last_streaming_asr_event_at = processing_state.last_streaming_asr_event_at;
                active.last_streaming_idle_evaluated_ms =
                    processing_state.last_streaming_idle_evaluated_ms;
                active.live_smart_routed_mode = processing_state.live_smart_routed_mode;
                active.hud_waveform = raw_waveform;
                active.streaming_unavailable_hint = streaming_unavailable_hint;
                if !pending_streaming_pump_results.is_empty() {
                    pending_streaming_pump_results
                        .append(&mut active.pending_streaming_pump_results);
                    active.pending_streaming_pump_results = pending_streaming_pump_results;
                }
                let metadata = config
                    .zip(active.live_streaming_correction_sender.clone())
                    .map(
                        |(config, live_correction_sender)| LiveStreamingDispatchMetadata {
                            config,
                            origin_insert_target: active.origin_insert_target.clone(),
                            existing_anchors: active.live_streaming_inserted_anchors.clone(),
                            live_correction_sender,
                        },
                    );
                (true, metadata)
            }
        };
        if !committed {
            return Ok(());
        }
        let dispatch_metadata_missing =
            live_dispatch_seed.is_some() && live_dispatch_metadata.is_none();
        let recording_error = streaming_pump_error.or_else(|| {
            dispatch_metadata_missing.then(|| {
                "Talk live streaming dispatch metadata is unavailable during recording".to_string()
            })
        });
        if let Some(error) = recording_error {
            eprintln!("Talk recording HUD refresh failed: {error}");
            let hud_transcript_parts = refreshed_hud_transcript_parts
                .clone()
                .unwrap_or_else(cached_hud_streaming_transcript_parts);
            let copy_text = format!(
                "{}{}",
                hud_transcript_parts.corrected_prefix, hud_transcript_parts.pre_recognized_tail
            );
            return fail_active_recording(hwnd, state, error, Some(copy_text));
        }

        let live_dispatch =
            live_dispatch_seed
                .zip(live_dispatch_metadata)
                .map(|(seed, metadata)| PendingLiveStreamingDispatch {
                    pipeline_config: desktop_speculative_pipeline_config(&metadata.config),
                    config: metadata.config,
                    generation,
                    origin_insert_target: metadata.origin_insert_target,
                    existing_anchors: metadata.existing_anchors,
                    live_correction_sender: metadata.live_correction_sender,
                    live_correction_tracker: seed.live_correction_tracker,
                    events: seed.events,
                    requested_mode: seed.requested_mode,
                    smart_routed_mode: seed.smart_routed_mode,
                    flush_corrected_backlog: seed.flush_corrected_backlog,
                    latest_live_segment_guard: Some(LatestLiveSegmentGuard { generation }),
                    allow_target_apply: true,
                    hwnd_value: hwnd as usize,
                    hud_hwnd_value: state.hud_hwnd.get() as usize,
                });

        if let Some(dispatch) = live_dispatch {
            let generation = dispatch.generation;
            let foreground_apply_gate = state
                .shared
                .lock()
                .ok()
                .map(|shared| shared.foreground_apply_gate.clone());
            if let Some(foreground_apply_gate) = foreground_apply_gate {
                let _foreground_apply_lease = foreground_apply_gate.acquire();
                let newly_inserted_anchors = dispatch_live_streaming_events(dispatch);
                if !newly_inserted_anchors.is_empty() {
                    if let Ok(mut shared) = state.shared.lock() {
                        if let Some(active) = shared.active_recording.as_mut() {
                            if active.generation == generation {
                                for anchor in newly_inserted_anchors {
                                    record_live_streaming_inserted_anchor(
                                        &mut active.live_streaming_inserted_segment_ids,
                                        &mut active.live_streaming_inserted_anchors,
                                        anchor,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        let updated_hud_state = if let Ok(mut overlay) = overlay_ui_state().lock() {
            let mut next_bins = overlay.hud_meter_bins;
            for (index, raw_bin) in raw_waveform.iter().take(next_bins.len()).enumerate() {
                let raw_bin = raw_bin.clamp(0.0, 1.0);
                next_bins[index] = if raw_bin >= next_bins[index] {
                    raw_bin
                } else {
                    (next_bins[index] * 0.72).max(raw_bin)
                }
                .clamp(0.0, 1.0);
            }
            overlay.hud_meter_bins = next_bins;
            let is_listening = overlay
                .hud_model
                .as_ref()
                .is_some_and(|model| model.visual_state == DesktopHudVisualState::Listening);
            if !is_listening {
                None
            } else if let Some(hud_transcript_parts) = refreshed_hud_transcript_parts.as_ref() {
                let corrected_prefix = (!hud_transcript_parts.corrected_prefix.trim().is_empty())
                    .then_some(hud_transcript_parts.corrected_prefix.as_str());
                let partial_tail = (!hud_transcript_parts.pre_recognized_tail.trim().is_empty())
                    .then_some(hud_transcript_parts.pre_recognized_tail.as_str());
                let transcript_changed = overlay.hud_streaming_corrected_prefix.as_deref()
                    != corrected_prefix
                    || overlay.hud_streaming_partial_tail.as_deref() != partial_tail;
                if transcript_changed {
                    overlay.hud_streaming_corrected_prefix = corrected_prefix.map(str::to_string);
                    overlay.hud_streaming_partial_tail = partial_tail.map(str::to_string);
                    if !overlay.hud_streaming_scroll_user_scrolled {
                        overlay.hud_streaming_scroll_line_offset = usize::MAX;
                    }
                    let presented_parts = listening_hud_presented_transcript_parts(
                        hud_transcript_parts,
                        overlay.hud_streaming_scroll_user_scrolled,
                    );
                    let (display_text, lifecycle) =
                        listening_hud_transcript_text_and_lifecycle(&presented_parts);
                    let Some(hud_model) = overlay.hud_model.as_mut() else {
                        return Ok(());
                    };
                    *hud_model =
                        desktop_hud_view_model_for_listening_waveform_with_partial_and_lifecycle(
                            next_bins,
                            display_text.as_deref(),
                            lifecycle,
                        );
                    Some(RecordingHudRefreshUpdate::Text(hud_model.clone()))
                } else {
                    let Some(hud_model) = overlay.hud_model.as_mut() else {
                        return Ok(());
                    };
                    hud_model.meter = Some(desktop_hud_audio_meter_model_for_waveform(next_bins));
                    Some(RecordingHudRefreshUpdate::WaveformOnly)
                }
            } else {
                let Some(hud_model) = overlay.hud_model.as_mut() else {
                    return Ok(());
                };
                hud_model.meter = Some(desktop_hud_audio_meter_model_for_waveform(next_bins));
                Some(RecordingHudRefreshUpdate::WaveformOnly)
            }
        } else {
            None
        };

        if matches!(
            updated_hud_state,
            Some(RecordingHudRefreshUpdate::WaveformOnly)
        ) {
            invalidate_recording_hud_waveform(state);
            return Ok(());
        }
        let Some(RecordingHudRefreshUpdate::Text(updated_hud_model)) = updated_hud_state else {
            return Ok(());
        };
        let text_changed = true;
        if state.hud_hwnd.get().is_null() {
            return Ok(());
        }
        let dpi = overlay_dpi_for_window(state.hud_hwnd.get());
        let metrics =
            scale_hud_metrics_for_dpi(desktop_hud_metrics_for_view_model(&updated_hud_model), dpi);
        let (screen_width, screen_height) = current_screen_size();
        let x = ((screen_width - metrics.width).max(0)) / 2;
        let y = (screen_height - metrics.height - metrics.bottom_margin).max(0);
        let next_geometry = DesktopHudGeometry {
            x,
            y,
            width: metrics.width,
            height: metrics.height,
            corner_radius: metrics.corner_radius,
        };
        let (geometry_plan, dirty_content_rect) = if let Ok(mut overlay) = overlay_ui_state().lock()
        {
            let plan = desktop_hud_geometry_update_plan(overlay.hud_geometry, next_geometry);
            overlay.hud_geometry = Some(next_geometry);
            if desktop_listening_hud_requires_text_layout(
                text_changed,
                plan.reposition || plan.reshape,
                false,
            ) {
                if let Some(partial_layout) = desktop_listening_hud_partial_text_layout(
                    metrics.width,
                    metrics.height,
                    dpi,
                    updated_hud_model.detail.as_deref(),
                ) {
                    overlay.hud_streaming_partial_layout = Some(partial_layout);
                    let max_offset = desktop_listening_hud_scroll_max_offset(&partial_layout);
                    let (next_offset, next_user_scrolled) =
                        desktop_listening_hud_reconcile_scroll_state(
                            overlay.hud_streaming_scroll_line_offset,
                            max_offset,
                            overlay.hud_streaming_scroll_user_scrolled,
                        );
                    overlay.hud_streaming_scroll_line_offset = next_offset;
                    overlay.hud_streaming_scroll_user_scrolled = next_user_scrolled;
                } else {
                    overlay.hud_streaming_partial_layout = None;
                    overlay.hud_streaming_scroll_line_offset = 0;
                    overlay.hud_streaming_scroll_user_scrolled = false;
                }
            }
            // The transcript, its scrollbar and the waveform are the only parts
            // of the listening HUD a level refresh can change; the shell border
            // and the two round buttons stay put, so keep them out of the dirty
            // region instead of invalidating the whole window every ~48 ms.
            let content_rect = overlay.hud_streaming_partial_layout.map(|layout| {
                let mut rect = desktop_overlay_rect_to_rect(layout.text_rect);
                union_overlay_rect(
                    &mut rect,
                    desktop_overlay_rect_to_rect(layout.waveform_rect),
                );
                if let Some(scrollbar_rect) = layout.scrollbar_rect {
                    union_overlay_rect(&mut rect, desktop_overlay_rect_to_rect(scrollbar_rect));
                }
                rect
            });
            (plan, content_rect)
        } else {
            (desktop_hud_geometry_update_plan(None, next_geometry), None)
        };
        let waveform = desktop_listening_hud_waveform_rect(metrics.width, metrics.height, dpi);
        let waveform_rect = RECT {
            left: waveform.left,
            top: waveform.top,
            right: waveform.right,
            bottom: waveform.bottom,
        };
        unsafe {
            if geometry_plan.reposition {
                SetWindowPos(
                    state.hud_hwnd.get(),
                    (-1isize) as HWND,
                    x,
                    y,
                    metrics.width,
                    metrics.height,
                    SWP_NOACTIVATE,
                );
            }
            if geometry_plan.reshape {
                apply_rounded_window_region(
                    state.hud_hwnd.get(),
                    metrics.width,
                    metrics.height,
                    metrics.corner_radius,
                );
            }
            if geometry_plan.reposition || geometry_plan.reshape {
                InvalidateRect(state.hud_hwnd.get(), ptr::null(), 0);
            } else if let Some(dirty_rect) = dirty_content_rect {
                InvalidateRect(state.hud_hwnd.get(), &dirty_rect, 0);
            } else if text_changed {
                InvalidateRect(state.hud_hwnd.get(), ptr::null(), 0);
            } else {
                InvalidateRect(state.hud_hwnd.get(), &waveform_rect, 0);
            }
        }
        Ok(())
    }

    fn refresh_thinking_hud_progress(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        let mut should_invalidate = false;
        if let Ok(mut overlay) = overlay_ui_state().lock() {
            if matches!(
                overlay.hud_model.as_ref().map(|hud| hud.visual_state),
                Some(DesktopHudVisualState::Thinking)
            ) {
                overlay.hud_thinking_pulse_tick = overlay.hud_thinking_pulse_tick.wrapping_add(1);
                should_invalidate = true;
            } else {
                unsafe {
                    KillTimer(hwnd, TIMER_THINKING_PROGRESS);
                }
            }
        }
        if should_invalidate && !state.hud_hwnd.get().is_null() {
            unsafe {
                // erase = FALSE: the thinking HUD paints through a cached back
                // buffer, so a system erase pass only adds a full-window
                // background blit that the back buffer immediately overwrites.
                InvalidateRect(state.hud_hwnd.get(), ptr::null(), 0);
            }
        }
        Ok(())
    }

    fn refresh_foreground_insert_target_focus_capture(
        target: ForegroundInsertTarget,
        shell_hwnd: HWND,
        hud_hwnd: HWND,
    ) -> ForegroundInsertTarget {
        let target_hwnd = target.window_handle as HWND;
        let focus = capture_foreground_focus_target(target_hwnd, shell_hwnd, hud_hwnd);
        hydrate_foreground_insert_target_focus(
            target,
            focus.primary_focus_hwnd.map(|handle| handle as isize),
            focus.fallback_focus_hwnd.map(|handle| handle as isize),
            shell_hwnd as isize,
            hud_hwnd as isize,
        )
    }

    fn capture_explicit_insert_target_from_env(
        shell_hwnd: HWND,
        hud_hwnd: HWND,
    ) -> Option<ForegroundInsertTarget> {
        let window_value = std::env::var(TALK_DESKTOP_INSERT_TARGET_WINDOW_ENV).ok()?;
        let window_handle = match parse_desktop_window_handle(&window_value) {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!(
                    "Talk desktop ignored {}='{}': {}",
                    TALK_DESKTOP_INSERT_TARGET_WINDOW_ENV, window_value, error
                );
                return None;
            }
        };

        let focus_handle = match std::env::var(TALK_DESKTOP_INSERT_TARGET_FOCUS_ENV) {
            Ok(value) => match parse_desktop_window_handle(&value) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    eprintln!(
                        "Talk desktop ignored {}='{}': {}",
                        TALK_DESKTOP_INSERT_TARGET_FOCUS_ENV, value, error
                    );
                    None
                }
            },
            Err(_) => None,
        };

        let mut target = select_foreground_insert_target(
            window_handle,
            focus_handle,
            shell_hwnd as isize,
            hud_hwnd as isize,
        )?;
        if let Some(focus_handle) = target.focus_handle {
            target.primary_focus_handle = Some(focus_handle);
        }
        Some(target)
    }

    fn capture_foreground_insert_target_context(
        shell_hwnd: HWND,
        hud_hwnd: HWND,
    ) -> Option<DesktopInsertTargetContext> {
        capture_foreground_insert_target_context_impl(shell_hwnd, hud_hwnd, true)
    }

    /// Cheap press-time snapshot: HWND-level capture only, without any UI
    /// Automation calls. Safe to run inside latency-sensitive paths such as
    /// the WH_KEYBOARD_LL hook callback; automation fields are enriched later
    /// on a background thread.
    fn capture_foreground_insert_target_context_snapshot(
        shell_hwnd: HWND,
        hud_hwnd: HWND,
    ) -> Option<DesktopInsertTargetContext> {
        capture_foreground_insert_target_context_impl(shell_hwnd, hud_hwnd, false)
    }

    fn capture_foreground_insert_target_context_impl(
        shell_hwnd: HWND,
        hud_hwnd: HWND,
        include_automation: bool,
    ) -> Option<DesktopInsertTargetContext> {
        if let Some(target) = capture_explicit_insert_target_from_env(shell_hwnd, hud_hwnd) {
            return Some(DesktopInsertTargetContext {
                target: Some(target),
                focus_class_name: None,
                caret_window_handle: target.focus_handle,
                automation_control_type: None,
                automation_framework_id: None,
                automation_runtime_id: None,
                automation_is_keyboard_focusable: None,
                automation_supports_text_pattern: false,
                automation_supports_value_pattern: false,
            });
        }

        let foreground = unsafe { GetForegroundWindow() };
        let focus = capture_foreground_focus_target(foreground, shell_hwnd, hud_hwnd);
        let automation_focus = if include_automation {
            capture_automation_focus_target(foreground)
        } else {
            CapturedAutomationFocusTarget::default()
        };
        let target = select_foreground_insert_target(
            foreground as isize,
            focus.focus_hwnd.map(|handle| handle as isize),
            shell_hwnd as isize,
            hud_hwnd as isize,
        )?;
        Some(DesktopInsertTargetContext {
            target: Some(target),
            focus_class_name: focus.focus_class_name,
            caret_window_handle: focus.caret_hwnd.map(|handle| handle as isize),
            automation_control_type: automation_focus.control_type,
            automation_framework_id: automation_focus.framework_id,
            automation_runtime_id: automation_focus.runtime_id,
            automation_is_keyboard_focusable: automation_focus.is_keyboard_focusable,
            automation_supports_text_pattern: automation_focus.supports_text_pattern,
            automation_supports_value_pattern: automation_focus.supports_value_pattern,
        })
    }

    fn capture_automation_focus_target(foreground_hwnd: HWND) -> CapturedAutomationFocusTarget {
        ensure_uia_com_initialized_for_current_thread();
        let automation = match UIAutomation::new() {
            Ok(automation) => automation,
            Err(_) => return CapturedAutomationFocusTarget::default(),
        };
        if let Ok(element) = automation.get_focused_element() {
            let captured = capture_automation_focus_target_from_element(&element);
            if captured.control_type.is_some()
                || captured.framework_id.is_some()
                || captured.runtime_id.is_some()
                || captured.supports_text_pattern
                || captured.supports_value_pattern
            {
                return captured;
            }
        }

        capture_automation_focus_target_from_window(&automation, foreground_hwnd)
            .unwrap_or_default()
    }

    fn capture_automation_focus_target_from_window(
        automation: &UIAutomation,
        foreground_hwnd: HWND,
    ) -> Option<CapturedAutomationFocusTarget> {
        if foreground_hwnd.is_null() {
            return None;
        }

        let window_element = automation
            .element_from_handle(UiAutomationHandle::from(WinHwnd(foreground_hwnd)))
            .ok()?;

        let mut candidates = Vec::new();
        if window_element.has_keyboard_focus().ok() == Some(true)
            && automation_ui_element_looks_editable(&window_element)
        {
            candidates.push(window_element.clone());
        }

        if let Ok(elements) = automation
            .create_matcher()
            .from(window_element)
            .depth(32)
            .timeout(0)
            .filter_fn(Box::new(|element: &UIElement| {
                Ok(element.has_keyboard_focus().ok() == Some(true)
                    && automation_ui_element_looks_editable(element))
            }))
            .find_all()
        {
            candidates.extend(elements);
        }

        candidates
            .into_iter()
            .max_by_key(automation_ui_element_capture_score)
            .map(|element| capture_automation_focus_target_from_element(&element))
    }

    fn capture_automation_focus_target_from_element(
        element: &UIElement,
    ) -> CapturedAutomationFocusTarget {
        let control_type = element
            .get_control_type()
            .ok()
            .map(automation_control_type_label);
        let framework_id = element
            .get_framework_id()
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let runtime_id = element
            .get_runtime_id()
            .ok()
            .filter(|value| !value.is_empty());
        let is_keyboard_focusable = element.is_keyboard_focusable().ok();
        let supports_text_pattern = element.get_pattern::<UITextPattern>().is_ok();
        let supports_value_pattern = element.get_pattern::<UIValuePattern>().is_ok();

        CapturedAutomationFocusTarget {
            control_type,
            framework_id,
            runtime_id,
            is_keyboard_focusable,
            supports_text_pattern,
            supports_value_pattern,
        }
    }

    fn automation_ui_element_looks_editable(element: &UIElement) -> bool {
        if element.is_keyboard_focusable().ok() == Some(false) {
            return false;
        }

        let supports_text_pattern = element.get_pattern::<UITextPattern>().is_ok();
        let supports_value_pattern = element.get_pattern::<UIValuePattern>().is_ok();
        if supports_text_pattern || supports_value_pattern {
            return true;
        }

        matches!(
            element
                .get_control_type()
                .ok()
                .map(automation_control_type_label)
                .as_deref(),
            Some("edit") | Some("document")
        )
    }

    fn automation_ui_element_capture_score(element: &UIElement) -> u32 {
        let mut score = 0;
        if element.has_keyboard_focus().ok() == Some(true) {
            score += 16;
        }
        if element.is_keyboard_focusable().ok() == Some(true) {
            score += 4;
        }
        if element.get_pattern::<UIValuePattern>().is_ok() {
            score += 8;
        }
        if element.get_pattern::<UITextPattern>().is_ok() {
            score += 6;
        }
        if element
            .get_runtime_id()
            .ok()
            .filter(|value| !value.is_empty())
            .is_some()
        {
            score += 4;
        }
        match element
            .get_control_type()
            .ok()
            .map(automation_control_type_label)
            .as_deref()
        {
            Some("edit") => score += 6,
            Some("document") => score += 3,
            _ => {}
        }

        score
    }

    fn automation_control_type_label(control_type: UiAutomationControlType) -> String {
        format!("{control_type:?}").to_ascii_lowercase()
    }

    fn capture_foreground_focus_target(
        target_hwnd: HWND,
        shell_hwnd: HWND,
        hud_hwnd: HWND,
    ) -> CapturedForegroundFocusTarget {
        if target_hwnd.is_null() {
            return CapturedForegroundFocusTarget {
                focus_hwnd: None,
                primary_focus_hwnd: None,
                fallback_focus_hwnd: None,
                caret_hwnd: None,
                focus_class_name: None,
            };
        }

        unsafe {
            if IsWindow(target_hwnd) == 0 {
                return CapturedForegroundFocusTarget {
                    focus_hwnd: None,
                    primary_focus_hwnd: None,
                    fallback_focus_hwnd: None,
                    caret_hwnd: None,
                    focus_class_name: None,
                };
            }

            let target_thread = GetWindowThreadProcessId(target_hwnd, ptr::null_mut());
            if target_thread == 0 {
                return CapturedForegroundFocusTarget {
                    focus_hwnd: None,
                    primary_focus_hwnd: None,
                    fallback_focus_hwnd: None,
                    caret_hwnd: None,
                    focus_class_name: None,
                };
            }

            let mut gui_info = GUITHREADINFO {
                cbSize: mem::size_of::<GUITHREADINFO>() as u32,
                ..mem::zeroed()
            };
            let gui_thread_focus = if GetGUIThreadInfo(target_thread, &mut gui_info) == 0 {
                None
            } else {
                normalize_focus_capture_candidate(gui_info.hwndFocus)
            };
            let caret_hwnd = normalize_focus_capture_candidate(gui_info.hwndCaret);

            let current_thread = GetCurrentThreadId();
            let attached = if current_thread != target_thread {
                AttachThreadInput(current_thread, target_thread, 1) != 0
            } else {
                false
            };
            let attached_thread_focus = if current_thread == target_thread || attached {
                normalize_focus_capture_candidate(GetFocus())
            } else {
                None
            };
            if attached {
                let _ = AttachThreadInput(current_thread, target_thread, 0);
            }

            let resolution = resolve_foreground_focus_capture(
                target_hwnd as isize,
                gui_thread_focus.map(|handle| handle as isize),
                attached_thread_focus.map(|handle| handle as isize),
                shell_hwnd as isize,
                hud_hwnd as isize,
            );
            let resolved_focus_hwnd = resolution.focus_handle.map(|handle| handle as HWND);
            CapturedForegroundFocusTarget {
                focus_hwnd: resolved_focus_hwnd,
                primary_focus_hwnd: gui_thread_focus,
                fallback_focus_hwnd: attached_thread_focus,
                caret_hwnd,
                focus_class_name: resolved_focus_hwnd.and_then(window_class_name),
            }
        }
    }

    fn normalize_focus_capture_candidate(candidate: HWND) -> Option<HWND> {
        if candidate.is_null() {
            return None;
        }

        unsafe { (IsWindow(candidate) != 0).then_some(candidate) }
    }

    fn window_class_name(hwnd: HWND) -> Option<String> {
        if hwnd.is_null() {
            return None;
        }

        let mut buffer = [0u16; 256];
        let length = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW(
                hwnd,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            )
        };
        if length <= 0 {
            return None;
        }

        Some(String::from_utf16_lossy(&buffer[..length as usize]))
    }

    fn resolve_window_process_base_name(window_handle: isize) -> Option<String> {
        let hwnd = window_handle as HWND;
        if hwnd.is_null() {
            return None;
        }

        unsafe {
            if IsWindow(hwnd) == 0 {
                return None;
            }

            let mut process_id = 0u32;
            let _thread_id = GetWindowThreadProcessId(hwnd, &mut process_id);
            if process_id == 0 {
                return None;
            }

            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
            if process.is_null() {
                return None;
            }

            let mut buffer = vec![0u16; 260];
            let mut length = buffer.len() as u32;
            let success = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut length,
            ) != 0;
            let _ = CloseHandle(process);
            if !success || length == 0 {
                return None;
            }

            let process_path = String::from_utf16_lossy(&buffer[..length as usize]);
            Path::new(&process_path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        }
    }

    fn begin_foreground_insert_target_restore(target_hwnd: HWND) -> bool {
        if target_hwnd.is_null() {
            return false;
        }

        unsafe {
            if IsWindow(target_hwnd) == 0 {
                return false;
            }

            if GetForegroundWindow() == target_hwnd && IsIconic(target_hwnd) == 0 {
                return false;
            }

            ShowWindow(target_hwnd, SW_RESTORE);
            SetWindowPos(
                target_hwnd,
                (-1isize) as HWND,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }

        thread::sleep(Duration::from_millis(60));
        unsafe {
            BringWindowToTop(target_hwnd);
            SetForegroundWindow(target_hwnd);
        }
        thread::sleep(Duration::from_millis(80));
        true
    }

    fn end_foreground_insert_target_restore(target_hwnd: HWND) {
        if target_hwnd.is_null() {
            return;
        }

        unsafe {
            if IsWindow(target_hwnd) == 0 {
                return;
            }

            SetWindowPos(
                target_hwnd,
                (-2isize) as HWND,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
    }

    fn restore_foreground_focus_target(target_hwnd: HWND, focus_hwnd: HWND) {
        if target_hwnd.is_null() || focus_hwnd.is_null() {
            return;
        }

        unsafe {
            if IsWindow(target_hwnd) == 0 || IsWindow(focus_hwnd) == 0 {
                return;
            }

            let current_thread = GetCurrentThreadId();
            let target_thread = GetWindowThreadProcessId(target_hwnd, ptr::null_mut());
            if target_thread == 0 {
                return;
            }

            let attached = if current_thread != target_thread {
                AttachThreadInput(current_thread, target_thread, 1) != 0
            } else {
                false
            };

            let _ = SetActiveWindow(target_hwnd);
            let _ = SetFocus(focus_hwnd);

            if attached {
                let _ = AttachThreadInput(current_thread, target_thread, 0);
            }
        }

        if unsafe { GetForegroundWindow() } != target_hwnd {
            thread::sleep(Duration::from_millis(40));
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct PostInsertForegroundHoldOutcome {
        release_reason: ForegroundTargetReleaseReason,
        wait_duration_ms: u64,
        required_stable_foreground_polls: u32,
        progress: ForegroundTargetStabilityProgress,
    }

    fn refresh_post_insert_foreground_target(target: ForegroundInsertTarget) {
        let target_hwnd = target.window_handle as HWND;
        let _ = begin_foreground_insert_target_restore(target_hwnd);
        if let Some(focus_handle) = target.focus_handle {
            restore_foreground_focus_target(target_hwnd, focus_handle as HWND);
        }
    }

    fn wait_for_post_insert_foreground_stability(
        target: ForegroundInsertTarget,
    ) -> PostInsertForegroundHoldOutcome {
        let start = Instant::now();
        let mut progress = ForegroundTargetStabilityProgress::default();
        let target_hwnd = target.window_handle as HWND;

        loop {
            let observed_foreground_hwnd = unsafe { GetForegroundWindow() };
            progress = observe_foreground_target_stability(
                progress,
                target_hwnd as isize,
                observed_foreground_hwnd as isize,
            );

            if foreground_target_stability_satisfied(
                progress,
                INSERT_TARGET_POST_INSERT_REQUIRED_STABLE_FOREGROUND_POLLS,
            ) {
                return PostInsertForegroundHoldOutcome {
                    release_reason: ForegroundTargetReleaseReason::TargetStable,
                    wait_duration_ms: start.elapsed().as_millis() as u64,
                    required_stable_foreground_polls:
                        INSERT_TARGET_POST_INSERT_REQUIRED_STABLE_FOREGROUND_POLLS,
                    progress,
                };
            }

            if foreground_target_refresh_requested(
                target.window_handle,
                observed_foreground_hwnd as isize,
            ) {
                refresh_post_insert_foreground_target(target);
            }

            if start.elapsed() >= Duration::from_millis(INSERT_TARGET_POST_INSERT_MAX_HOLD_MS) {
                return PostInsertForegroundHoldOutcome {
                    release_reason: ForegroundTargetReleaseReason::Timeout,
                    wait_duration_ms: start.elapsed().as_millis() as u64,
                    required_stable_foreground_polls:
                        INSERT_TARGET_POST_INSERT_REQUIRED_STABLE_FOREGROUND_POLLS,
                    progress,
                };
            }

            thread::sleep(Duration::from_millis(
                INSERT_TARGET_POST_INSERT_POLL_INTERVAL_MS,
            ));
        }
    }

    fn begin_restore_foreground_insert_target(
        target: ForegroundInsertTarget,
        shell_hwnd: HWND,
        hud_hwnd: HWND,
    ) -> (ForegroundInsertTarget, DesktopInsertTargetRestoreDiagnostic) {
        let target_hwnd = target.window_handle as HWND;
        let target_window_exists = unsafe {
            if target_hwnd.is_null() {
                None
            } else {
                Some(IsWindow(target_hwnd) != 0)
            }
        };

        let restore_applied = begin_foreground_insert_target_restore(target_hwnd);
        let restored_target =
            refresh_foreground_insert_target_focus_capture(target, shell_hwnd, hud_hwnd);
        let target_focus_exists = restored_target.focus_handle.map(|focus_handle| unsafe {
            let focus_hwnd = focus_handle as HWND;
            if focus_hwnd.is_null() {
                false
            } else {
                IsWindow(focus_hwnd) != 0
            }
        });

        if let Some(focus_handle) = restored_target.focus_handle {
            restore_foreground_focus_target(target_hwnd, focus_handle as HWND);
        }

        (
            restored_target,
            DesktopInsertTargetRestoreDiagnostic {
                attempted: true,
                target_window_exists,
                target_focus_exists,
                focus_restore_requested: restored_target.focus_handle.is_some(),
                restore_applied,
                post_insert_release_reason: None,
                post_insert_wait_duration_ms: None,
                post_insert_poll_count: None,
                post_insert_target_foreground_poll_count: None,
                post_insert_trailing_target_foreground_poll_count: None,
                post_insert_required_stable_foreground_polls: None,
            },
        )
    }

    fn end_restore_foreground_insert_target(
        target: ForegroundInsertTarget,
        restore_applied: bool,
    ) -> PostInsertForegroundHoldOutcome {
        if !restore_applied {
            return PostInsertForegroundHoldOutcome {
                release_reason: ForegroundTargetReleaseReason::TargetStable,
                wait_duration_ms: 0,
                required_stable_foreground_polls: 0,
                progress: ForegroundTargetStabilityProgress::default(),
            };
        }

        let target_hwnd = target.window_handle as HWND;
        let post_insert_hold = wait_for_post_insert_foreground_stability(target);
        end_foreground_insert_target_restore(target_hwnd);
        post_insert_hold
    }

    fn persist_insert_target_diagnostic_if_available(
        session_log_path: &Path,
        target: Option<ForegroundInsertTarget>,
        origin_context: Option<&DesktopInsertTargetContext>,
        current_context: Option<&DesktopInsertTargetContext>,
        origin_source: Option<&str>,
        pending_hotkey_origin_context: Option<&DesktopInsertTargetContext>,
        release_time_origin_context: Option<&DesktopInsertTargetContext>,
        output_strategy: Option<DesktopOutputStrategy>,
        restore: Option<DesktopInsertTargetRestoreDiagnostic>,
    ) {
        let Some(target) = target else {
            return;
        };

        let trace = build_desktop_insert_target_trace_diagnostic(
            origin_source,
            origin_context,
            current_context,
            pending_hotkey_origin_context,
            release_time_origin_context,
        );
        let diagnostic = build_desktop_insert_target_diagnostic_with_trace(
            target,
            current_context,
            output_strategy,
            restore,
            trace,
        );
        if let Err(error) = write_desktop_insert_target_diagnostic(session_log_path, &diagnostic) {
            eprintln!(
                "Talk desktop insert-target diagnostic write failed for {}: {error}",
                session_log_path.display()
            );
        }
    }

    fn hide_hud(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd)? };
        if !state.hud_hwnd.get().is_null() {
            unsafe {
                KillTimer(hwnd, TIMER_HIDE_HUD);
                KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                KillTimer(hwnd, TIMER_THINKING_PROGRESS);
                if let Ok(mut overlay) = overlay_ui_state().lock() {
                    overlay.hud_model = None;
                    overlay.hud_geometry = None;
                    overlay.hud_meter_bins = [0.0; 9];
                    overlay.hud_streaming_corrected_prefix = None;
                    overlay.hud_streaming_partial_tail = None;
                    overlay.hud_streaming_partial_layout = None;
                    overlay.hud_streaming_scroll_line_offset = 0;
                    overlay.hud_streaming_scroll_dragging = false;
                    overlay.hud_streaming_scroll_user_scrolled = false;
                    overlay.hud_thinking_pulse_tick = 0;
                }
                ShowWindow(state.hud_hwnd.get(), SW_HIDE);
            }
        }
        Ok(())
    }

    fn listening_hud_partial_layout_and_transcript(
        hud_hwnd: HWND,
    ) -> Option<(DesktopListeningHudPartialTextLayout, String, String, usize)> {
        let mut rect = RECT::default();
        unsafe {
            GetClientRect(hud_hwnd, &mut rect);
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let dpi = overlay_dpi_for_window(hud_hwnd);
        let overlay = overlay_ui_state().lock().ok()?;
        let model = overlay.hud_model.as_ref()?;
        if model.visual_state != DesktopHudVisualState::Listening {
            return None;
        }

        let corrected_prefix = overlay
            .hud_streaming_corrected_prefix
            .clone()
            .unwrap_or_default();
        let partial_tail = overlay
            .hud_streaming_partial_tail
            .clone()
            .or_else(|| model.detail.clone())
            .unwrap_or_default();
        let presented_parts = listening_hud_presented_transcript_parts(
            &DesktopStreamingHudTranscriptParts {
                corrected_prefix,
                pre_recognized_tail: partial_tail,
            },
            overlay.hud_streaming_scroll_user_scrolled,
        );
        let combined = format!(
            "{}{}",
            presented_parts.corrected_prefix, presented_parts.pre_recognized_tail
        );
        let layout = overlay.hud_streaming_partial_layout.or_else(|| {
            desktop_listening_hud_partial_text_layout(width, height, dpi, Some(&combined))
        })?;
        let scroll_offset = overlay
            .hud_streaming_scroll_line_offset
            .min(desktop_listening_hud_scroll_max_offset(&layout));
        Some((
            layout,
            presented_parts.corrected_prefix,
            presented_parts.pre_recognized_tail,
            scroll_offset,
        ))
    }

    fn update_listening_hud_scroll_from_pointer(
        hud_hwnd: HWND,
        point: POINT,
        require_hit_test: bool,
    ) -> bool {
        let Some((layout, _, _, _)) = listening_hud_partial_layout_and_transcript(hud_hwnd) else {
            return false;
        };
        let dpi = overlay_dpi_for_window(hud_hwnd);
        let Some(scrollbar_hit_rect) = desktop_listening_hud_scrollbar_hit_rect(&layout, dpi)
        else {
            return false;
        };
        if require_hit_test && !scrollbar_hit_rect.contains(point.x, point.y) {
            return false;
        }

        let next_offset =
            desktop_listening_hud_scroll_line_offset_for_pointer(&layout, dpi, point.y);
        let changed = if let Ok(mut overlay) = overlay_ui_state().lock() {
            let max_offset = desktop_listening_hud_scroll_max_offset(&layout);
            overlay.hud_streaming_scroll_user_scrolled =
                !desktop_listening_hud_auto_follow_for_scroll(next_offset, max_offset);
            if overlay.hud_streaming_scroll_line_offset != next_offset {
                overlay.hud_streaming_scroll_line_offset = next_offset;
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            unsafe {
                InvalidateRect(hud_hwnd, ptr::null(), 0);
            }
        }
        true
    }

    fn wheel_delta_from_wparam(wparam: WPARAM) -> i16 {
        ((wparam >> 16) & 0xFFFF) as u16 as i16
    }

    fn handle_listening_hud_mouse_wheel(hud_hwnd: HWND, wheel_delta: i16) {
        let Some((layout, _, _, current_offset)) =
            listening_hud_partial_layout_and_transcript(hud_hwnd)
        else {
            return;
        };
        let max_offset = desktop_listening_hud_scroll_max_offset(&layout);
        let next_offset = desktop_listening_hud_scroll_line_offset_for_wheel(
            current_offset,
            max_offset,
            wheel_delta,
        );
        let changed = if let Ok(mut overlay) = overlay_ui_state().lock() {
            overlay.hud_streaming_scroll_user_scrolled =
                !desktop_listening_hud_auto_follow_for_scroll(next_offset, max_offset);
            if overlay.hud_streaming_scroll_line_offset != next_offset {
                overlay.hud_streaming_scroll_line_offset = next_offset;
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            unsafe {
                InvalidateRect(hud_hwnd, ptr::null(), 0);
            }
        }
    }

    fn handle_listening_hud_mouse_down(hud_hwnd: HWND, point: POINT) {
        if update_listening_hud_scroll_from_pointer(hud_hwnd, point, true) {
            if let Ok(mut overlay) = overlay_ui_state().lock() {
                overlay.hud_streaming_scroll_dragging = true;
            }
            unsafe {
                SetCapture(hud_hwnd);
            }
        }
    }

    fn handle_listening_hud_mouse_move(hud_hwnd: HWND, point: POINT) {
        if overlay_ui_state()
            .lock()
            .ok()
            .is_some_and(|overlay| overlay.hud_streaming_scroll_dragging)
        {
            let _ = update_listening_hud_scroll_from_pointer(hud_hwnd, point, false);
        }
    }

    fn handle_listening_hud_mouse_up(hud_hwnd: HWND, point: POINT) {
        let was_dragging = if let Ok(mut overlay) = overlay_ui_state().lock() {
            let dragging = overlay.hud_streaming_scroll_dragging;
            overlay.hud_streaming_scroll_dragging = false;
            dragging
        } else {
            false
        };
        if was_dragging {
            let _ = update_listening_hud_scroll_from_pointer(hud_hwnd, point, false);
            unsafe {
                ReleaseCapture();
            }
            return;
        }

        handle_listening_hud_click(hud_hwnd, point);
    }

    fn handle_listening_hud_click(hud_hwnd: HWND, point: POINT) {
        let owner_hwnd = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindow(
                hud_hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::GW_OWNER,
            )
        };
        if owner_hwnd.is_null() {
            return;
        }

        let model = overlay_ui_state()
            .lock()
            .ok()
            .and_then(|overlay| overlay.hud_model.clone());
        if !matches!(
            model.as_ref().map(|item| item.visual_state),
            Some(DesktopHudVisualState::Listening)
        ) {
            return;
        }

        let mut rect = RECT::default();
        unsafe {
            GetClientRect(hud_hwnd, &mut rect);
        }
        let dpi = overlay_dpi_for_window(hud_hwnd);
        match desktop_listening_hud_action_for_point(
            rect.right - rect.left,
            rect.bottom - rect.top,
            dpi,
            point.x,
            point.y,
        ) {
            DesktopListeningHudAction::Cancel => {
                let _ = cancel_active_recording(owner_hwnd);
            }
            DesktopListeningHudAction::Complete => {
                let generation = unsafe { get_window_state(owner_hwnd) }
                    .ok()
                    .and_then(|state| {
                        state.shared.lock().ok().and_then(|shared| {
                            shared
                                .active_recording
                                .as_ref()
                                .map(|active| active.generation)
                        })
                    });
                if let Some(generation) = generation {
                    request_stop_recording(owner_hwnd, generation);
                }
            }
            DesktopListeningHudAction::Ignore => {}
        }
    }

    fn copy_popup_visible_panes(model: &DesktopCopyPopupModel) -> Vec<DesktopCopyPopupPaneModel> {
        let mut panes = if model.panes.is_empty() {
            vec![DesktopCopyPopupPaneModel {
                label: String::new(),
                text: model.editable_text.clone(),
                editable: true,
                copy_default: true,
            }]
        } else {
            model
                .panes
                .iter()
                .take(COPY_POPUP_MAX_PANES)
                .cloned()
                .collect::<Vec<_>>()
        };

        if !panes.iter().any(|pane| pane.copy_default) {
            if let Some(first) = panes.first_mut() {
                first.copy_default = true;
            }
        }
        panes
    }

    fn copy_popup_default_pane_index(panes: &[DesktopCopyPopupPaneModel]) -> usize {
        panes.iter().position(|pane| pane.copy_default).unwrap_or(0)
    }

    fn copy_popup_metrics_for_model(
        model: &DesktopCopyPopupModel,
        dpi: u32,
    ) -> DesktopCopyPopupMetrics {
        let base = desktop_copy_popup_metrics();
        let pane_count = copy_popup_visible_panes(model).len();
        let height = if pane_count <= 1 {
            base.height
        } else {
            244 + ((pane_count.saturating_sub(2)) as i32 * 72)
        };
        scale_copy_popup_metrics_for_dpi(
            DesktopCopyPopupMetrics {
                width: base.width,
                height,
                bottom_margin: base.bottom_margin,
            },
            dpi,
        )
    }

    fn ensure_copy_popup_edit_controls(
        owner_hwnd: HWND,
        copy_popup_hwnd: HWND,
        model: &DesktopCopyPopupModel,
        dpi: u32,
    ) -> Result<()> {
        let panes = copy_popup_visible_panes(model);
        let pane_count = panes.len().max(1);
        let state = unsafe { get_window_state(owner_hwnd)? };

        while state.copy_popup_pane_edit_hwnds.borrow().len() < pane_count {
            let index = state.copy_popup_pane_edit_hwnds.borrow().len();
            let edit_hwnd = create_copy_popup_edit_control(
                copy_popup_hwnd,
                dpi,
                copy_popup_edit_control_id(index),
            )?;
            state
                .copy_popup_pane_edit_hwnds
                .borrow_mut()
                .push(edit_hwnd);
        }

        for (index, edit_hwnd) in state.copy_popup_pane_edit_hwnds.borrow().iter().enumerate() {
            unsafe {
                if index < pane_count {
                    let pane = &panes[index];
                    let read_only = if pane.editable { 0 } else { 1 };
                    let _ = SendMessageW(*edit_hwnd, EM_SETREADONLY_MESSAGE, read_only, 0);
                    SetWindowTextW(*edit_hwnd, to_wide(&pane.text).as_ptr());
                    ShowWindow(*edit_hwnd, SW_SHOW);
                } else {
                    SetWindowTextW(*edit_hwnd, to_wide("").as_ptr());
                    ShowWindow(*edit_hwnd, SW_HIDE);
                }
            }
        }

        let default_index = copy_popup_default_pane_index(&panes).min(pane_count - 1);
        state
            .copy_popup_edit_hwnd
            .set(state.copy_popup_pane_edit_hwnds.borrow()[default_index]);
        Ok(())
    }

    fn show_copy_popup(hwnd: HWND, model: DesktopCopyPopupModel) -> Result<()> {
        let copy_popup_hwnd = {
            let state = unsafe { get_window_state(hwnd)? };
            state.copy_popup_hwnd.get()
        };
        if copy_popup_hwnd.is_null() {
            anyhow::bail!("Talk desktop copy popup is unavailable");
        }

        let popup_dpi = overlay_dpi_for_window(copy_popup_hwnd);
        let metrics = copy_popup_metrics_for_model(&model, popup_dpi);
        let (screen_width, screen_height) = current_screen_size();
        let position = desktop_copy_popup_position(
            screen_width,
            screen_height,
            metrics.width,
            metrics.height,
            metrics.bottom_margin,
        );

        if let Ok(mut overlay) = overlay_ui_state().lock() {
            overlay.copy_popup = Some(CopyPopupRenderState {
                model: model.clone(),
                hovered_control: CopyPopupHoveredControl::None,
                pressed_control: CopyPopupHoveredControl::None,
                keyboard_focused_control: CopyPopupHoveredControl::None,
            });
        }

        ensure_copy_popup_edit_controls(hwnd, copy_popup_hwnd, &model, popup_dpi)?;

        unsafe {
            SetWindowPos(
                copy_popup_hwnd,
                (-1isize) as HWND,
                position.x,
                position.y,
                metrics.width,
                metrics.height,
                SWP_NOACTIVATE,
            );
            apply_rounded_window_region(
                copy_popup_hwnd,
                metrics.width,
                metrics.height,
                scale_desktop_overlay_length(COPY_POPUP_CORNER_RADIUS, popup_dpi),
            );
            let _ = update_copy_popup_edit_layout(hwnd, copy_popup_hwnd);
            InvalidateRect(copy_popup_hwnd, ptr::null(), 0);
            match desktop_copy_popup_activation_policy() {
                DesktopOverlayActivationPolicy::NoActivate => {
                    ShowWindow(copy_popup_hwnd, SW_SHOWNOACTIVATE);
                }
                DesktopOverlayActivationPolicy::ActivateOnInteract => {
                    ShowWindow(copy_popup_hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
        Ok(())
    }

    fn hide_copy_popup(hwnd: HWND) -> Result<()> {
        let (
            copy_popup_hwnd,
            edit_hwnds,
            restore_foreground_hwnd,
            restore_focus_hwnd,
            should_restore_focus,
        ) = {
            let state = unsafe { get_window_state(hwnd)? };
            let should_restore_focus = !state.copy_popup_hwnd.get().is_null()
                && unsafe { GetForegroundWindow() == state.copy_popup_hwnd.get() };
            let restore_foreground_hwnd = state.copy_popup_restore_foreground_hwnd.get();
            let restore_focus_hwnd = state.copy_popup_restore_focus_hwnd.get();
            state
                .copy_popup_restore_foreground_hwnd
                .set(ptr::null_mut());
            state.copy_popup_restore_focus_hwnd.set(ptr::null_mut());
            (
                state.copy_popup_hwnd.get(),
                state.copy_popup_pane_edit_hwnds.borrow().clone(),
                restore_foreground_hwnd,
                restore_focus_hwnd,
                should_restore_focus,
            )
        };

        if !copy_popup_hwnd.is_null() {
            if let Ok(mut overlay) = overlay_ui_state().lock() {
                overlay.copy_popup = None;
            }
            unsafe {
                for edit_hwnd in &edit_hwnds {
                    if !edit_hwnd.is_null() {
                        ShowWindow(*edit_hwnd, SW_HIDE);
                    }
                }
                ShowWindow(copy_popup_hwnd, SW_HIDE);
            }
        }
        restore_copy_popup_focus_after_hide(
            should_restore_focus,
            restore_foreground_hwnd,
            restore_focus_hwnd,
        );
        Ok(())
    }

    fn should_offer_shortcut_help(hwnd: HWND) -> bool {
        unsafe { get_window_state(hwnd) }
            .ok()
            .and_then(|state| {
                state
                    .shared
                    .lock()
                    .ok()
                    .map(|shared| shared.shell_state.can_start_session())
            })
            .unwrap_or(false)
    }

    fn update_pending_hotkey_origin_insert_target(
        hwnd: HWND,
        candidate_context: Option<DesktopInsertTargetContext>,
    ) -> Result<()> {
        let state = unsafe { get_window_state(hwnd) }?;
        if let Ok(mut shared) = state.shared.lock() {
            if shared.shell_state.can_start_session() {
                shared.pending_hotkey_origin_insert_target = resolve_pending_hotkey_origin_capture(
                    shared.pending_hotkey_origin_insert_target.as_ref(),
                    candidate_context.as_ref(),
                );
            } else {
                shared.pending_hotkey_origin_insert_target = None;
            }
        }
        Ok(())
    }

    fn capture_pending_hotkey_origin_insert_target(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state(hwnd) }?;
        let hud_hwnd = state.hud_hwnd.get();
        let shared = Arc::clone(&state.shared);
        let context = capture_foreground_insert_target_context_snapshot(hwnd, hud_hwnd);
        update_pending_hotkey_origin_insert_target(hwnd, context)?;
        spawn_pending_hotkey_origin_enrichment(hwnd, hud_hwnd, shared);
        Ok(())
    }

    fn capture_pending_hotkey_origin_insert_target_from_hook(hwnd: HWND) {
        let _ = capture_pending_hotkey_origin_insert_target(hwnd);
    }

    /// Runs the expensive UI Automation focus capture off the hot path and
    /// merges the richer context into the pending hotkey origin snapshot.
    /// The merge keeps the press-time window snapshot semantics because
    /// `resolve_pending_hotkey_origin_capture` rejects candidates from
    /// another window and only upgrades weaker captures.
    fn spawn_pending_hotkey_origin_enrichment(
        hwnd: HWND,
        hud_hwnd: HWND,
        shared: Arc<Mutex<SharedState>>,
    ) {
        let hwnd_value = hwnd as usize;
        let hud_hwnd_value = hud_hwnd as usize;
        thread::spawn(move || {
            let candidate_context = capture_foreground_insert_target_context(
                hwnd_value as HWND,
                hud_hwnd_value as HWND,
            );
            if let Ok(mut shared) = shared.lock() {
                if shared.shell_state.can_start_session() {
                    shared.pending_hotkey_origin_insert_target =
                        resolve_pending_hotkey_origin_capture(
                            shared.pending_hotkey_origin_insert_target.as_ref(),
                            candidate_context.as_ref(),
                        );
                } else {
                    shared.pending_hotkey_origin_insert_target = None;
                }
            }
        });
    }

    fn spawn_hotkey_origin_enrichment(
        hwnd: HWND,
        hud_hwnd: HWND,
        generation: u64,
        shared: Arc<Mutex<SharedState>>,
    ) {
        let hwnd_value = hwnd as usize;
        let hud_hwnd_value = hud_hwnd as usize;
        thread::spawn(move || {
            for _ in 0..HOTKEY_ORIGIN_ENRICH_MAX_POLLS {
                thread::sleep(Duration::from_millis(HOTKEY_ORIGIN_ENRICH_POLL_INTERVAL_MS));

                let candidate_context = capture_foreground_insert_target_context(
                    hwnd_value as HWND,
                    hud_hwnd_value as HWND,
                );
                let should_stop = shared
                    .lock()
                    .ok()
                    .and_then(|mut shared| {
                        let active = shared.active_recording.as_mut()?;
                        if active.generation != generation {
                            return Some(true);
                        }

                        let enriched_origin = resolve_hotkey_recording_origin_enrichment(
                            active.origin_insert_target.as_ref(),
                            candidate_context.as_ref(),
                        );
                        if enriched_origin != active.origin_insert_target {
                            active.origin_insert_target = enriched_origin;
                            active.origin_insert_target_source =
                                Some(HOTKEY_ORIGIN_ENRICH_SOURCE.to_string());
                            unsafe {
                                let _ = PostMessageW(
                                    hwnd_value as HWND,
                                    STREAMING_CORRECTED_HUD_MESSAGE,
                                    generation as usize,
                                    0,
                                );
                            }
                            return Some(true);
                        }

                        Some(false)
                    })
                    .unwrap_or(true);

                if should_stop {
                    break;
                }
            }
        });
    }

    fn schedule_pending_shortcut_help(hwnd: HWND) -> Result<()> {
        let _ = capture_pending_hotkey_origin_insert_target(hwnd);
        if !should_offer_shortcut_help(hwnd) {
            return cancel_pending_shortcut_help(hwnd);
        }

        unsafe {
            KillTimer(hwnd, TIMER_SHORTCUT_HELP_HOLD);
            SetTimer(
                hwnd,
                TIMER_SHORTCUT_HELP_HOLD,
                SHORTCUT_HELP_HOLD_DELAY_MS,
                None,
            );
        }
        Ok(())
    }

    fn cancel_pending_shortcut_help(hwnd: HWND) -> Result<()> {
        unsafe {
            KillTimer(hwnd, TIMER_SHORTCUT_HELP_HOLD);
        }
        hide_shortcut_help(hwnd)
    }

    fn maybe_show_pending_shortcut_help(hwnd: HWND) -> Result<()> {
        unsafe {
            KillTimer(hwnd, TIMER_SHORTCUT_HELP_HOLD);
        }
        if !should_offer_shortcut_help(hwnd) {
            return hide_shortcut_help(hwnd);
        }

        let should_show = with_low_level_toggle_router(|router, owner_hwnd| {
            if owner_hwnd != hwnd {
                return false;
            }
            router.activate_pending_hold_help()
        })
        .unwrap_or(false);
        if !should_show {
            return Ok(());
        }

        let model = unsafe { get_window_state(hwnd) }.ok().and_then(|state| {
            state
                .shared
                .lock()
                .ok()
                .map(|shared| desktop_shortcut_help_model(&shared.desktop_actions))
        });
        if let Some(model) = model {
            show_shortcut_help(hwnd, model)?;
        }
        Ok(())
    }

    fn show_shortcut_help(hwnd: HWND, model: DesktopShortcutHelpModel) -> Result<()> {
        let shortcut_help_hwnd = unsafe { get_window_state(hwnd)? }.shortcut_help_hwnd.get();
        show_shortcut_help_window(shortcut_help_hwnd, model)
    }

    fn show_shortcut_help_window(
        shortcut_help_hwnd: HWND,
        model: DesktopShortcutHelpModel,
    ) -> Result<()> {
        if shortcut_help_hwnd.is_null() {
            anyhow::bail!("Talk desktop shortcut help window is unavailable");
        }

        let metrics = scale_shortcut_help_metrics_for_dpi(
            desktop_shortcut_help_metrics_for_entry_count(model.entries.len()),
            overlay_dpi_for_window(shortcut_help_hwnd),
        );
        let (screen_width, screen_height) = current_screen_size();
        let position = desktop_shortcut_help_position(
            screen_width,
            screen_height,
            metrics.width,
            metrics.height,
            metrics.bottom_margin,
        );

        if let Ok(mut overlay) = overlay_ui_state().lock() {
            overlay.shortcut_help = Some(model);
        }

        unsafe {
            SetWindowPos(
                shortcut_help_hwnd,
                (-1isize) as HWND,
                position.x,
                position.y,
                metrics.width,
                metrics.height,
                SWP_NOACTIVATE,
            );
            apply_rounded_window_region(
                shortcut_help_hwnd,
                metrics.width,
                metrics.height,
                scale_desktop_overlay_length(
                    SHORTCUT_HELP_CORNER_RADIUS,
                    overlay_dpi_for_window(shortcut_help_hwnd),
                ),
            );
            InvalidateRect(shortcut_help_hwnd, ptr::null(), 0);
            match desktop_shortcut_help_activation_policy() {
                DesktopOverlayActivationPolicy::NoActivate => {
                    ShowWindow(shortcut_help_hwnd, SW_SHOWNOACTIVATE);
                }
                DesktopOverlayActivationPolicy::ActivateOnInteract => {
                    ShowWindow(shortcut_help_hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
        Ok(())
    }

    fn hide_shortcut_help(hwnd: HWND) -> Result<()> {
        let shortcut_help_hwnd = unsafe { get_window_state(hwnd)? }.shortcut_help_hwnd.get();
        hide_shortcut_help_window(shortcut_help_hwnd)
    }

    fn hide_shortcut_help_window(shortcut_help_hwnd: HWND) -> Result<()> {
        if !shortcut_help_hwnd.is_null() {
            if let Ok(mut overlay) = overlay_ui_state().lock() {
                overlay.shortcut_help = None;
            }
            unsafe {
                ShowWindow(shortcut_help_hwnd, SW_HIDE);
            }
        }
        Ok(())
    }

    /// Arms the recording timeout on the UI thread with a window timer instead
    /// of a dedicated sleeping thread, so an early stop/cancel simply kills the
    /// timer instead of leaving a thread lingering until the deadline.
    fn arm_recording_timeout_timer(hwnd: HWND, generation: u64, max_recording_seconds: u64) {
        if max_recording_seconds == 0 {
            return;
        }
        if let Ok(state) = unsafe { get_window_state(hwnd) } {
            state.recording_timeout_generation.set(Some(generation));
        }
        let timeout_ms = max_recording_seconds.saturating_mul(1000).min(0x7FFF_FFFF) as u32;
        unsafe {
            SetTimer(hwnd, TIMER_RECORDING_TIMEOUT, timeout_ms, None);
        }
    }

    fn cancel_recording_timeout_timer(hwnd: HWND) {
        if let Ok(state) = unsafe { get_window_state(hwnd) } {
            state.recording_timeout_generation.set(None);
        }
        unsafe {
            KillTimer(hwnd, TIMER_RECORDING_TIMEOUT);
        }
    }

    fn handle_recording_timeout_timer(hwnd: HWND) {
        let generation = unsafe { get_window_state(hwnd) }
            .ok()
            .and_then(|state| state.recording_timeout_generation.take());
        unsafe {
            KillTimer(hwnd, TIMER_RECORDING_TIMEOUT);
        }
        if let Some(generation) = generation {
            request_stop_recording(hwnd, generation);
        }
    }

    fn spawn_preparation_release_watcher(hwnd: HWND, generation: u64, hotkey: HotkeySpec) {
        let hwnd_value = hwnd as usize;
        thread::spawn(move || {
            let hwnd = hwnd_value as HWND;
            loop {
                if !hotkey.is_pressed() {
                    unsafe {
                        let _ = PostMessageW(hwnd, STOP_MESSAGE, generation as usize, 0);
                    }
                    break;
                }
                thread::sleep(Duration::from_millis(40));
            }
        });
    }

    fn loword(value: usize) -> u16 {
        (value & 0xFFFF) as u16
    }

    fn runtime_phase_to_code(phase: RuntimePhase) -> u32 {
        match phase {
            RuntimePhase::TriggerArmed => 1,
            RuntimePhase::Recording => 2,
            RuntimePhase::Transcribing => 3,
            RuntimePhase::Processing => 4,
            RuntimePhase::Inserting => 5,
            RuntimePhase::Completed => 6,
            RuntimePhase::Failed => 7,
            RuntimePhase::Cancelled => 8,
        }
    }

    fn runtime_phase_from_code(code: u32) -> RuntimePhase {
        match code {
            1 => RuntimePhase::TriggerArmed,
            2 => RuntimePhase::Recording,
            3 => RuntimePhase::Transcribing,
            4 => RuntimePhase::Processing,
            5 => RuntimePhase::Inserting,
            6 => RuntimePhase::Completed,
            7 => RuntimePhase::Failed,
            _ => RuntimePhase::Cancelled,
        }
    }

    fn to_wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn current_screen_size() -> (i32, i32) {
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
    }

    fn point_from_lparam(lparam: LPARAM) -> POINT {
        POINT {
            x: (lparam as u32 & 0xFFFF) as i16 as i32,
            y: ((lparam as u32 >> 16) & 0xFFFF) as i16 as i32,
        }
    }

    fn point_in_rect(point: POINT, rect: RECT) -> bool {
        point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
    }

    fn rects_intersect(left: RECT, right: RECT) -> bool {
        left.left < right.right
            && left.right > right.left
            && left.top < right.bottom
            && left.bottom > right.top
    }

    fn track_copy_popup_mouse_leave(hwnd: HWND) {
        let mut event = TRACKMOUSEEVENT {
            cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        unsafe {
            let _ = TrackMouseEvent(&mut event);
        }
    }

    fn copy_popup_hovered_control_for_point(
        copy_popup_hwnd: HWND,
        dpi: u32,
        point: POINT,
    ) -> CopyPopupHoveredControl {
        if point_in_rect(point, copy_popup_copy_button_rect(copy_popup_hwnd, dpi)) {
            CopyPopupHoveredControl::Copy
        } else if point_in_rect(point, copy_popup_close_button_rect(copy_popup_hwnd, dpi)) {
            CopyPopupHoveredControl::Close
        } else {
            CopyPopupHoveredControl::None
        }
    }

    fn refresh_copy_popup_hover(hwnd: HWND, point: POINT) {
        let dpi = overlay_dpi_for_window(hwnd);
        let hovered = copy_popup_hovered_control_for_point(hwnd, dpi, point);
        let mut should_invalidate = false;
        if let Ok(mut overlay) = overlay_ui_state().lock() {
            if let Some(popup) = overlay.copy_popup.as_mut() {
                if popup.hovered_control != hovered {
                    popup.hovered_control = hovered;
                    should_invalidate = true;
                }
            }
        }
        unsafe {
            if should_invalidate {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
        }
    }

    fn clear_copy_popup_hover(hwnd: HWND) {
        let mut should_invalidate = false;
        if let Ok(mut overlay) = overlay_ui_state().lock() {
            if let Some(popup) = overlay.copy_popup.as_mut() {
                if popup.hovered_control != CopyPopupHoveredControl::None {
                    popup.hovered_control = CopyPopupHoveredControl::None;
                    should_invalidate = true;
                }
            }
        }
        if should_invalidate {
            unsafe {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
        }
    }

    fn set_copy_popup_pressed_control(hwnd: HWND, pressed: CopyPopupHoveredControl) {
        let mut should_invalidate = false;
        if let Ok(mut overlay) = overlay_ui_state().lock() {
            if let Some(popup) = overlay.copy_popup.as_mut() {
                if popup.pressed_control != pressed {
                    popup.pressed_control = pressed;
                    should_invalidate = true;
                }
            }
        }
        if should_invalidate {
            unsafe {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
        }
    }

    fn clear_copy_popup_pressed_control(hwnd: HWND) {
        set_copy_popup_pressed_control(hwnd, CopyPopupHoveredControl::None);
    }

    fn desktop_overlay_rect_to_rect(rect: talk_desktop::DesktopOverlayRect) -> RECT {
        RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }

    /// Grows `target` so it also covers `other`, used to build a single dirty
    /// region out of the HUD parts that actually changed.
    fn union_overlay_rect(target: &mut RECT, other: RECT) {
        target.left = target.left.min(other.left);
        target.top = target.top.min(other.top);
        target.right = target.right.max(other.right);
        target.bottom = target.bottom.max(other.bottom);
    }

    fn copy_popup_client_metrics(copy_popup_hwnd: HWND, dpi: u32) -> DesktopCopyPopupMetrics {
        let fallback = scale_copy_popup_metrics_for_dpi(desktop_copy_popup_metrics(), dpi);
        if copy_popup_hwnd.is_null() {
            return fallback;
        }

        let mut rect = RECT::default();
        unsafe {
            GetClientRect(copy_popup_hwnd, &mut rect);
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            fallback
        } else {
            DesktopCopyPopupMetrics {
                width,
                height,
                bottom_margin: fallback.bottom_margin,
            }
        }
    }

    fn copy_popup_copy_button_rect(copy_popup_hwnd: HWND, dpi: u32) -> RECT {
        let metrics = copy_popup_client_metrics(copy_popup_hwnd, dpi);
        desktop_overlay_rect_to_rect(popup_copy_button_layout_rect(
            metrics.width,
            metrics.height,
            dpi,
        ))
    }

    fn copy_popup_close_button_rect(copy_popup_hwnd: HWND, dpi: u32) -> RECT {
        let metrics = copy_popup_client_metrics(copy_popup_hwnd, dpi);
        desktop_overlay_rect_to_rect(popup_close_button_layout_rect(
            metrics.width,
            metrics.height,
            dpi,
        ))
    }

    fn copy_popup_editor_frame_rect(copy_popup_hwnd: HWND, dpi: u32) -> RECT {
        let metrics = copy_popup_client_metrics(copy_popup_hwnd, dpi);
        desktop_overlay_rect_to_rect(popup_editor_frame_layout_rect(
            metrics.width,
            metrics.height,
            dpi,
        ))
    }

    fn copy_popup_editor_content_rect_for_metrics(
        metrics: DesktopCopyPopupMetrics,
        dpi: u32,
        content_height: i32,
    ) -> RECT {
        desktop_overlay_rect_to_rect(desktop_copy_popup_editor_content_rect(
            metrics.width,
            metrics.height,
            dpi,
            content_height,
        ))
    }

    fn copy_popup_editor_content_rect_for_window(
        copy_popup_hwnd: HWND,
        dpi: u32,
        content_height: i32,
    ) -> RECT {
        copy_popup_editor_content_rect_for_metrics(
            copy_popup_client_metrics(copy_popup_hwnd, dpi),
            dpi,
            content_height,
        )
    }

    fn measure_copy_popup_wrapped_text_height(
        edit_hwnd: HWND,
        max_width: i32,
        text: &str,
        dpi: u32,
    ) -> i32 {
        let fallback_height = scale_desktop_overlay_length(24, dpi);
        let hdc = unsafe { GetDC(edit_hwnd) };
        if hdc.is_null() {
            return fallback_height;
        }

        let font = unsafe {
            let handle = SendMessageW(edit_hwnd, WM_GETFONT, 0, 0) as isize;
            if handle == 0 {
                GetStockObject(DEFAULT_GUI_FONT) as isize
            } else {
                handle
            }
        };
        let old_font = unsafe { SelectObject(hdc, font as _) };
        let sample_text = if text.trim().is_empty() { "Ag" } else { text };
        let mut measure_rect = RECT {
            left: 0,
            top: 0,
            right: max_width.max(1),
            bottom: 0,
        };
        let draw_flags = DT_CALCRECT | DT_CENTER | DT_EDITCONTROL | DT_NOPREFIX | DT_WORDBREAK;
        let wide = to_wide(sample_text);
        unsafe {
            DrawTextW(hdc, wide.as_ptr(), -1, &mut measure_rect, draw_flags);
            SelectObject(hdc, old_font);
            ReleaseDC(edit_hwnd, hdc);
        }

        (measure_rect.bottom - measure_rect.top).max(fallback_height)
    }

    fn update_copy_popup_edit_layout(owner_hwnd: HWND, copy_popup_hwnd: HWND) -> Result<()> {
        let (copy_popup_edit_hwnd, edit_hwnds) = {
            let state = unsafe { get_window_state(owner_hwnd)? };
            (
                state.copy_popup_edit_hwnd.get(),
                state.copy_popup_pane_edit_hwnds.borrow().clone(),
            )
        };
        if copy_popup_edit_hwnd.is_null() || edit_hwnds.is_empty() {
            return Ok(());
        }

        let panes = overlay_ui_state()
            .lock()
            .ok()
            .and_then(|overlay| overlay.copy_popup.as_ref().map(|popup| popup.model.clone()))
            .map(|model| copy_popup_visible_panes(&model))
            .unwrap_or_else(|| {
                vec![DesktopCopyPopupPaneModel {
                    label: String::new(),
                    text: window_text(copy_popup_edit_hwnd).unwrap_or_default(),
                    editable: true,
                    copy_default: true,
                }]
            });
        let visible_count = panes.len().min(edit_hwnds.len());
        if visible_count == 0 {
            return Ok(());
        }

        let dpi = overlay_dpi_for_window(copy_popup_hwnd);
        let metrics = copy_popup_client_metrics(copy_popup_hwnd, dpi);
        let probe_heights = vec![scale_desktop_overlay_length(40, dpi); visible_count];
        let probe_layouts =
            desktop_copy_popup_pane_layouts(metrics.width, metrics.height, dpi, &probe_heights);
        let content_heights = (0..visible_count)
            .map(|index| {
                let edit_hwnd = edit_hwnds[index];
                let text = window_text(edit_hwnd).unwrap_or_else(|| panes[index].text.clone());
                let probe_rect = if visible_count == 1 {
                    copy_popup_editor_content_rect_for_window(
                        copy_popup_hwnd,
                        dpi,
                        scale_desktop_overlay_length(24, dpi),
                    )
                } else {
                    desktop_overlay_rect_to_rect(probe_layouts[index].editor_rect)
                };
                measure_copy_popup_wrapped_text_height(
                    edit_hwnd,
                    probe_rect.right - probe_rect.left,
                    &text,
                    dpi,
                )
            })
            .collect::<Vec<_>>();
        let layouts =
            desktop_copy_popup_pane_layouts(metrics.width, metrics.height, dpi, &content_heights);

        unsafe {
            for (index, edit_hwnd) in edit_hwnds.iter().enumerate() {
                if index < visible_count {
                    let layout_rect = desktop_overlay_rect_to_rect(layouts[index].editor_rect);
                    SetWindowPos(
                        *edit_hwnd,
                        ptr::null_mut(),
                        layout_rect.left,
                        layout_rect.top,
                        layout_rect.right - layout_rect.left,
                        layout_rect.bottom - layout_rect.top,
                        0,
                    );
                    ShowWindow(*edit_hwnd, SW_SHOW);
                } else {
                    ShowWindow(*edit_hwnd, SW_HIDE);
                }
            }
        }
        Ok(())
    }

    unsafe fn apply_rounded_window_region(hwnd: HWND, width: i32, height: i32, radius: i32) {
        let region = if radius <= 0 {
            CreateRectRgn(0, 0, width + 1, height + 1)
        } else {
            CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius)
        };
        if !region.is_null() {
            let _ = SetWindowRgn(hwnd, region, 1);
        }
    }

    unsafe fn create_overlay_font(dpi: u32, point_size: i32, weight: i32) -> isize {
        let pixel_height = -scale_desktop_overlay_length(point_size, dpi);
        CreateFontW(
            pixel_height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS.into(),
            CLIP_DEFAULT_PRECIS.into(),
            CLEARTYPE_QUALITY.into(),
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            to_wide("Segoe UI").as_ptr(),
        ) as isize
    }

    #[derive(Default)]
    struct OverlayFontCache {
        dpi: u32,
        fonts: HashMap<(i32, i32), isize>,
    }

    impl OverlayFontCache {
        fn clear(&mut self) {
            for font in self.fonts.values() {
                unsafe {
                    DeleteObject(*font as _);
                }
            }
            self.fonts.clear();
        }
    }

    impl Drop for OverlayFontCache {
        fn drop(&mut self) {
            self.clear();
        }
    }

    thread_local! {
        static OVERLAY_FONT_CACHE: RefCell<OverlayFontCache> =
            RefCell::new(OverlayFontCache::default());
    }

    /// Returns a cached HFONT for the overlay windows. Cached fonts stay
    /// selected for the lifetime of the UI thread and must NOT be deleted by
    /// callers; the cache is invalidated (and the old fonts released) when the
    /// DPI changes.
    unsafe fn cached_overlay_font(dpi: u32, point_size: i32, weight: i32) -> isize {
        OVERLAY_FONT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.dpi != dpi {
                cache.clear();
                cache.dpi = dpi;
            }
            *cache
                .fonts
                .entry((point_size, weight))
                .or_insert_with(|| create_overlay_font(dpi, point_size, weight))
        })
    }

    #[derive(Default)]
    struct OverlayGdiObjectCache {
        brushes: HashMap<u32, isize>,
        pens: HashMap<(i32, u32), isize>,
    }

    impl OverlayGdiObjectCache {
        fn clear(&mut self) {
            for handle in self.brushes.values().chain(self.pens.values()) {
                unsafe {
                    DeleteObject(*handle as _);
                }
            }
            self.brushes.clear();
            self.pens.clear();
        }
    }

    impl Drop for OverlayGdiObjectCache {
        fn drop(&mut self) {
            self.clear();
        }
    }

    thread_local! {
        static OVERLAY_GDI_OBJECT_CACHE: RefCell<OverlayGdiObjectCache> =
            RefCell::new(OverlayGdiObjectCache::default());
    }

    /// Returns a cached solid HBRUSH for `color`. Overlay colors all come from
    /// the static design tokens, so the cache stays bounded to a handful of
    /// entries for the lifetime of the UI thread. Callers must NOT delete the
    /// returned handle; the cache releases it on thread teardown.
    unsafe fn cached_overlay_brush(color: u32) -> isize {
        OVERLAY_GDI_OBJECT_CACHE.with(|cache| {
            *cache
                .borrow_mut()
                .brushes
                .entry(color)
                .or_insert_with(|| CreateSolidBrush(color) as isize)
        })
    }

    /// Returns a cached solid HPEN for `width`/`color`, with the same ownership
    /// rules as [`cached_overlay_brush`].
    unsafe fn cached_overlay_pen(width: i32, color: u32) -> isize {
        OVERLAY_GDI_OBJECT_CACHE.with(|cache| {
            *cache
                .borrow_mut()
                .pens
                .entry((width, color))
                .or_insert_with(|| CreatePen(PS_SOLID, width, color) as isize)
        })
    }

    /// Strokes a one pixel border around `rect` in `color`.
    ///
    /// `FrameRect` replaces the `SelectObject(pen)` + `SelectObject(HOLLOW_BRUSH)`
    /// + `RoundRect(.., 0, 0)` + two restores that every square outline in the
    /// overlay used to pay for. Same pixels, one call, no device state to save.
    unsafe fn stroke_overlay_border(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        color: u32,
    ) {
        FrameRect(hdc, &rect, cached_overlay_brush(color) as _);
    }

    /// Strokes a `thickness` pixel border inside `rect`. Unlike a pen, which
    /// straddles the boundary, the whole stroke stays inside the rectangle, so a
    /// focus border never bleeds into the neighbouring control.
    unsafe fn stroke_overlay_border_thickness(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        thickness: i32,
        color: u32,
    ) {
        let thickness = thickness.max(1);
        if thickness == 1 {
            stroke_overlay_border(hdc, rect, color);
            return;
        }
        let brush = cached_overlay_brush(color) as _;
        let edges = [
            RECT {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.top + thickness,
            },
            RECT {
                left: rect.left,
                top: rect.bottom - thickness,
                right: rect.right,
                bottom: rect.bottom,
            },
            RECT {
                left: rect.left,
                top: rect.top,
                right: rect.left + thickness,
                bottom: rect.bottom,
            },
            RECT {
                left: rect.right - thickness,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
        ];
        for edge in edges {
            FillRect(hdc, &edge, brush);
        }
    }

    /// Identifies which overlay window a cached back buffer belongs to. Each
    /// window keeps its own DC/bitmap pair because they have different sizes.
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    enum OverlayBackBufferSlot {
        Hud,
        CopyPopup,
        ShortcutHelp,
    }

    struct OverlayBackBuffer {
        width: i32,
        height: i32,
        memory_dc: isize,
        memory_bitmap: isize,
        old_bitmap: isize,
    }

    impl Drop for OverlayBackBuffer {
        fn drop(&mut self) {
            unsafe {
                let _ = SelectObject(self.memory_dc as _, self.old_bitmap as _);
                DeleteObject(self.memory_bitmap as _);
                DeleteDC(self.memory_dc as _);
            }
        }
    }

    thread_local! {
        static OVERLAY_BACK_BUFFERS: RefCell<HashMap<OverlayBackBufferSlot, OverlayBackBuffer>> =
            RefCell::new(HashMap::new());
    }

    /// Returns a memory DC backing one overlay window, reusing the cached
    /// DC/bitmap pair across frames and recreating it only when that window's
    /// size changes. The cached GDI objects are released by
    /// `OverlayBackBuffer`'s `Drop` implementation on replacement or thread
    /// teardown.
    unsafe fn overlay_back_buffer_dc(
        window_hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        slot: OverlayBackBufferSlot,
        width: i32,
        height: i32,
    ) -> Option<windows_sys::Win32::Graphics::Gdi::HDC> {
        if width <= 0 || height <= 0 {
            return None;
        }
        OVERLAY_BACK_BUFFERS.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(buffer) = cache.get(&slot) {
                if buffer.width == width && buffer.height == height {
                    return Some(buffer.memory_dc as _);
                }
            }
            cache.remove(&slot);

            let memory_dc = CreateCompatibleDC(window_hdc);
            if memory_dc.is_null() {
                return None;
            }
            let memory_bitmap = CreateCompatibleBitmap(window_hdc, width, height);
            if memory_bitmap.is_null() {
                DeleteDC(memory_dc);
                return None;
            }
            let old_bitmap = SelectObject(memory_dc, memory_bitmap as _);
            if old_bitmap.is_null() {
                DeleteObject(memory_bitmap as _);
                DeleteDC(memory_dc);
                return None;
            }
            cache.insert(
                slot,
                OverlayBackBuffer {
                    width,
                    height,
                    memory_dc: memory_dc as isize,
                    memory_bitmap: memory_bitmap as isize,
                    old_bitmap: old_bitmap as isize,
                },
            );
            Some(memory_dc)
        })
    }

    /// Copies the freshly painted region of a back buffer onto the window DC.
    unsafe fn blit_overlay_back_buffer(
        window_hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        memory_dc: windows_sys::Win32::Graphics::Gdi::HDC,
        paint_rect: RECT,
    ) {
        let paint_width = (paint_rect.right - paint_rect.left).max(0);
        let paint_height = (paint_rect.bottom - paint_rect.top).max(0);
        if paint_width == 0 || paint_height == 0 {
            return;
        }
        BitBlt(
            window_hdc,
            paint_rect.left,
            paint_rect.top,
            paint_width,
            paint_height,
            memory_dc,
            paint_rect.left,
            paint_rect.top,
            SRCCOPY,
        );
    }

    /// Measures text that has already been converted to UTF-16, so callers that
    /// also draw it convert once instead of once per measurement and once per
    /// `TextOutW`. `wide` is expected to be NUL-terminated like `to_wide`
    /// returns.
    unsafe fn measure_overlay_wide_text_size(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        wide: &[u16],
    ) -> SIZE {
        let mut size = SIZE { cx: 0, cy: 0 };
        let text_len = wide.len().saturating_sub(1) as i32;
        if text_len > 0 {
            let _ = GetTextExtentPoint32W(hdc, wide.as_ptr(), text_len, &mut size);
        }
        size
    }

    fn encode_overlay_glyph_utf16(glyph: char) -> ([u16; 2], usize) {
        let mut buffer = [0u16; 2];
        let length = glyph.encode_utf16(&mut buffer).len();
        (buffer, length)
    }

    unsafe fn measure_overlay_glyph_size(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        glyph: char,
    ) -> SIZE {
        let (wide, length) = encode_overlay_glyph_utf16(glyph);
        let mut size = SIZE { cx: 0, cy: 0 };
        let _ = GetTextExtentPoint32W(hdc, wide.as_ptr(), length as i32, &mut size);
        size
    }

    #[derive(Default)]
    struct OverlayGlyphMetricCache {
        dpi: u32,
        glyphs: HashMap<char, (i32, i32)>,
    }

    thread_local! {
        static OVERLAY_GLYPH_METRIC_CACHE: RefCell<OverlayGlyphMetricCache> =
            RefCell::new(OverlayGlyphMetricCache::default());
    }

    /// Measures `glyph` once per DPI and reuses the result afterwards. The
    /// thinking HUD re-lays out its title on every animation frame, and
    /// `GetTextExtentPoint32W` was the dominant cost of that pass. Callers must
    /// have the overlay title font selected into `hdc`, which is the only font
    /// the wave text is drawn with.
    unsafe fn cached_overlay_glyph_metrics(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        dpi: u32,
        glyph: char,
    ) -> (i32, i32) {
        OVERLAY_GLYPH_METRIC_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.dpi != dpi {
                cache.glyphs.clear();
                cache.dpi = dpi;
            }
            if let Some(metrics) = cache.glyphs.get(&glyph) {
                return *metrics;
            }
            let size = measure_overlay_glyph_size(hdc, glyph);
            let metrics = (size.cx, size.cy);
            cache.glyphs.insert(glyph, metrics);
            metrics
        })
    }

    unsafe fn draw_thinking_wave_text(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        text: &str,
        rect: RECT,
        dpi: u32,
        pulse_tick: u32,
    ) {
        let palette = desktop_hud_thinking_palette();
        let glyph_count = text.chars().count();
        if glyph_count == 0 {
            return;
        }

        let wave_offsets = desktop_hud_thinking_text_wave_offsets(glyph_count, pulse_tick);
        let letter_spacing = scale_desktop_overlay_length(1, dpi);
        let min_width = scale_desktop_overlay_length(6, dpi);
        let min_height = scale_desktop_overlay_length(14, dpi);
        let mut glyph_widths = Vec::with_capacity(glyph_count);
        let mut total_width = 0i32;
        let mut max_height = 0i32;

        for glyph in text.chars() {
            let (glyph_width, glyph_height) = cached_overlay_glyph_metrics(hdc, dpi, glyph);
            let width = glyph_width.max(min_width);
            glyph_widths.push(width);
            total_width += width;
            max_height = max_height.max(glyph_height.max(min_height));
        }

        total_width += letter_spacing * (glyph_count.saturating_sub(1) as i32);
        let mut current_x = rect.left + ((rect.right - rect.left - total_width).max(0) / 2);
        let base_y = rect.top + ((rect.bottom - rect.top - max_height).max(0) / 2);

        // One text pass, not two: the drop shadow doubled the TextOutW calls per
        // frame for a 1px offset nobody could read at this font size.
        SetTextColor(hdc, rgb_triplet(palette.text_rgb));
        for ((glyph, width), wave_offset_px) in
            text.chars().zip(glyph_widths.iter()).zip(wave_offsets)
        {
            let wave_direction = if wave_offset_px.is_negative() { -1 } else { 1 };
            let wave_magnitude = scale_desktop_overlay_length(i32::from(wave_offset_px.abs()), dpi);
            let y = base_y + (wave_direction * wave_magnitude);
            let (wide, text_len) = encode_overlay_glyph_utf16(glyph);
            let _ = TextOutW(hdc, current_x, y, wide.as_ptr(), text_len as i32);
            current_x += *width + letter_spacing;
        }
    }

    unsafe fn draw_listening_hud_transcript(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        dpi: u32,
        text_rect: RECT,
        layout: &DesktopListeningHudPartialTextLayout,
        corrected_prefix: &str,
        partial_tail: &str,
        scroll_line_offset: usize,
    ) {
        let visible_lines = desktop_listening_hud_visible_lines(
            corrected_prefix,
            partial_tail,
            layout,
            scroll_line_offset,
        );
        if visible_lines.is_empty() {
            return;
        }

        let transcript_font = cached_overlay_font(dpi, 9, FW_BOLD as i32);
        let old_transcript_font = SelectObject(hdc, transcript_font as _);
        let line_height = scale_desktop_overlay_length(17, dpi).max(12);
        let unit_width = desktop_listening_hud_text_unit_width(dpi);
        let mut line_top = text_rect.top;

        for line in visible_lines {
            // Convert each run to UTF-16 once and keep the buffer: the same
            // bytes were previously encoded twice per run per frame, once to
            // measure the run and once to draw it.
            let measured_runs = line
                .runs
                .into_iter()
                .map(|run| {
                    let wide = to_wide(&run.text);
                    let measured_width = measure_overlay_wide_text_size(hdc, &wide).cx;
                    let width = if measured_width > 0 {
                        measured_width
                    } else {
                        (display_text_units(&run.text) as i32 * unit_width).max(1)
                    };
                    (run.lifecycle, wide, width)
                })
                .collect::<Vec<_>>();
            let line_width = measured_runs
                .iter()
                .map(|(_, _, width)| *width)
                .sum::<i32>()
                .max(1);
            let mut current_x = desktop_listening_hud_line_origin(
                DesktopOverlayRect {
                    left: text_rect.left,
                    top: text_rect.top,
                    right: text_rect.right,
                    bottom: text_rect.bottom,
                },
                line_width,
            );

            for (lifecycle, wide, run_width) in measured_runs {
                SetTextColor(hdc, text_lifecycle_color(lifecycle));
                let text_len = wide.len().saturating_sub(1) as i32;
                let _ = TextOutW(hdc, current_x, line_top, wide.as_ptr(), text_len);
                current_x += run_width;
            }

            line_top += line_height;
        }

        let _ = SelectObject(hdc, old_transcript_font);
    }

    fn desktop_hud_view_model_for_text(text: &str) -> DesktopHudViewModel {
        if text == hud_message_for_phase(RuntimePhase::Recording) {
            return desktop_hud_view_model_for_phase(RuntimePhase::Recording);
        }
        if text == hud_message_for_phase(RuntimePhase::Transcribing) {
            return desktop_hud_view_model_for_phase(RuntimePhase::Transcribing);
        }
        if text == hud_message_for_phase(RuntimePhase::Processing) {
            return desktop_hud_view_model_for_phase(RuntimePhase::Processing);
        }
        if text == hud_message_for_phase(RuntimePhase::Inserting) {
            return desktop_hud_view_model_for_phase(RuntimePhase::Inserting);
        }
        if text == hud_message_for_phase(RuntimePhase::Completed) {
            return desktop_hud_view_model_for_phase(RuntimePhase::Completed);
        }
        if text == hud_message_for_phase(RuntimePhase::Failed) {
            return desktop_hud_view_model_for_phase(RuntimePhase::Failed);
        }
        if text == hud_message_for_phase(RuntimePhase::Cancelled) {
            return desktop_hud_view_model_for_phase(RuntimePhase::Cancelled);
        }

        let mut parts = text.splitn(2, '\n');
        let summary = parts.next().unwrap_or("Talk").trim();
        let title = summary
            .strip_prefix("Talk: ")
            .unwrap_or(summary)
            .to_string();
        let detail = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let visual_state = if title.eq_ignore_ascii_case("failed") {
            DesktopHudVisualState::Error
        } else if title.eq_ignore_ascii_case("cancelled") {
            DesktopHudVisualState::Cancelled
        } else if title.eq_ignore_ascii_case("done") || title.eq_ignore_ascii_case("copied") {
            DesktopHudVisualState::Success
        } else {
            DesktopHudVisualState::Informational
        };
        DesktopHudViewModel {
            visual_state,
            title,
            detail,
            detail_lifecycle: None,
            meter: None,
            progress_percent: None,
        }
    }

    fn copy_popup_owner(copy_popup_hwnd: HWND) -> Option<HWND> {
        if copy_popup_hwnd.is_null() {
            return None;
        }

        let owner = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindow(
                copy_popup_hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::GW_OWNER,
            )
        };
        (!owner.is_null()).then_some(owner)
    }

    fn activate_copy_popup_for_interaction(owner_hwnd: HWND) -> Result<()> {
        let foreground_hwnd = unsafe { GetForegroundWindow() };
        let (copy_popup_hwnd, copy_popup_edit_hwnd, hud_hwnd, needs_focus_capture) = {
            let state = unsafe { get_window_state(owner_hwnd)? };
            (
                state.copy_popup_hwnd.get(),
                state.copy_popup_edit_hwnd.get(),
                state.hud_hwnd.get(),
                state.copy_popup_restore_foreground_hwnd.get().is_null(),
            )
        };
        if copy_popup_hwnd.is_null() || copy_popup_edit_hwnd.is_null() {
            anyhow::bail!("Talk desktop copy popup is unavailable");
        }

        let can_capture_foreground = needs_focus_capture
            && !foreground_hwnd.is_null()
            && foreground_hwnd != copy_popup_hwnd
            && foreground_hwnd != owner_hwnd
            && foreground_hwnd != hud_hwnd
            && unsafe { IsWindow(foreground_hwnd) != 0 };
        if can_capture_foreground {
            let focus_target =
                capture_foreground_focus_target(foreground_hwnd, owner_hwnd, hud_hwnd)
                    .focus_hwnd
                    .unwrap_or(foreground_hwnd);
            let state = unsafe { get_window_state(owner_hwnd)? };
            if state.copy_popup_restore_foreground_hwnd.get().is_null() {
                state
                    .copy_popup_restore_foreground_hwnd
                    .set(foreground_hwnd);
                state.copy_popup_restore_focus_hwnd.set(focus_target);
            }
        }

        unsafe {
            clear_copy_popup_keyboard_focus(copy_popup_hwnd);
            let _ = SetForegroundWindow(copy_popup_hwnd);
            let _ = SetFocus(copy_popup_edit_hwnd);
        }
        Ok(())
    }

    fn restore_copy_popup_focus_after_hide(
        should_restore_focus: bool,
        foreground_hwnd: HWND,
        focus_hwnd: HWND,
    ) {
        if !should_restore_focus || foreground_hwnd.is_null() {
            return;
        }

        unsafe {
            if IsWindow(foreground_hwnd) == 0 {
                return;
            }
            let _ = SetForegroundWindow(foreground_hwnd);
        }
        let focus_hwnd = if focus_hwnd.is_null() {
            foreground_hwnd
        } else {
            focus_hwnd
        };
        restore_foreground_focus_target(foreground_hwnd, focus_hwnd);
    }

    fn window_text(hwnd: HWND) -> Option<String> {
        if hwnd.is_null() {
            return None;
        }

        let text_len = unsafe { GetWindowTextLengthW(hwnd) };
        let mut buffer = vec![0u16; text_len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        (copied >= 0).then(|| String::from_utf16_lossy(&buffer[..copied as usize]))
    }

    fn copy_popup_current_text(owner_hwnd: HWND) -> Result<String> {
        let state = unsafe { get_window_state(owner_hwnd)? };
        if !state.copy_popup_edit_hwnd.get().is_null() {
            if let Some(text) = window_text(state.copy_popup_edit_hwnd.get()) {
                return Ok(text);
            }
        }

        overlay_ui_state()
            .lock()
            .ok()
            .and_then(|overlay| {
                overlay
                    .copy_popup
                    .as_ref()
                    .map(|popup| popup.model.editable_text.clone())
            })
            .context("Talk copy popup text is unavailable")
    }

    fn copy_popup_text_to_clipboard(owner_hwnd: HWND) -> Result<()> {
        let raw_text = copy_popup_current_text(owner_hwnd)?;
        let clipboard = WindowsClipboardBackend;
        clipboard
            .write_text(&raw_text)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let _ = activate_copy_popup_for_interaction(owner_hwnd);
        if desktop_copy_popup_copy_shows_follow_up_hud() {
            show_hud_text(
                owner_hwnd,
                &compose_hud_message("Talk: copied", Some("Copied latest transcript")),
                Some(1200),
            )?;
        }
        Ok(())
    }

    /// Draws the overlay panel: a single hairline border and a short accent bar
    /// that carries the state color. The caller owns the background fill, so the
    /// same full-window blit is not paid twice. Decorative grid lines, corner
    /// brackets and the second accent rail were removed so the HUD reads as one
    /// surface and every animated frame costs a bounded number of GDI calls.
    unsafe fn draw_terminal_shell_chrome(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        accent_color: u32,
        dpi: u32,
        corner_radius: i32,
    ) {
        if corner_radius > 0 {
            let border_pen = cached_overlay_pen(1, terminal_border_color());
            let old_pen = SelectObject(hdc, border_pen as _);
            let old_brush = SelectObject(hdc, GetStockObject(HOLLOW_BRUSH) as _);
            RoundRect(
                hdc,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                corner_radius,
                corner_radius,
            );
            let _ = SelectObject(hdc, old_brush);
            let _ = SelectObject(hdc, old_pen);
        } else {
            stroke_overlay_border(hdc, rect, terminal_border_color());
        }

        let rail_height = scale_desktop_overlay_length(3, dpi).max(2);
        let rail_left = rect.left + scale_desktop_overlay_length(14, dpi);
        let rail_top = rect.top + scale_desktop_overlay_length(11, dpi);
        let rail_width = ((rect.right - rect.left) / 4)
            .min(scale_desktop_overlay_length(56, dpi))
            .max(28);
        let rail_rect = RECT {
            left: rail_left,
            top: rail_top,
            right: rail_left + rail_width,
            bottom: rail_top + rail_height,
        };
        FillRect(hdc, &rail_rect, cached_overlay_brush(accent_color) as _);
    }

    unsafe fn draw_terminal_pill(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        _dpi: u32,
        fill_color: u32,
        border_color: u32,
        text_color: u32,
        font: isize,
        text: &str,
    ) {
        let brush = cached_overlay_brush(fill_color);
        FillRect(hdc, &rect, brush as _);
        stroke_overlay_border(hdc, rect, border_color);
        let old_font = SelectObject(hdc, font as _);
        SetTextColor(hdc, text_color);
        let mut text_rect = rect;
        DrawTextW(
            hdc,
            to_wide(text).as_ptr(),
            -1,
            &mut text_rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        let _ = SelectObject(hdc, old_font);
    }

    unsafe fn draw_terminal_thinking_indicator(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        dpi: u32,
        accent_color: u32,
    ) {
        let shell_brush = cached_overlay_brush(terminal_panel_fill_color());
        FillRect(hdc, &rect, shell_brush as _);
        stroke_overlay_border(hdc, rect, terminal_border_color());

        let old_brush = SelectObject(hdc, cached_overlay_brush(accent_color) as _);
        let old_pen = SelectObject(hdc, cached_overlay_pen(1, terminal_border_color()) as _);
        let dot_size = scale_desktop_overlay_length(8, dpi).max(6);
        let dot_gap = scale_desktop_overlay_length(8, dpi).max(6);
        let total_width = (dot_size * 3) + (dot_gap * 2);
        let start_x = rect.left + ((rect.right - rect.left - total_width).max(0) / 2);
        let top = rect.top + ((rect.bottom - rect.top - dot_size).max(0) / 2);
        for index in 0..3 {
            let x = start_x + (index * (dot_size + dot_gap));
            Ellipse(hdc, x, top, x + dot_size, top + dot_size);
        }

        let _ = SelectObject(hdc, old_pen);
        let _ = SelectObject(hdc, old_brush);
    }

    unsafe fn paint_hud_window(hwnd: HWND) {
        let mut paint = PAINTSTRUCT::default();
        let window_hdc = BeginPaint(hwnd, &mut paint);
        if window_hdc.is_null() {
            return;
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect);
        let dpi = overlay_dpi_for_window(hwnd);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let (
            model,
            thinking_pulse_tick,
            corrected_prefix,
            partial_tail,
            scroll_line_offset,
            cached_partial_layout,
            paint_text_region,
        ) = overlay_ui_state()
            .lock()
            .ok()
            .map(|overlay| {
                let cached_partial_layout = overlay.hud_streaming_partial_layout;
                let paint_text_region = cached_partial_layout
                    .map(|layout| {
                        rects_intersect(
                            paint.rcPaint,
                            desktop_overlay_rect_to_rect(layout.text_rect),
                        )
                    })
                    .unwrap_or(true);
                let mut model = overlay
                    .hud_model
                    .clone()
                    .unwrap_or_else(|| desktop_hud_view_model_for_phase(RuntimePhase::Recording));
                if model.visual_state == DesktopHudVisualState::Listening && !paint_text_region {
                    // Drop the detail in place instead of rebuilding the model:
                    // the rebuild cloned the title and the meter a second time on
                    // every frame the transcript region was skipped.
                    model.detail = None;
                }
                (
                    model,
                    overlay.hud_thinking_pulse_tick,
                    if paint_text_region {
                        overlay
                            .hud_streaming_corrected_prefix
                            .clone()
                            .unwrap_or_default()
                    } else {
                        String::new()
                    },
                    if paint_text_region {
                        overlay
                            .hud_streaming_partial_tail
                            .clone()
                            .unwrap_or_default()
                    } else {
                        String::new()
                    },
                    overlay.hud_streaming_scroll_line_offset,
                    cached_partial_layout,
                    paint_text_region,
                )
            })
            .unwrap_or_else(|| {
                (
                    desktop_hud_view_model_for_phase(RuntimePhase::Recording),
                    0,
                    String::new(),
                    String::new(),
                    0,
                    None,
                    true,
                )
            });
        let buffer = overlay_back_buffer_dc(window_hdc, OverlayBackBufferSlot::Hud, width, height);
        let hdc = buffer.unwrap_or(window_hdc);
        // One background fill per frame, whichever surface we ended up with. The
        // shell chrome below used to repeat it, so a full-window blit was paid
        // twice on every animated frame.
        let background_color = if model.visual_state == DesktopHudVisualState::Listening {
            listening_shell_color()
        } else {
            terminal_panel_fill_color()
        };
        FillRect(hdc, &rect, cached_overlay_brush(background_color) as _);
        let title_font = cached_overlay_font(dpi, 15, FW_BOLD as i32);
        let badge_font = cached_overlay_font(dpi, 9, FW_BOLD as i32);
        let icon_font = cached_overlay_font(dpi, 13, FW_BOLD as i32);
        let accent_color = accent_fill_color(model.visual_state);
        let old_font = SelectObject(hdc, title_font as _);
        SetBkMode(hdc, TRANSPARENT as i32);

        if model.visual_state == DesktopHudVisualState::Listening {
            // Background already laid down above; the shell only needs its
            // hairline border here.
            stroke_overlay_border(hdc, rect, listening_shell_border_color());
            let cancel_rect = desktop_listening_hud_cancel_button_rect(width, height, dpi);
            let confirm_rect = desktop_listening_hud_complete_button_rect(width, height, dpi);
            let mut waveform_rect = cached_partial_layout
                .map(|layout| layout.waveform_rect)
                .unwrap_or_else(|| desktop_listening_hud_waveform_rect(width, height, dpi));

            let cancel_brush = cached_overlay_brush(listening_cancel_fill_color());
            let cancel_pen = cached_overlay_pen(1, listening_cancel_border_color());
            let old_brush = SelectObject(hdc, cancel_brush as _);
            let old_pen = SelectObject(hdc, cancel_pen as _);
            Ellipse(
                hdc,
                cancel_rect.left,
                cancel_rect.top,
                cancel_rect.right,
                cancel_rect.bottom,
            );
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);

            let confirm_brush = cached_overlay_brush(listening_confirm_fill_color());
            let confirm_pen = cached_overlay_pen(1, listening_confirm_border_color());
            let old_brush = SelectObject(hdc, confirm_brush as _);
            let old_pen = SelectObject(hdc, confirm_pen as _);
            Ellipse(
                hdc,
                confirm_rect.left,
                confirm_rect.top,
                confirm_rect.right,
                confirm_rect.bottom,
            );
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);

            SelectObject(hdc, icon_font as _);
            SetTextColor(hdc, listening_cancel_glyph_color());
            let mut cancel_text_rect = RECT {
                left: cancel_rect.left,
                top: cancel_rect.top - scale_desktop_overlay_length(1, dpi),
                right: cancel_rect.right,
                bottom: cancel_rect.bottom,
            };
            DrawTextW(
                hdc,
                to_wide("×").as_ptr(),
                -1,
                &mut cancel_text_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            SetTextColor(hdc, listening_confirm_glyph_color());
            let mut confirm_text_rect = RECT {
                left: confirm_rect.left,
                top: confirm_rect.top - scale_desktop_overlay_length(1, dpi),
                right: confirm_rect.right,
                bottom: confirm_rect.bottom,
            };
            DrawTextW(
                hdc,
                to_wide("✓").as_ptr(),
                -1,
                &mut confirm_text_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            let use_fallback_partial =
                corrected_prefix.trim().is_empty() && partial_tail.trim().is_empty();
            let partial_text = if use_fallback_partial {
                model
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else {
                format!("{}{}", corrected_prefix, partial_tail)
            };
            if paint_text_region && !partial_text.trim().is_empty() {
                if let Some(partial_layout) = cached_partial_layout.or_else(|| {
                    desktop_listening_hud_partial_text_layout(
                        width,
                        height,
                        dpi,
                        Some(&partial_text),
                    )
                }) {
                    draw_listening_hud_transcript(
                        hdc,
                        dpi,
                        desktop_overlay_rect_to_rect(partial_layout.text_rect),
                        &partial_layout,
                        &corrected_prefix,
                        if use_fallback_partial {
                            partial_text.as_str()
                        } else {
                            partial_tail.as_str()
                        },
                        scroll_line_offset,
                    );
                    if let Some(scrollbar_rect) = partial_layout.scrollbar_rect {
                        // Plain fills: the track and thumb are square, so the
                        // RoundRect pass only added an outline stroked with
                        // whatever pen happened to be selected.
                        FillRect(
                            hdc,
                            &RECT {
                                left: scrollbar_rect.left,
                                top: scrollbar_rect.top,
                                right: scrollbar_rect.right,
                                bottom: scrollbar_rect.bottom,
                            },
                            cached_overlay_brush(listening_shell_border_color()) as _,
                        );

                        if let Some(thumb_rect) = desktop_listening_hud_scrollbar_thumb_rect(
                            &partial_layout,
                            dpi,
                            scroll_line_offset,
                        ) {
                            FillRect(
                                hdc,
                                &RECT {
                                    left: thumb_rect.left,
                                    top: thumb_rect.top,
                                    right: thumb_rect.right,
                                    bottom: thumb_rect.bottom,
                                },
                                cached_overlay_brush(terminal_text_soft_color()) as _,
                            );
                        }
                    }
                    waveform_rect = partial_layout.waveform_rect;
                }
            }

            if let Some(meter) = model.meter.as_ref() {
                let bar_brush = cached_overlay_brush(listening_waveform_color());
                let bar_width = scale_desktop_overlay_length(4, dpi).max(2);
                let bar_spacing = scale_desktop_overlay_length(3, dpi).max(2);
                let total_width = (meter.bar_heights.len() as i32 * bar_width)
                    + ((meter.bar_heights.len() as i32 - 1) * bar_spacing);
                let start_x = waveform_rect.left
                    + ((waveform_rect.right - waveform_rect.left - total_width).max(0) / 2);
                let center_y = (waveform_rect.top + waveform_rect.bottom) / 2;
                for (index, bar_height) in meter.bar_heights.iter().enumerate() {
                    let x = start_x + (index as i32 * (bar_width + bar_spacing));
                    let scaled_height = scale_desktop_overlay_length(*bar_height, dpi).max(4);
                    let top = center_y - (scaled_height / 2);
                    // FillRect instead of a pen-outlined RoundRect: the radius
                    // was zero and the outline used the fill color, so each bar
                    // paid for a pen it could not show.
                    let bar_rect = RECT {
                        left: x,
                        top,
                        right: x + bar_width,
                        bottom: top + scaled_height,
                    };
                    FillRect(hdc, &bar_rect, bar_brush as _);
                }
            }
        } else if model.visual_state == DesktopHudVisualState::Thinking {
            let palette = desktop_hud_thinking_palette();
            let thinking_progress =
                desktop_hud_thinking_progress_model(model.progress_percent, thinking_pulse_tick);
            let progress_track_rect = RECT {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            };
            // Flat fills instead of three per-frame GradientFill passes: the
            // thinking HUD repaints ~14 times a second, and the gradient was
            // only a few RGB steps wide at this size.
            FillRect(
                hdc,
                &progress_track_rect,
                cached_overlay_brush(rgb_triplet(palette.track_end_rgb)) as _,
            );

            let track_width = progress_track_rect.right - progress_track_rect.left;
            let fill_width = ((track_width as i64 * i64::from(thinking_progress.fill_percent))
                / 100)
                .clamp(0, i64::from(track_width)) as i32;
            if fill_width > 0 {
                let fill_rect = RECT {
                    left: progress_track_rect.left,
                    top: progress_track_rect.top,
                    right: progress_track_rect.left + fill_width,
                    bottom: progress_track_rect.bottom,
                };
                FillRect(
                    hdc,
                    &fill_rect,
                    cached_overlay_brush(rgb_triplet(palette.fill_end_rgb)) as _,
                );

                let head_width = scale_desktop_overlay_length(8, dpi).max(4).min(fill_width);
                let head_rect = RECT {
                    left: fill_rect.right - head_width,
                    top: fill_rect.top,
                    right: fill_rect.right,
                    bottom: fill_rect.bottom,
                };
                FillRect(
                    hdc,
                    &head_rect,
                    cached_overlay_brush(rgb_triplet(palette.fill_head_rgb)) as _,
                );
            }

            stroke_overlay_border(hdc, progress_track_rect, rgb_triplet(palette.border_rgb));

            if fill_width > 0 {
                let edge_width = scale_desktop_overlay_length(1, dpi).max(1).min(fill_width);
                let edge_pen = cached_overlay_pen(edge_width, rgb_triplet(palette.fill_head_rgb));
                let old_pen = SelectObject(hdc, edge_pen as _);
                let edge_x = progress_track_rect.left + fill_width - (edge_width / 2);
                let _ = MoveToEx(hdc, edge_x, progress_track_rect.top, ptr::null_mut());
                let _ = LineTo(hdc, edge_x, progress_track_rect.bottom);
                let _ = SelectObject(hdc, old_pen);
            }

            draw_thinking_wave_text(
                hdc,
                &model.title,
                progress_track_rect,
                dpi,
                thinking_pulse_tick,
            );
        } else {
            draw_terminal_shell_chrome(hdc, rect, accent_color, dpi, 0);
            let badge_right = width - scale_desktop_overlay_length(14, dpi);
            let badge_left = badge_right - scale_desktop_overlay_length(48, dpi);
            let badge_label = match model.visual_state {
                DesktopHudVisualState::Thinking => "RUN",
                DesktopHudVisualState::Success => "OK",
                DesktopHudVisualState::Error => "FAIL",
                DesktopHudVisualState::Cancelled => "OFF",
                DesktopHudVisualState::Informational => "ON",
                DesktopHudVisualState::Listening => "REC",
            };
            draw_terminal_pill(
                hdc,
                RECT {
                    left: badge_left,
                    top: scale_desktop_overlay_length(10, dpi),
                    right: badge_right,
                    bottom: scale_desktop_overlay_length(26, dpi),
                },
                dpi,
                accent_color,
                accent_outline_color(model.visual_state),
                accent_badge_text_color(model.visual_state),
                badge_font,
                badge_label,
            );

            if model.visual_state == DesktopHudVisualState::Thinking {
                draw_terminal_thinking_indicator(
                    hdc,
                    RECT {
                        left: badge_left,
                        top: scale_desktop_overlay_length(30, dpi),
                        right: badge_right,
                        bottom: height - scale_desktop_overlay_length(10, dpi),
                    },
                    dpi,
                    accent_color,
                );
            }

            SelectObject(hdc, title_font as _);
            SetTextColor(hdc, terminal_text_color());
            DrawTextW(
                hdc,
                to_wide(&model.title).as_ptr(),
                -1,
                &mut RECT {
                    left: scale_desktop_overlay_length(14, dpi),
                    top: scale_desktop_overlay_length(16, dpi),
                    right: badge_left - scale_desktop_overlay_length(10, dpi),
                    bottom: height - scale_desktop_overlay_length(12, dpi),
                },
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );

            if let Some(detail) = model
                .detail
                .as_deref()
                .filter(|detail| !detail.trim().is_empty())
            {
                let detail_font = cached_overlay_font(dpi, 10, 400);
                let old_detail_font = SelectObject(hdc, detail_font as _);
                let detail_color = desktop_hud_detail_lifecycle(&model)
                    .map(text_lifecycle_color)
                    .unwrap_or_else(terminal_text_soft_color);
                SetTextColor(hdc, detail_color);
                DrawTextW(
                    hdc,
                    to_wide(detail).as_ptr(),
                    -1,
                    &mut RECT {
                        left: scale_desktop_overlay_length(14, dpi),
                        top: scale_desktop_overlay_length(42, dpi),
                        right: badge_left - scale_desktop_overlay_length(10, dpi),
                        bottom: height - scale_desktop_overlay_length(10, dpi),
                    },
                    DT_LEFT | DT_WORDBREAK | DT_EDITCONTROL | DT_NOPREFIX,
                );
                let _ = SelectObject(hdc, old_detail_font);
            }
        }

        SelectObject(hdc, old_font);
        if let Some(memory_dc) = buffer {
            blit_overlay_back_buffer(window_hdc, memory_dc, paint.rcPaint);
        }
        EndPaint(hwnd, &paint);
    }

    unsafe fn paint_copy_popup_window(hwnd: HWND) {
        let mut paint = PAINTSTRUCT::default();
        let window_hdc = BeginPaint(hwnd, &mut paint);
        if window_hdc.is_null() {
            return;
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect);
        let popup = overlay_ui_state()
            .lock()
            .ok()
            .and_then(|overlay| overlay.copy_popup.clone());
        let dpi = overlay_dpi_for_window(hwnd);
        // Paint into a cached memory DC: the popup redraws whenever the pointer
        // moves between its two buttons, and painting straight to the window DC
        // made every hover flash the shell.
        let buffer = overlay_back_buffer_dc(
            window_hdc,
            OverlayBackBufferSlot::CopyPopup,
            rect.right - rect.left,
            rect.bottom - rect.top,
        );
        let hdc = buffer.unwrap_or(window_hdc);
        FillRect(
            hdc,
            &rect,
            cached_overlay_brush(typeless_popup_fill_color()) as _,
        );
        let title_font = cached_overlay_font(dpi, 15, FW_BOLD as i32);
        let label_font = cached_overlay_font(dpi, 10, FW_BOLD as i32);
        let button_font = cached_overlay_font(dpi, 12, FW_BOLD as i32);
        let copy_button_rect = copy_popup_copy_button_rect(hwnd, dpi);
        let close_button_rect = copy_popup_close_button_rect(hwnd, dpi);
        let editor_frame_rect = copy_popup_editor_frame_rect(hwnd, dpi);
        let old_font = SelectObject(hdc, title_font as _);
        SetBkMode(hdc, TRANSPARENT as i32);
        // The fill is already down; the window region clips the corners, so the
        // rounded pass only needs to stroke the border.
        let shell_pen = cached_overlay_pen(1, typeless_popup_border_color());
        let old_brush = SelectObject(hdc, GetStockObject(HOLLOW_BRUSH) as _);
        let old_pen = SelectObject(hdc, shell_pen as _);
        let shell_radius = scale_desktop_overlay_length(COPY_POPUP_CORNER_RADIUS, dpi);
        RoundRect(
            hdc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            shell_radius,
            shell_radius,
        );
        let _ = SelectObject(hdc, old_pen);
        let _ = SelectObject(hdc, old_brush);

        if let Some(popup) = popup {
            let close_keyboard_focused =
                popup.keyboard_focused_control == CopyPopupHoveredControl::Close;
            let close_fill = if popup.pressed_control == CopyPopupHoveredControl::Close {
                typeless_popup_close_button_pressed_fill_color()
            } else if popup.hovered_control == CopyPopupHoveredControl::Close
                || close_keyboard_focused
            {
                typeless_popup_close_button_hover_fill_color()
            } else {
                typeless_popup_close_button_fill_color()
            };
            let close_border = if popup.pressed_control == CopyPopupHoveredControl::Close {
                typeless_popup_close_button_pressed_border_color()
            } else if popup.hovered_control == CopyPopupHoveredControl::Close
                || close_keyboard_focused
            {
                typeless_popup_close_button_hover_border_color()
            } else {
                typeless_popup_close_button_border_color()
            };
            let close_rect = RECT {
                left: close_button_rect.left,
                top: close_button_rect.top,
                right: close_button_rect.right,
                bottom: close_button_rect.bottom,
            };
            FillRect(hdc, &close_rect, cached_overlay_brush(close_fill) as _);
            stroke_overlay_border_thickness(
                hdc,
                close_rect,
                if close_keyboard_focused { 2 } else { 1 },
                close_border,
            );

            SelectObject(hdc, title_font as _);
            SetTextColor(
                hdc,
                if popup.hovered_control == CopyPopupHoveredControl::Close || close_keyboard_focused
                {
                    typeless_popup_close_hover_color()
                } else {
                    typeless_popup_close_color()
                },
            );
            DrawTextW(
                hdc,
                to_wide("×").as_ptr(),
                -1,
                &mut RECT {
                    left: close_button_rect.left,
                    top: close_button_rect.top,
                    right: close_button_rect.right,
                    bottom: close_button_rect.bottom,
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            if close_keyboard_focused {
                let inset = scale_desktop_overlay_length(3, dpi);
                let focus_rect = RECT {
                    left: close_button_rect.left + inset,
                    top: close_button_rect.top + inset,
                    right: close_button_rect.right - inset,
                    bottom: close_button_rect.bottom - inset,
                };
                let _ = DrawFocusRect(hdc, &focus_rect);
            }

            let editor_brush = cached_overlay_brush(typeless_popup_editor_fill_color());
            let editor_pen = cached_overlay_pen(2, typeless_popup_editor_border_color());
            let old_brush = SelectObject(hdc, editor_brush as _);
            let old_pen = SelectObject(hdc, editor_pen as _);
            RoundRect(
                hdc,
                editor_frame_rect.left,
                editor_frame_rect.top,
                editor_frame_rect.right,
                editor_frame_rect.bottom,
                scale_desktop_overlay_length(4, dpi),
                scale_desktop_overlay_length(4, dpi),
            );
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);

            let panes = copy_popup_visible_panes(&popup.model);
            if panes.len() > 1 {
                let metrics = copy_popup_client_metrics(hwnd, dpi);
                let content_heights = vec![scale_desktop_overlay_length(40, dpi); panes.len()];
                let pane_layouts = desktop_copy_popup_pane_layouts(
                    metrics.width,
                    metrics.height,
                    dpi,
                    &content_heights,
                );
                SelectObject(hdc, label_font as _);
                SetTextColor(hdc, terminal_text_soft_color());
                for (pane, layout) in panes.iter().zip(pane_layouts.iter()) {
                    if pane.label.trim().is_empty() {
                        continue;
                    }
                    let mut label_rect = desktop_overlay_rect_to_rect(layout.label_rect);
                    DrawTextW(
                        hdc,
                        to_wide(&pane.label).as_ptr(),
                        -1,
                        &mut label_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );
                }
            }

            let copy_keyboard_focused =
                popup.keyboard_focused_control == CopyPopupHoveredControl::Copy;
            let copy_fill = if popup.pressed_control == CopyPopupHoveredControl::Copy {
                typeless_popup_button_pressed_fill_color()
            } else if popup.hovered_control == CopyPopupHoveredControl::Copy
                || copy_keyboard_focused
            {
                typeless_popup_button_hover_fill_color()
            } else {
                typeless_popup_button_fill_color()
            };
            let copy_border = if popup.pressed_control == CopyPopupHoveredControl::Copy {
                typeless_popup_button_pressed_border_color()
            } else if popup.hovered_control == CopyPopupHoveredControl::Copy
                || copy_keyboard_focused
            {
                typeless_popup_button_hover_border_color()
            } else {
                typeless_popup_button_border_color()
            };
            let copy_rect = RECT {
                left: copy_button_rect.left,
                top: copy_button_rect.top,
                right: copy_button_rect.right,
                bottom: copy_button_rect.bottom,
            };
            FillRect(hdc, &copy_rect, cached_overlay_brush(copy_fill) as _);
            stroke_overlay_border_thickness(
                hdc,
                copy_rect,
                if copy_keyboard_focused { 2 } else { 1 },
                copy_border,
            );

            SelectObject(hdc, button_font as _);
            SetTextColor(hdc, typeless_popup_button_text_color());
            DrawTextW(
                hdc,
                to_wide(&popup.model.copy_label).as_ptr(),
                -1,
                &mut RECT {
                    left: copy_button_rect.left,
                    top: copy_button_rect.top,
                    right: copy_button_rect.right,
                    bottom: copy_button_rect.bottom,
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            if copy_keyboard_focused {
                let inset = scale_desktop_overlay_length(3, dpi);
                let focus_rect = RECT {
                    left: copy_button_rect.left + inset,
                    top: copy_button_rect.top + inset,
                    right: copy_button_rect.right - inset,
                    bottom: copy_button_rect.bottom - inset,
                };
                let _ = DrawFocusRect(hdc, &focus_rect);
            }
        }

        SelectObject(hdc, old_font);
        if let Some(memory_dc) = buffer {
            blit_overlay_back_buffer(window_hdc, memory_dc, paint.rcPaint);
        }
        EndPaint(hwnd, &paint);
    }

    unsafe fn paint_shortcut_help_window(hwnd: HWND) {
        let mut paint = PAINTSTRUCT::default();
        let window_hdc = BeginPaint(hwnd, &mut paint);
        if window_hdc.is_null() {
            return;
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect);
        let model = overlay_ui_state()
            .lock()
            .ok()
            .and_then(|overlay| overlay.shortcut_help.clone());
        let dpi = overlay_dpi_for_window(hwnd);
        // Same reason as the copy popup: draw the whole sheet off-screen once,
        // then blit, so opening the help window never shows a half-built list.
        let buffer = overlay_back_buffer_dc(
            window_hdc,
            OverlayBackBufferSlot::ShortcutHelp,
            rect.right - rect.left,
            rect.bottom - rect.top,
        );
        let hdc = buffer.unwrap_or(window_hdc);
        FillRect(
            hdc,
            &rect,
            cached_overlay_brush(terminal_panel_fill_color()) as _,
        );
        let title_font = cached_overlay_font(dpi, 15, FW_BOLD as i32);
        let row_title_font = cached_overlay_font(dpi, 12, FW_BOLD as i32);
        let pill_font = cached_overlay_font(dpi, 11, FW_BOLD as i32);
        let old_font = SelectObject(hdc, title_font as _);
        SetBkMode(hdc, TRANSPARENT as i32);
        draw_terminal_shell_chrome(
            hdc,
            rect,
            terminal_signal_color(),
            dpi,
            scale_desktop_overlay_length(SHORTCUT_HELP_CORNER_RADIUS, dpi),
        );

        if let Some(model) = model {
            SelectObject(hdc, title_font as _);
            SetTextColor(hdc, terminal_text_color());
            DrawTextW(
                hdc,
                to_wide(&model.title).as_ptr(),
                -1,
                &mut RECT {
                    left: scale_desktop_overlay_length(20, dpi),
                    top: scale_desktop_overlay_length(16, dpi),
                    right: rect.right - scale_desktop_overlay_length(20, dpi),
                    bottom: scale_desktop_overlay_length(34, dpi),
                },
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );

            for (index, entry) in model.entries.iter().enumerate() {
                let row_top = scale_desktop_overlay_length(52, dpi)
                    + (index as i32 * scale_desktop_overlay_length(38, dpi));
                let row_bottom = row_top + scale_desktop_overlay_length(28, dpi);
                let row_rect = RECT {
                    left: scale_desktop_overlay_length(16, dpi),
                    top: row_top - scale_desktop_overlay_length(2, dpi),
                    right: rect.right - scale_desktop_overlay_length(16, dpi),
                    bottom: row_bottom + scale_desktop_overlay_length(4, dpi),
                };
                let pill_rect = RECT {
                    left: rect.right - scale_desktop_overlay_length(104, dpi),
                    top: row_top + scale_desktop_overlay_length(1, dpi),
                    right: rect.right - scale_desktop_overlay_length(16, dpi),
                    bottom: row_bottom,
                };

                FillRect(
                    hdc,
                    &row_rect,
                    cached_overlay_brush(terminal_panel_light_color()) as _,
                );
                stroke_overlay_border(hdc, row_rect, terminal_border_color());

                SelectObject(hdc, row_title_font as _);
                SetTextColor(hdc, terminal_text_color());
                DrawTextW(
                    hdc,
                    to_wide(&entry.title).as_ptr(),
                    -1,
                    &mut RECT {
                        left: scale_desktop_overlay_length(26, dpi),
                        top: row_top,
                        right: rect.right - scale_desktop_overlay_length(116, dpi),
                        bottom: row_bottom,
                    },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );

                draw_terminal_pill(
                    hdc,
                    pill_rect,
                    dpi,
                    terminal_panel_fill_color(),
                    terminal_border_color(),
                    terminal_text_soft_color(),
                    pill_font,
                    &entry.shortcut,
                );
            }
        }

        SelectObject(hdc, old_font);
        if let Some(memory_dc) = buffer {
            blit_overlay_back_buffer(window_hdc, memory_dc, paint.rcPaint);
        }
        EndPaint(hwnd, &paint);
    }

    /// The Neuro semantic palette and the shades derived from it, evaluated at
    /// compile time. The derived set costs about twenty channel mixes to build,
    /// and every color accessor below used to rebuild it on each call - several
    /// dozen times per painted frame.
    const UI_COLOR_TOKENS: DesktopUiColorTokens = desktop_ui_color_tokens();
    const UI_DERIVED_COLORS: DesktopUiDerivedColors = desktop_ui_derived_colors();

    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }

    const fn rgb_triplet(color: [u8; 3]) -> u32 {
        rgb(color[0], color[1], color[2])
    }

    fn terminal_window_class_background_brush() -> isize {
        static BRUSH: OnceLock<isize> = OnceLock::new();
        *BRUSH.get_or_init(|| unsafe { CreateSolidBrush(terminal_panel_fill_color()) as isize })
    }

    fn copy_popup_edit_brush() -> isize {
        static BRUSH: OnceLock<isize> = OnceLock::new();
        *BRUSH.get_or_init(|| unsafe {
            CreateSolidBrush(typeless_popup_editor_fill_color()) as isize
        })
    }

    fn terminal_signal_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.signal_yellow_rgb)
    }

    fn terminal_signal_border_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.signal_yellow_line_rgb)
    }

    fn terminal_panel_fill_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.panel_rgb)
    }

    fn terminal_panel_light_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.control_rgb)
    }

    fn terminal_border_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.line_rgb)
    }

    fn terminal_text_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.text_rgb)
    }

    fn terminal_text_soft_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.text_muted_rgb)
    }

    fn text_lifecycle_color(state: DesktopTextLifecycleState) -> u32 {
        let colors = UI_COLOR_TOKENS;
        rgb_triplet(match state {
            DesktopTextLifecycleState::AudioWave => colors.text_muted_rgb,
            DesktopTextLifecycleState::PreRecognized => colors.signal_yellow_rgb,
            DesktopTextLifecycleState::Corrected => colors.text_rgb,
        })
    }

    fn listening_shell_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.panel_rgb)
    }

    fn listening_shell_border_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.line_rgb)
    }

    fn listening_cancel_fill_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.control_rgb)
    }

    fn listening_cancel_border_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.line_rgb)
    }

    fn listening_cancel_glyph_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.text_rgb)
    }

    fn listening_confirm_fill_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.signal_yellow_rgb)
    }

    fn listening_confirm_border_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.signal_yellow_line_rgb)
    }

    fn listening_confirm_glyph_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.on_signal_yellow_rgb)
    }

    fn listening_waveform_color() -> u32 {
        terminal_signal_color()
    }

    fn typeless_popup_fill_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.panel_rgb)
    }

    fn typeless_popup_border_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.line_rgb)
    }

    fn typeless_popup_editor_fill_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.focus_surface_rgb)
    }

    fn typeless_popup_editor_border_color() -> u32 {
        terminal_signal_color()
    }

    fn typeless_popup_editor_text_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.focus_ink_rgb)
    }

    fn typeless_popup_close_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.text_muted_rgb)
    }

    fn typeless_popup_close_hover_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.text_rgb)
    }

    fn typeless_popup_close_button_fill_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.control_rgb)
    }

    fn typeless_popup_close_button_border_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.line_rgb)
    }

    fn typeless_popup_close_button_hover_fill_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.control_hover_rgb)
    }

    fn typeless_popup_close_button_hover_border_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.line_rgb)
    }

    fn typeless_popup_close_button_pressed_fill_color() -> u32 {
        rgb_triplet(UI_COLOR_TOKENS.surface_rgb)
    }

    fn typeless_popup_close_button_pressed_border_color() -> u32 {
        terminal_signal_border_color()
    }

    fn typeless_popup_button_fill_color() -> u32 {
        terminal_signal_color()
    }

    fn typeless_popup_button_border_color() -> u32 {
        terminal_signal_border_color()
    }

    fn typeless_popup_button_hover_fill_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.signal_yellow_hover_rgb)
    }

    fn typeless_popup_button_hover_border_color() -> u32 {
        terminal_signal_color()
    }

    fn typeless_popup_button_pressed_fill_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.signal_yellow_pressed_rgb)
    }

    fn typeless_popup_button_pressed_border_color() -> u32 {
        terminal_signal_border_color()
    }

    fn typeless_popup_button_text_color() -> u32 {
        rgb_triplet(UI_DERIVED_COLORS.on_signal_yellow_rgb)
    }

    fn accent_fill_color(state: DesktopHudVisualState) -> u32 {
        let colors = UI_COLOR_TOKENS;
        match state {
            DesktopHudVisualState::Listening | DesktopHudVisualState::Thinking => {
                rgb_triplet(colors.signal_yellow_rgb)
            }
            DesktopHudVisualState::Success => rgb_triplet(colors.signal_green_rgb),
            DesktopHudVisualState::Error => rgb_triplet(colors.danger_red_rgb),
            DesktopHudVisualState::Cancelled => rgb_triplet(colors.control_hover_rgb),
            DesktopHudVisualState::Informational => rgb_triplet(colors.info_blue_rgb),
        }
    }

    fn accent_outline_color(state: DesktopHudVisualState) -> u32 {
        let derived = UI_DERIVED_COLORS;
        match state {
            DesktopHudVisualState::Listening | DesktopHudVisualState::Thinking => {
                rgb_triplet(derived.signal_yellow_line_rgb)
            }
            DesktopHudVisualState::Success => rgb_triplet(derived.signal_green_line_rgb),
            DesktopHudVisualState::Error => rgb_triplet(derived.danger_red_line_rgb),
            DesktopHudVisualState::Cancelled => rgb_triplet(derived.line_rgb),
            DesktopHudVisualState::Informational => rgb_triplet(derived.info_blue_line_rgb),
        }
    }

    fn accent_badge_text_color(state: DesktopHudVisualState) -> u32 {
        let derived = UI_DERIVED_COLORS;
        match state {
            DesktopHudVisualState::Listening | DesktopHudVisualState::Thinking => {
                rgb_triplet(derived.on_signal_yellow_rgb)
            }
            DesktopHudVisualState::Success => rgb_triplet(derived.on_signal_green_rgb),
            DesktopHudVisualState::Error => rgb_triplet(derived.on_danger_red_rgb),
            DesktopHudVisualState::Cancelled => terminal_text_color(),
            DesktopHudVisualState::Informational => rgb_triplet(derived.on_info_blue_rgb),
        }
    }

    fn write_wide_fixed(value: &str, target: &mut [u16]) {
        let wide = to_wide(value);
        let limit = target
            .len()
            .saturating_sub(1)
            .min(wide.len().saturating_sub(1));
        target.fill(0);
        target[..limit].copy_from_slice(&wide[..limit]);
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code < HC_ACTION as i32 {
            return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
        }

        let message = wparam as u32;
        let is_key_down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let is_key_up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
        if !is_key_down && !is_key_up {
            return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
        }

        let key = &*(lparam as *const KBDLLHOOKSTRUCT);
        let mut consume = false;
        // Origin capture is deferred until after the hook-state lock is
        // released so the WH_KEYBOARD_LL callback never blocks other threads
        // (or Windows' LowLevelHooksTimeout) on foreground-window inspection.
        let mut origin_capture_hwnd_value: Option<isize> = None;

        if let Ok(mut state_guard) = low_level_hook_state().lock() {
            if let Some(state) = state_guard.as_mut() {
                match state {
                    LowLevelHookState::OriginCapture {
                        hwnd_value,
                        tracker,
                    } => {
                        let event = tracker.handle_key_event(key.vkCode, is_key_down);
                        consume = false;
                        if event.transition == Some(LowLevelHotkeyTransition::Pressed) {
                            origin_capture_hwnd_value = Some(*hwnd_value);
                        }
                    }
                    LowLevelHookState::Single {
                        hwnd_value,
                        trigger_mode,
                        tracker,
                    } => {
                        let event = tracker.handle_key_event(key.vkCode, is_key_down);
                        consume = event.consume;

                        match event.transition {
                            Some(LowLevelHotkeyTransition::Pressed) => {
                                origin_capture_hwnd_value = Some(*hwnd_value);
                                let _ =
                                    PostMessageW(*hwnd_value as HWND, HOTKEY_ACTION_MESSAGE, 0, 0);
                            }
                            Some(LowLevelHotkeyTransition::Released)
                                if *trigger_mode == TriggerMode::PushToTalk =>
                            {
                                let _ = PostMessageW(
                                    *hwnd_value as HWND,
                                    LOW_LEVEL_HOTKEY_RELEASE_MESSAGE,
                                    0,
                                    0,
                                );
                            }
                            _ => {}
                        }
                    }
                    LowLevelHookState::ToggleRouter { hwnd_value, router } => {
                        let event = router.handle_key_event(key.vkCode, is_key_down);
                        consume = event.consume;
                        match event.pending_hold {
                            ToggleDesktopHotkeyRouterPendingHold::Start { .. } => {
                                origin_capture_hwnd_value = Some(*hwnd_value);
                                let _ = PostMessageW(
                                    *hwnd_value as HWND,
                                    HOTKEY_PENDING_HOLD_START_MESSAGE,
                                    0,
                                    0,
                                );
                            }
                            ToggleDesktopHotkeyRouterPendingHold::Cancelled => {
                                let _ = PostMessageW(
                                    *hwnd_value as HWND,
                                    HOTKEY_PENDING_HOLD_CANCEL_MESSAGE,
                                    0,
                                    0,
                                );
                            }
                            ToggleDesktopHotkeyRouterPendingHold::None => {}
                        }
                        if let Some(action_index) = event.action_index {
                            let _ = PostMessageW(
                                *hwnd_value as HWND,
                                HOTKEY_ACTION_MESSAGE,
                                action_index,
                                0,
                            );
                        }
                    }
                }
            }
        }

        if let Some(hwnd_value) = origin_capture_hwnd_value {
            capture_pending_hotkey_origin_insert_target_from_hook(hwnd_value as HWND);
        }

        if consume {
            1
        } else {
            CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn native_theme_roles_resolve_from_neuro_semantic_tokens() {
            let colors = desktop_ui_color_tokens();
            let derived = desktop_ui_derived_colors();

            assert_eq!(
                terminal_signal_color(),
                rgb_triplet(colors.signal_yellow_rgb)
            );
            assert_eq!(terminal_border_color(), rgb_triplet(derived.line_rgb));
            assert_eq!(terminal_text_color(), rgb_triplet(colors.text_rgb));
            assert_eq!(
                terminal_text_soft_color(),
                rgb_triplet(colors.text_muted_rgb)
            );
            assert_eq!(listening_confirm_fill_color(), terminal_signal_color());
            assert_eq!(typeless_popup_button_fill_color(), terminal_signal_color());
            assert_eq!(
                typeless_popup_button_pressed_fill_color(),
                rgb_triplet(derived.signal_yellow_pressed_rgb)
            );
            assert_eq!(
                typeless_popup_editor_fill_color(),
                rgb_triplet(colors.focus_surface_rgb)
            );
            assert_eq!(
                typeless_popup_editor_text_color(),
                rgb_triplet(colors.focus_ink_rgb)
            );
            assert_eq!(
                typeless_popup_editor_border_color(),
                terminal_signal_color()
            );
            assert_eq!(
                accent_fill_color(DesktopHudVisualState::Success),
                rgb_triplet(colors.signal_green_rgb)
            );
            assert_eq!(
                accent_fill_color(DesktopHudVisualState::Error),
                rgb_triplet(colors.danger_red_rgb)
            );
            assert_eq!(
                accent_fill_color(DesktopHudVisualState::Informational),
                rgb_triplet(colors.info_blue_rgb)
            );
            assert_eq!(
                text_lifecycle_color(DesktopTextLifecycleState::PreRecognized),
                rgb_triplet(colors.signal_yellow_rgb)
            );
            assert_eq!(
                text_lifecycle_color(DesktopTextLifecycleState::Corrected),
                rgb_triplet(colors.text_rgb)
            );
        }

        #[test]
        fn listening_hud_transcript_join_preserves_boundaries_and_lifecycle() {
            assert_eq!(
                listening_hud_transcript_text_and_lifecycle(&DesktopStreamingHudTranscriptParts {
                    corrected_prefix: "\u{3000}第一段。 ".to_string(),
                    pre_recognized_tail: " 第二段\n".to_string(),
                }),
                (
                    Some("第一段。  第二段".to_string()),
                    DesktopTextLifecycleState::PreRecognized,
                )
            );
            assert_eq!(
                listening_hud_transcript_text_and_lifecycle(&DesktopStreamingHudTranscriptParts {
                    corrected_prefix: "\n 已校正文本 \u{3000}".to_string(),
                    pre_recognized_tail: String::new(),
                }),
                (
                    Some("已校正文本".to_string()),
                    DesktopTextLifecycleState::Corrected,
                )
            );
            assert_eq!(
                listening_hud_transcript_text_and_lifecycle(&DesktopStreamingHudTranscriptParts {
                    corrected_prefix: " \u{3000}".to_string(),
                    pre_recognized_tail: "\n\t".to_string(),
                }),
                (None, DesktopTextLifecycleState::PreRecognized)
            );
        }

        #[test]
        fn listening_hud_transcript_join_uses_one_owned_buffer() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn listening_hud_transcript_text_and_lifecycle")
                .expect("listening HUD transcript join");
            let end = source[start..]
                .find("    const LISTENING_HUD_AUTO_FOLLOW_MAX_DISPLAY_UNITS")
                .map(|offset| start + offset)
                .expect("constant following listening HUD transcript join");
            let join_source = &source[start..end];

            assert!(join_source.contains("String::with_capacity("));
            assert!(join_source.contains("display_text.push_str("));
            assert!(!join_source.contains("format!("));
            assert!(!join_source.contains("trim().to_string()"));
        }

        #[test]
        fn native_window_classes_use_the_neuro_theme_background_brush() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn register_named_window_class")
                .expect("window class registration function");
            let end = source[start..]
                .find("    fn initialize_window")
                .map(|offset| start + offset)
                .expect("function following window class registration");
            let registration_source = &source[start..end];

            assert!(registration_source.contains("terminal_window_class_background_brush()"));
            assert!(!registration_source.contains("COLOR_WINDOW"));
        }

        #[test]
        fn live_streaming_anchor_registry_preserves_first_insert_order_on_replacement() {
            let mut inserted_segment_ids = Vec::new();
            let mut inserted_anchors = HashMap::new();

            record_live_streaming_inserted_anchor(
                &mut inserted_segment_ids,
                &mut inserted_anchors,
                SpeculativeInsertAnchor::new(1, Some(2), "seg-1", "first", 10)
                    .expect("first anchor"),
            );
            record_live_streaming_inserted_anchor(
                &mut inserted_segment_ids,
                &mut inserted_anchors,
                SpeculativeInsertAnchor::new(1, Some(2), "seg-2", "second", 20)
                    .expect("second anchor"),
            );
            record_live_streaming_inserted_anchor(
                &mut inserted_segment_ids,
                &mut inserted_anchors,
                SpeculativeInsertAnchor::new(1, Some(2), "seg-1", "corrected", 30)
                    .expect("replacement anchor"),
            );

            assert_eq!(inserted_segment_ids, vec!["seg-1", "seg-2"]);
            assert_eq!(inserted_anchors.len(), 2);
            assert_eq!(inserted_anchors["seg-1"].inserted_text, "corrected");
            assert_eq!(inserted_anchors["seg-1"].inserted_at_ms, 30);
            assert_eq!(inserted_anchors["seg-2"].inserted_text, "second");
        }

        #[test]
        fn streaming_pump_partial_wait_stays_below_the_ui_refresh_interval() {
            assert_eq!(STREAMING_PUMP_PARTIAL_IDLE_TIMEOUT_MS, 1);
            assert!(STREAMING_PUMP_PARTIAL_IDLE_TIMEOUT_MS < HUD_RECORDING_LEVEL_REFRESH_MS.into());
        }

        #[test]
        fn recording_hud_refresh_only_schedules_streaming_io() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn process_recording_hud_refresh")
                .expect("recording HUD processing function");
            let end = source[start..]
                .find("    fn refresh_recording_hud_level")
                .map(|offset| start + offset)
                .expect("function following recording HUD processing");
            let processing_source = &source[start..end];

            assert!(processing_source.contains("streaming_pump.request_pump()"));
            assert!(!processing_source.contains("block_on("));
            assert!(!processing_source.contains("pump_available_audio("));
        }

        #[test]
        fn recording_hud_transcript_refresh_reuses_revisioned_snapshot_and_one_summary() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn process_recording_hud_refresh")
                .expect("recording HUD processing function");
            let end = source[start..]
                .find("    fn refresh_recording_hud_level")
                .map(|offset| start + offset)
                .expect("function following recording HUD processing");
            let processing_source = &source[start..end];

            assert_eq!(
                processing_source
                    .matches("refresh_snapshot_if_changed(")
                    .count(),
                1
            );
            assert!(!processing_source.contains("live_correction_tracker.snapshot()"));
            assert!(processing_source.contains("live_correction_snapshot_revision"));
            assert!(processing_source.contains("live_correction_snapshot"));
            assert!(processing_source
                .contains("desktop_streaming_hud_transcript_summary_with_fallback_text_owned("));
            assert!(processing_source.contains("transcript_summary.effective_segment_count"));
            assert!(processing_source.contains("desktop_streaming_hud_transcript_parts_text("));
            assert!(!processing_source.contains("desktop_streaming_effective_segment_count_owned("));
            assert!(!processing_source.contains("format!("));
            assert!(!processing_source.contains("active_recording_hud_transcript_parts(active)"));
        }

        #[test]
        fn live_correction_snapshot_cache_refreshes_only_for_segment_changes() {
            let tracker = LiveCorrectionTracker::new(1);
            let mut revision = 0;
            let mut snapshot = Vec::new();

            assert!(!tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));
            assert!(snapshot.is_empty());

            assert!(tracker.register_job("seg-1", "local", None));
            assert!(tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));
            assert_eq!(revision, 1);
            assert_eq!(snapshot.len(), 1);
            assert_eq!(snapshot[0].local_text, "local");
            assert!(!tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));

            tracker.record_result("seg-1", "corrected", None);
            assert!(tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));
            assert_eq!(revision, 2);
            assert_eq!(snapshot[0].corrected_text.as_deref(), Some("corrected"));
            assert!(!tracker.has_insert_anchor("seg-1"));
            let backlog = tracker.ordered_backlog();
            assert_eq!(backlog.len(), 1);
            assert_eq!(backlog[0].segment_id, "seg-1");
            assert_eq!(backlog[0].corrected_text, "corrected");

            tracker.record_result("seg-1", "corrected", None);
            assert!(!tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));
            let anchor = SpeculativeInsertAnchor::new(1, Some(2), "seg-1", "corrected", 3)
                .expect("test insert anchor");
            tracker.record_result("seg-1", "corrected", Some(anchor.clone()));
            assert!(tracker.has_insert_anchor("seg-1"));
            assert!(tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));
            assert_eq!(revision, 3);
            assert!(tracker.ordered_backlog().is_empty());
            tracker.record_result("seg-1", "corrected", Some(anchor));
            assert!(!tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));
            tracker.complete_job("seg-1", None, None);
            assert!(!tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));

            assert!(tracker.register_job("seg-2", "second", None));
            assert!(tracker.ordered_backlog().is_empty());
            assert!(tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));
            assert_eq!(revision, 4);
            assert_eq!(snapshot.len(), 2);

            tracker.complete_job("seg-2", Some("updated"), None);
            assert!(tracker.refresh_snapshot_if_changed(&mut revision, &mut snapshot));
            assert_eq!(revision, 5);
            assert_eq!(snapshot[1].corrected_text.as_deref(), Some("updated"));
            let backlog = tracker.ordered_backlog();
            assert_eq!(backlog.len(), 1);
            assert_eq!(backlog[0].segment_id, "seg-2");
            assert_eq!(backlog[0].corrected_text, "updated");
        }

        #[test]
        fn live_correction_tracker_indexes_segment_lookups_without_changing_order() {
            let tracker = LiveCorrectionTracker::new(1);
            for index in 0..256 {
                assert!(tracker.register_job(
                    format!("seg-{index}").as_str(),
                    format!("local-{index}").as_str(),
                    None,
                ));
            }
            assert!(!tracker.register_job("seg-128", "duplicate", None));

            {
                let state = lock_recovering(&tracker.state, "live correction tracker state");
                assert_eq!(state.segment_indices.len(), state.segments.len());
                for (index, segment) in state.segments.iter().enumerate() {
                    assert_eq!(state.segment_index(&segment.segment_id), Some(index));
                }
            }

            assert!(tracker.can_process("seg-255"));
            assert_eq!(
                tracker.local_fallback_text("seg-255").as_deref(),
                Some("local-255")
            );
            let anchor = SpeculativeInsertAnchor::new(1, Some(2), "seg-255", "corrected", 3)
                .expect("test insert anchor");
            tracker.record_result("seg-255", "corrected", Some(anchor));
            assert!(!tracker.can_process("seg-255"));
            assert!(tracker.local_fallback_text("seg-255").is_none());
            assert!(tracker.has_insert_anchor("seg-255"));
            assert!(!tracker.can_process("missing"));
            assert!(tracker.local_fallback_text("missing").is_none());
            assert!(!tracker.has_insert_anchor("missing"));
        }

        #[test]
        fn live_correction_tracker_direct_id_lookups_do_not_scan_segments() {
            let source = include_str!("main.rs");
            let start = source
                .find("    struct LiveCorrectionTrackerState")
                .expect("live correction tracker state");
            let end = source[start..]
                .find("    struct LatestLiveSegmentGuard")
                .map(|offset| start + offset)
                .expect("type following live correction tracker implementation");
            let tracker_source = &source[start..end];

            assert!(tracker_source.contains("segment_indices: HashMap<String, usize>"));
            assert!(tracker_source.contains("state.segment_index(segment_id)?"));
            assert!(!tracker_source.contains(".find(|segment| segment.segment_id == segment_id)"));
            assert!(
                !tracker_source.contains(".position(|segment| segment.segment_id == segment_id)")
            );
        }

        #[test]
        fn live_smart_route_candidate_commit_rejects_stale_or_superseded_work() {
            assert_eq!(
                commit_live_smart_route_candidate(
                    VoiceMode::Smart,
                    None,
                    true,
                    Some(VoiceMode::Transcribe),
                ),
                Some(VoiceMode::Transcribe)
            );
            assert_eq!(
                commit_live_smart_route_candidate(
                    VoiceMode::Smart,
                    Some(VoiceMode::Command),
                    true,
                    Some(VoiceMode::Transcribe),
                ),
                Some(VoiceMode::Command)
            );
            assert_eq!(
                commit_live_smart_route_candidate(
                    VoiceMode::Smart,
                    None,
                    false,
                    Some(VoiceMode::Transcribe),
                ),
                None
            );
            assert_eq!(
                commit_live_smart_route_candidate(
                    VoiceMode::Transcribe,
                    None,
                    true,
                    Some(VoiceMode::Transcribe),
                ),
                None
            );
        }

        #[test]
        fn live_correction_smart_route_computation_runs_outside_shared_lock() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn drain_active_live_correction_backlog")
                .expect("live correction backlog drain");
            let end = source[start..]
                .find("    fn handle_streaming_corrected_hud")
                .map(|offset| start + offset)
                .expect("function following live correction backlog drain");
            let drain_source = &source[start..end];
            let computation_start = drain_source
                .find("        let pending_transcript =")
                .expect("lock-free transcript computation");
            let commit_start = drain_source[computation_start..]
                .find("state.shared.lock()")
                .map(|offset| computation_start + offset)
                .expect("guarded transcript commit");
            let snapshot_source = &drain_source[..computation_start];
            let computation_source = &drain_source[computation_start..commit_start];

            assert!(snapshot_source.contains("state.shared.lock()"));
            assert!(!snapshot_source.contains("desktop_streaming_hud_transcript_summary_owned("));
            assert!(!snapshot_source.contains("desktop_streaming_hud_transcript_parts_text("));
            assert!(!snapshot_source.contains("live_smart_route_for_corrected_transcript("));
            assert!(computation_source.contains(".snapshot_with_revision()"));
            assert!(computation_source.contains("desktop_streaming_hud_transcript_summary_owned("));
            assert!(computation_source.contains("desktop_streaming_hud_transcript_parts_text("));
            assert!(computation_source.contains("live_smart_route_for_corrected_transcript("));
            assert!(!computation_source.contains("state.shared.lock()"));
        }

        #[test]
        fn live_correction_backlog_extraction_avoids_full_tracker_snapshot() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn insert_live_streaming_corrected_backlog_if_safe")
                .expect("live correction backlog insertion helper");
            let end = source[start..]
                .find("    fn queue_live_streaming_corrected_hud")
                .map(|offset| start + offset)
                .expect("function following backlog insertion helper");
            let helper_source = &source[start..end];

            assert!(helper_source.contains("let backlog = tracker.ordered_backlog();"));
            assert!(!helper_source.contains("tracker.snapshot()"));
            assert!(!helper_source.contains("desktop_live_correction_ordered_backlog("));
        }

        #[test]
        fn recording_hud_pending_segments_do_not_materialize_reference_vectors() {
            let source = include_str!("main.rs");
            let backlog_start = source
                .find("    fn drain_active_live_correction_backlog")
                .expect("live correction backlog drain");
            let backlog_end = source[backlog_start..]
                .find("    fn handle_streaming_corrected_hud")
                .map(|offset| backlog_start + offset)
                .expect("function following live correction backlog drain");
            let backlog_source = &source[backlog_start..backlog_end];

            assert!(backlog_source.contains("desktop_streaming_hud_transcript_summary_owned("));
            assert!(backlog_source
                .contains("desktop_streaming_hud_transcript_summary_apply_fallback_text("));
            assert!(backlog_source.contains("transcript_summary.effective_segment_count"));
            assert!(backlog_source.contains("desktop_streaming_hud_transcript_parts_text("));
            assert!(backlog_source.contains("Arc::clone(&active.hud_streaming_segments)"));
            assert!(!backlog_source.contains("active.hud_streaming_segments.clone()"));
            assert_eq!(
                backlog_source.matches(".snapshot_with_revision()").count(),
                1
            );
            assert_eq!(
                backlog_source
                    .matches("desktop_streaming_hud_transcript_summary_owned(")
                    .count(),
                1
            );
            assert!(backlog_source.contains("active.live_correction_snapshot ="));
            assert!(backlog_source.contains("active.hud_streaming_segments_revision"));
            assert!(backlog_source.contains("== candidate.hud_streaming_segments_revision"));
            assert!(!backlog_source.contains("== candidate.hud_streaming_segments\n"));
            assert!(!backlog_source.contains("active.hud_streaming_segments\n                    == candidate.hud_streaming_segments"));
            assert!(!backlog_source.contains("desktop_streaming_effective_segment_count_owned("));
            assert!(!backlog_source.contains("format!("));
            assert!(!backlog_source.contains("collect::<Vec<_>>()"));
        }

        #[test]
        fn corrected_hud_reuses_validated_lock_free_transcript_parts() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn handle_streaming_corrected_hud")
                .expect("streaming corrected HUD handler");
            let end = source[start..]
                .find("    fn handle_correction_copy_popup")
                .map(|offset| start + offset)
                .expect("function following streaming corrected HUD handler");
            let handler_source = &source[start..end];

            assert_eq!(
                handler_source
                    .matches("drain_active_live_correction_backlog(")
                    .count(),
                1
            );
            assert!(handler_source.contains("generation_is_current"));
            assert!(handler_source.contains("overlay.hud_streaming_corrected_prefix.as_deref()"));
            assert!(handler_source.contains("if !transcript_changed"));
            assert!(handler_source.contains("return None;"));
            assert!(!handler_source.contains(".then(|| transcript_parts.corrected_prefix.clone())"));
            assert!(
                !handler_source.contains(".then(|| transcript_parts.pre_recognized_tail.clone())")
            );
            assert!(!handler_source.contains("active_recording_hud_transcript_parts"));
            assert!(!handler_source.contains("refresh_snapshot_if_changed"));
            assert!(!handler_source.contains("desktop_streaming_hud_transcript_summary"));
            assert!(!handler_source.contains("snapshot_with_revision"));
        }

        #[test]
        fn hud_streaming_segment_revision_covers_processing_and_anchor_mutations() {
            let source = include_str!("main.rs");
            let processing_start = source
                .find("    fn process_recording_hud_refresh")
                .expect("recording HUD processing function");
            let processing_end = source[processing_start..]
                .find("    fn refresh_recording_hud_level")
                .map(|offset| processing_start + offset)
                .expect("function following recording HUD processing");
            let processing_source = &source[processing_start..processing_end];
            let mirror_start = source
                .find("    fn mirror_live_streaming_inserted_anchor")
                .expect("live streaming anchor mirror");
            let mirror_end = source[mirror_start..]
                .find("    fn record_live_streaming_inserted_anchor")
                .map(|offset| mirror_start + offset)
                .expect("function following live streaming anchor mirror");
            let mirror_source = &source[mirror_start..mirror_end];

            assert!(processing_source.contains("if upsert_hud_streaming_segment("));
            assert!(processing_source.contains("if remove_hud_streaming_segments("));
            assert!(
                processing_source
                    .matches("hud_streaming_segments_revision")
                    .count()
                    >= 4
            );
            assert!(mirror_source.contains("remove_hud_streaming_segments("));
            assert!(mirror_source.contains("hud_streaming_segments_revision.wrapping_add(1)"));
            assert!(mirror_source.contains("Arc::make_mut(&mut active.hud_streaming_segments)"));
            assert!(processing_source
                .contains("Arc::make_mut(&mut work.processing_state.hud_streaming_segments)"));
        }

        #[test]
        fn recording_start_does_not_wait_for_streaming_ready_on_the_ui_thread() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn begin_recording")
                .expect("begin recording function");
            let end = source[start..]
                .find("    fn finish_recording_begin")
                .map(|offset| start + offset)
                .expect("recording begin continuation");
            let begin_recording_source = &source[start..end];
            let worker_start = source
                .find("    fn spawn_local_asr_recording_prepare")
                .expect("local ASR recording prepare worker");
            let worker_end = source[worker_start..]
                .find("    fn handle_local_asr_prepare_done")
                .map(|offset| worker_start + offset)
                .expect("local ASR prepare completion handler");
            let prepare_worker_source = &source[worker_start..worker_end];

            assert!(begin_recording_source.contains("spawn_local_asr_recording_prepare"));
            assert!(begin_recording_source.contains("spawn_recording_audio_prepare"));
            assert!(begin_recording_source.contains("spawn_preparation_release_watcher"));
            assert!(begin_recording_source.contains("recording_streaming_route_prepare_needed"));
            assert!(!begin_recording_source.contains("ensure_packaged_local_asr_daemon"));
            assert!(!begin_recording_source.contains("start_recording("));
            assert!(!begin_recording_source.contains("block_on("));
            assert!(prepare_worker_source.contains("thread::spawn"));
            assert!(prepare_worker_source.contains("ensure_packaged_local_asr_daemon"));
            assert!(prepare_worker_source.contains("LOCAL_ASR_PREPARE_DONE_MESSAGE"));
        }

        #[test]
        fn explicit_audio_override_never_claims_a_live_streaming_source() {
            assert!(recording_streaming_enabled_for_source(true, true));
            assert!(!recording_streaming_enabled_for_source(true, false));
            assert!(!recording_streaming_enabled_for_source(false, true));
            assert!(!recording_streaming_enabled_for_source(false, false));
            assert!(recording_streaming_route_prepare_needed(true, false));
            assert!(!recording_streaming_route_prepare_needed(true, true));
            assert!(!recording_streaming_route_prepare_needed(false, false));
            assert!(!recording_streaming_route_prepare_needed(false, true));
        }

        #[test]
        fn recording_audio_start_and_failures_stay_outside_the_ui_thread() {
            let source = include_str!("main.rs");
            let prepare_start = source
                .find("    fn prepare_recording_audio")
                .expect("recording audio preparation");
            let worker_start = source[prepare_start..]
                .find("    fn spawn_recording_audio_prepare")
                .map(|offset| prepare_start + offset)
                .expect("recording audio prepare worker");
            let local_completion_start = source[worker_start..]
                .find("    fn handle_local_asr_prepare_done")
                .map(|offset| worker_start + offset)
                .expect("local ASR completion handler");
            let audio_completion_start = source[local_completion_start..]
                .find("    fn handle_recording_audio_start_done")
                .map(|offset| local_completion_start + offset)
                .expect("recording audio completion handler");
            let begin_start = source[audio_completion_start..]
                .find("    fn begin_recording")
                .map(|offset| audio_completion_start + offset)
                .expect("recording begin handler");
            let finish_start = source
                .find("    fn finish_recording_begin")
                .expect("recording begin continuation");
            let finish_end = source[finish_start..]
                .find("    fn desktop_speculative_pipeline_config")
                .map(|offset| finish_start + offset)
                .expect("function following recording begin continuation");
            let prepare_source = &source[prepare_start..worker_start];
            let worker_source = &source[worker_start..local_completion_start];
            let completion_source = &source[audio_completion_start..begin_start];
            let finish_source = &source[finish_start..finish_end];

            assert!(prepare_source.contains("start_recording(&recording_request)"));
            assert!(prepare_source.contains("recording.streaming_pcm_source()"));
            assert!(prepare_source.contains("spawn_local_streaming_asr_pump"));
            assert!(prepare_source.contains("audio_override.is_none()"));
            assert!(worker_source.contains("thread::spawn"));
            assert!(worker_source.contains("AUDIO_START_DONE_MESSAGE"));
            assert!(worker_source.contains("cleanup_prepared_recording_audio"));
            assert!(worker_source.contains("if posted == 0"));
            assert!(worker_source.contains("release_recording_begin_reservation"));
            assert!(completion_source.contains("spawn_failed_session_persistence_worker"));
            assert!(completion_source.contains("release_recording_begin_reservation"));
            assert!(completion_source.contains("finish_recording_begin"));
            assert!(finish_source.contains("PreparedRecordingAudio"));
            assert!(finish_source.contains("spawn_recording_source_cancel_worker"));
            assert!(!finish_source.contains("start_recording("));
            assert!(!finish_source.contains("streaming_pcm_source()"));
            assert!(!finish_source.contains("resolve_desktop_audio_file_override"));
            assert!(!finish_source.contains("recording.cancel()"));
            assert!(finish_source.contains("Talk listening tray status update failed"));
            assert!(finish_source.contains("Talk listening HUD update failed"));
            assert!(finish_source.contains("arm_recording_timeout_timer"));
            assert!(!finish_source.contains("spawn_timeout_watcher"));
            assert!(!finish_source.contains("update_tray_icon(hwnd, \"Talk: listening\")?"));
        }

        #[test]
        fn recording_stop_finalizes_live_audio_outside_the_ui_thread() {
            let source = include_str!("main.rs");
            let helper_start = source
                .find("    async fn finish_recording_in_blocking_worker")
                .expect("recording finalize helper");
            let stop_start = source
                .find("    fn request_stop_recording")
                .expect("recording stop handler");
            let stop_end = source[stop_start..]
                .find("    fn apply_runtime_phase")
                .map(|offset| stop_start + offset)
                .expect("function following recording stop");
            let helper_source = &source[helper_start..stop_start];
            let stop_source = &source[stop_start..stop_end];

            assert!(helper_source.contains("tokio::task::spawn_blocking"));
            assert!(helper_source.contains("recording.finish()"));
            assert!(stop_source.contains("StoppedRecordingSource::LiveRecording"));
            assert!(stop_source.contains("finish_recording_in_blocking_worker(recording).await"));
            assert!(!stop_source.contains("recording.finish()"));
            assert!(!stop_source.contains("block_on("));
        }

        #[test]
        fn recording_cancel_and_shutdown_cleanup_stay_off_the_ui_thread() {
            let source = include_str!("main.rs");
            let helper_start = source
                .find("    fn spawn_recording_source_cancel_worker")
                .expect("recording cancel worker helper");
            let helper_end = source[helper_start..]
                .find("    fn cleanup_shutdown_resources")
                .map(|offset| helper_start + offset)
                .expect("shutdown cleanup helper");
            let cancel_start = source
                .find("    fn cancel_active_recording")
                .expect("active recording cancel handler");
            let cancel_end = source[cancel_start..]
                .find("    fn fail_active_recording")
                .map(|offset| cancel_start + offset)
                .expect("active recording failure handler");
            let fail_end = source[cancel_end..]
                .find("    fn register_or_mark_hotkey_failure")
                .map(|offset| cancel_end + offset)
                .expect("function following active recording failure");
            let destroy_start = source
                .find("            WM_DESTROY =>")
                .expect("window destroy handler");
            let destroy_end = source[destroy_start..]
                .find("            WM_NCDESTROY =>")
                .map(|offset| destroy_start + offset)
                .expect("window non-client destroy handler");

            let helper_source = &source[helper_start..helper_end];
            let cancel_source = &source[cancel_start..cancel_end];
            let fail_source = &source[cancel_end..fail_end];
            let destroy_source = &source[destroy_start..destroy_end];
            assert!(helper_source.contains("runtime_handle.spawn(async move"));
            assert!(helper_source.contains("tokio::task::spawn_blocking"));
            assert!(helper_source.contains("recording.cancel()"));
            assert!(cancel_source.contains("spawn_recording_source_cancel_worker"));
            assert!(cancel_source.contains("spawn_cancelled_session_persistence_worker"));
            assert!(!cancel_source.contains("recording.cancel()"));
            assert!(!cancel_source.contains("complete_cancelled_session("));
            assert!(fail_source.contains("spawn_recording_source_cancel_worker"));
            assert!(fail_source.contains("spawn_failed_session_persistence_worker"));
            assert!(!fail_source.contains("recording.cancel()"));
            assert!(!fail_source.contains("complete_failed_session_with_mode_override("));
            assert!(destroy_source.contains("runtime_handle.spawn_blocking"));
            assert!(destroy_source.contains("cleanup_shutdown_resources"));
            assert!(destroy_source.contains("shared.pending_recording_begin.take()"));
            assert!(destroy_source.contains("shared.pending_config_reload = None"));
            assert!(destroy_source.contains("shared.config_reload_generation.saturating_add(1)"));
            assert!(
                destroy_source.contains("shared.product_bootstrap_generation.saturating_add(1)")
            );
            assert!(destroy_source
                .contains("shared.pending_product_bootstrap_prepare_generation = None"));
            assert!(destroy_source.contains("cleanup_shutdown_resources(active, pending"));
            assert!(!destroy_source.contains("block_on("));
            assert!(!destroy_source.contains("recording.cancel()"));
        }

        #[test]
        fn recording_begin_completion_rejects_stale_idle_and_shutdown_results() {
            let preparing = ShellState::idle()
                .begin_local_asr_preparation()
                .expect("begin local ASR preparation");

            assert!(recording_begin_completion_allowed(false, preparing, 8, 7));
            assert!(!recording_begin_completion_allowed(false, preparing, 9, 7));
            assert!(!recording_begin_completion_allowed(
                false,
                ShellState::idle(),
                8,
                7
            ));
            assert!(!recording_begin_completion_allowed(true, preparing, 8, 7));
        }

        #[test]
        fn pending_recording_prepare_is_connected_to_every_cancellation_boundary() {
            let source = include_str!("main.rs");
            let action_start = source
                .find("    fn handle_desktop_action")
                .expect("desktop action handler");
            let action_end = source[action_start..]
                .find("    fn handle_low_level_hotkey_release")
                .map(|offset| action_start + offset)
                .expect("low-level release handler");
            let stop_start = source
                .find("    fn request_stop_recording")
                .expect("recording stop handler");
            let stop_end = source[stop_start..]
                .find("    fn apply_runtime_phase")
                .map(|offset| stop_start + offset)
                .expect("function following recording stop");
            let reload_start = source
                .find("    fn reload_config")
                .expect("config reload handler");
            let reload_end = source[reload_start..]
                .find("    fn resolve_logs_dir")
                .map(|offset| reload_start + offset)
                .expect("function following config reload");
            let cancel_start = source
                .find("    fn cancel_pending_recording_begin")
                .expect("pending recording preparation cancellation handler");
            let cancel_end = source[cancel_start..]
                .find("    fn spawn_local_asr_recording_prepare")
                .map(|offset| cancel_start + offset)
                .expect("function following pending preparation cancellation");

            assert!(source[action_start..action_end].contains("cancel_pending_recording_begin"));
            assert!(source[stop_start..stop_end].contains("cancel_pending_recording_begin"));
            assert!(source[reload_start..reload_end].contains("cancel_pending_recording_begin"));
            assert!(source[cancel_start..cancel_end].contains("cleanup_pending_recording_begin"));
            assert!(source.contains("shared.pending_recording_begin = None;"));
        }

        #[test]
        fn config_reload_completion_rejects_stale_and_shutdown_results() {
            assert!(config_reload_completion_allowed(false, 7, 7));
            assert!(!config_reload_completion_allowed(false, 8, 7));
            assert!(!config_reload_completion_allowed(true, 7, 7));
        }

        #[test]
        fn config_reload_io_and_daemon_reconcile_stay_outside_the_ui_thread() {
            let source = include_str!("main.rs");
            let prepare_start = source
                .find("    fn prepare_config_reload")
                .expect("config reload prepare function");
            let release_start = source[prepare_start..]
                .find("    fn release_config_reload_reservation")
                .map(|offset| prepare_start + offset)
                .expect("config reload reservation release");
            let worker_start = source[release_start..]
                .find("    fn spawn_config_reload_prepare")
                .map(|offset| release_start + offset)
                .expect("config reload prepare worker");
            let daemon_start = source[worker_start..]
                .find("    fn spawn_config_reload_daemon_reconcile")
                .map(|offset| worker_start + offset)
                .expect("config reload daemon worker");
            let handler_start = source[daemon_start..]
                .find("    fn handle_config_reload_done")
                .map(|offset| daemon_start + offset)
                .expect("config reload completion handler");
            let reload_start = source[handler_start..]
                .find("    fn reload_config")
                .map(|offset| handler_start + offset)
                .expect("config reload request handler");
            let reload_end = source[reload_start..]
                .find("    fn resolve_logs_dir")
                .map(|offset| reload_start + offset)
                .expect("function following config reload request");
            let begin_start = source
                .find("    fn begin_recording")
                .expect("recording begin handler");
            let begin_end = source[begin_start..]
                .find("    fn finish_recording_begin")
                .map(|offset| begin_start + offset)
                .expect("recording begin continuation");

            let prepare_source = &source[prepare_start..release_start];
            let release_source = &source[release_start..worker_start];
            let worker_source = &source[worker_start..daemon_start];
            let daemon_source = &source[daemon_start..handler_start];
            let handler_source = &source[handler_start..reload_start];
            let reload_source = &source[reload_start..reload_end];
            let begin_source = &source[begin_start..begin_end];

            assert!(prepare_source.contains(".block_on(load_effective_config(config_path))"));
            assert!(prepare_source.contains("configured_native_readiness(&config)"));
            assert!(worker_source.contains("thread::spawn"));
            assert!(worker_source.contains("catch_unwind"));
            assert!(worker_source.contains("CONFIG_RELOAD_DONE_MESSAGE"));
            assert!(worker_source.contains("if posted == 0"));
            assert!(worker_source.contains("release_config_reload_reservation"));
            assert!(release_source.contains("shared.pending_config_reload = None"));
            assert!(daemon_source.contains("thread::spawn"));
            assert!(daemon_source.contains("stop_managed_local_asr_daemon"));
            assert!(daemon_source.contains("run_product_local_asr_daemon_prewarm"));
            assert!(handler_source.contains("register_or_mark_hotkey_failure"));
            assert!(handler_source.contains("spawn_config_reload_daemon_reconcile"));
            assert!(!handler_source.contains(".block_on("));
            assert!(!handler_source.contains("configured_native_readiness"));
            assert!(!handler_source.contains("stop_managed_local_asr_daemon("));
            assert!(reload_source.contains("spawn_config_reload_prepare"));
            assert!(reload_source.contains("shared.shell_state.can_start_session()"));
            assert!(reload_source.contains("shared.active_recording.is_some()"));
            assert!(!reload_source.contains(".block_on("));
            assert!(!reload_source.contains("configured_native_readiness"));
            assert!(begin_source.contains("shared.pending_config_reload.is_some()"));
        }

        #[test]
        fn desktop_cold_start_exposes_loading_shell_before_config_and_product_io() {
            let source = include_str!("main.rs");
            let run_start = source
                .find("    pub fn run()")
                .expect("desktop run function");
            let run_end = source[run_start..]
                .find("    fn register_window_class")
                .map(|offset| run_start + offset)
                .expect("function following desktop run");
            let product_prepare_start = source
                .find("    fn prepare_product_bootstrap")
                .expect("product bootstrap preparation");
            let product_prepare_end = source[product_prepare_start..]
                .find("    fn commit_product_bootstrap_if_current")
                .map(|offset| product_prepare_start + offset)
                .expect("product bootstrap commit helper");
            let product_download_start = source
                .find("    fn start_product_model_download")
                .expect("product model download function");
            let product_start = source
                .find("    fn start_product_bootstrap")
                .expect("product bootstrap start function");
            let product_start_end = source[product_start..]
                .find("    fn handle_model_bootstrap_status")
                .map(|offset| product_start + offset)
                .expect("product bootstrap completion handler");
            let config_completion_start = source
                .find("    fn handle_config_reload_done")
                .expect("config load completion handler");
            let config_completion_end = source[config_completion_start..]
                .find("    fn reload_config")
                .map(|offset| config_completion_start + offset)
                .expect("config load request handler");

            let run_source = &source[run_start..run_end];
            let product_prepare_source = &source[product_prepare_start..product_prepare_end];
            let product_download_source = &source[product_download_start..product_start];
            let product_start_source = &source[product_start..product_start_end];
            let config_completion_source = &source[config_completion_start..config_completion_end];

            let initialize_position = run_source
                .find("initialize_window(hwnd, instance)")
                .expect("window initialization");
            let reload_position = run_source
                .find("reload_config(hwnd)?")
                .expect("background startup config request");
            let message_loop_position = run_source
                .find("GetMessageW")
                .expect("Windows message loop");
            assert!(initialize_position < reload_position);
            assert!(reload_position < message_loop_position);
            assert!(run_source.contains("ConfigAvailability::loading()"));
            assert!(!run_source.contains("load_effective_config"));
            assert!(!run_source.contains("configured_native_readiness"));
            assert!(!run_source.contains("fs::read"));
            assert!(product_prepare_source.contains("locate_verified_embedded_runtime"));
            let fast_path_position = product_prepare_source
                .find("locate_verified_embedded_runtime")
                .expect("runtime cache fast path lookup");
            let full_read_position = product_prepare_source
                .find("fs::read(&executable_path)")
                .expect("runtime payload full read fallback");
            assert!(fast_path_position < full_read_position);
            assert!(product_prepare_source.contains("extract_embedded_runtime_payload"));
            assert!(product_prepare_source.contains("validate_installed_model"));
            assert!(product_download_source.contains("tokio::sync::oneshot::channel"));
            assert!(product_download_source.contains(".catch_unwind()"));
            assert!(product_download_source.contains("pending_model_bootstrap_task.take()"));
            assert!(product_download_source.contains("spawn_product_local_asr_daemon_prewarm"));
            assert!(product_start_source.contains("thread::spawn"));
            assert!(product_start_source.contains("catch_unwind"));
            assert!(product_start_source.contains("spawn_product_local_asr_daemon_prewarm"));
            assert!(product_start_source.contains("pending_product_bootstrap_prepare_generation"));
            assert!(!product_start_source.contains("fs::read"));
            assert!(config_completion_source.contains("start_product_bootstrap"));
        }

        #[test]
        fn shell_menu_actions_do_not_hold_shared_state_across_blocking_os_calls() {
            let source = include_str!("main.rs");
            let open_logs_start = source
                .find("    fn open_logs_folder")
                .expect("open logs handler");
            let open_config_start = source[open_logs_start..]
                .find("    fn open_config_file")
                .map(|offset| open_logs_start + offset)
                .expect("open config handler");
            let external_open_start = source[open_config_start..]
                .find("    fn spawn_external_open")
                .map(|offset| open_config_start + offset)
                .expect("external shell worker");
            let status_start = source[external_open_start..]
                .find("    fn show_status_dialog")
                .map(|offset| external_open_start + offset)
                .expect("status dialog handler");
            let status_end = source[status_start..]
                .find("    fn config_reload_completion_allowed")
                .map(|offset| status_start + offset)
                .expect("function following status dialog");

            let open_logs_source = &source[open_logs_start..open_config_start];
            let open_config_source = &source[open_config_start..external_open_start];
            let external_open_source = &source[external_open_start..status_start];
            let status_source = &source[status_start..status_end];

            assert!(open_logs_source.contains("let logs_dir = {"));
            assert!(open_logs_source.contains("spawn_external_open"));
            assert!(!open_logs_source.contains("Command::new"));
            assert!(open_config_source.contains("shared.config_path.clone()"));
            assert!(open_config_source.contains("spawn_external_open"));
            assert!(!open_config_source.contains("Command::new"));
            assert!(external_open_source.contains("thread::Builder::new()"));
            assert!(external_open_source.contains("Command::new(program)"));
            assert!(status_source.contains("let snapshot = {"));
            assert!(status_source.contains("let report = build_status_report(&snapshot)"));
            assert!(status_source.contains("MessageBoxW"));
            assert!(!status_source.contains("build_status_report(&status_snapshot(&shared))"));
        }

        #[test]
        fn local_asr_daemon_epoch_rejects_stale_and_shutdown_operations() {
            assert!(local_asr_daemon_operation_allowed(false, 7, 7));
            assert!(!local_asr_daemon_operation_allowed(false, 8, 7));
            assert!(!local_asr_daemon_operation_allowed(true, 7, 7));
        }

        #[test]
        fn local_asr_daemon_config_identity_ignores_unrelated_settings() {
            let original = correction_job_test_config();
            let mut unrelated_change = (*original).clone();
            unrelated_change.audio.max_recording_seconds = 30;
            assert!(local_asr_daemon_config_matches(
                &original,
                &unrelated_change
            ));

            let mut route_change = (*original).clone();
            route_change.speculative.enabled = true;
            route_change.speculative.local_asr = "streaming_service".to_string();
            assert!(!local_asr_daemon_config_matches(&original, &route_change));
        }

        #[test]
        fn daemon_reconcile_keeps_process_and_network_operations_outside_direct_state_locks() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn reconcile_managed_local_asr_daemon")
                .expect("daemon reconcile function");
            let end = source[start..]
                .find("    fn run_product_local_asr_daemon_prewarm")
                .map(|offset| start + offset)
                .expect("function following daemon reconcile");
            let reconcile_source = &source[start..end];

            assert!(!reconcile_source.contains(".lock()"));
            assert!(reconcile_source.contains("managed_local_asr_daemon_is_running"));
            assert!(reconcile_source.contains("local_asr_endpoint_accepts_tcp"));
            assert!(reconcile_source.contains("start_managed_local_asr_daemon"));
            assert!(reconcile_source.contains("stop_managed_local_asr_daemon"));
        }

        #[test]
        fn daemon_startup_status_errors_attempt_child_cleanup_before_returning() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn start_managed_local_asr_daemon")
                .expect("daemon start function");
            let end = source[start..]
                .find("    fn packaged_local_asr_daemon_request")
                .map(|offset| start + offset)
                .expect("function following daemon start");
            let start_source = &source[start..end];
            let status_error = start_source
                .find("check packaged local ASR daemon status")
                .expect("daemon status error branch");
            let cleanup_prefix = &start_source[..status_error];
            let cleanup_prefix =
                &cleanup_prefix[cleanup_prefix.len().saturating_sub(800)..cleanup_prefix.len()];

            assert!(cleanup_prefix.contains("child.kill()"));
            assert!(cleanup_prefix.contains("child.wait()"));
        }

        #[test]
        fn latest_streaming_asr_text_change_check_borrows_and_compares_the_last_incoming_event() {
            let current = StreamingAsrEvent::partial("seg-1", "  unchanged text  ");

            assert!(!latest_streaming_asr_text_changed(Some(&current), &[]));
            assert!(!latest_streaming_asr_text_changed(
                Some(&current),
                &[StreamingAsrEvent::partial("seg-1", "unchanged text")],
            ));
            assert!(latest_streaming_asr_text_changed(
                Some(&current),
                &[StreamingAsrEvent::partial("seg-1", "changed text")],
            ));
            assert!(!latest_streaming_asr_text_changed(
                Some(&current),
                &[
                    StreamingAsrEvent::partial("seg-1", "intermediate text"),
                    StreamingAsrEvent::partial("seg-1", " unchanged text "),
                ],
            ));
            assert!(latest_streaming_asr_text_changed(
                None,
                &[StreamingAsrEvent::partial("seg-1", "first text")],
            ));
        }

        #[test]
        fn streaming_idle_segmentation_runs_only_when_a_readiness_threshold_is_crossed() {
            let config = desktop_live_streaming_segmenter_config();

            assert!(!streaming_idle_segmentation_due(0, 299, &config));
            assert!(streaming_idle_segmentation_due(299, 300, &config));
            assert!(!streaming_idle_segmentation_due(300, 619, &config));
            assert!(streaming_idle_segmentation_due(619, 620, &config));
            assert!(!streaming_idle_segmentation_due(620, 2_000, &config));
            assert!(streaming_idle_segmentation_due(0, 2_000, &config));
        }

        #[test]
        fn recording_hud_event_budget_preserves_deferred_events_in_order() {
            let mut pending = VecDeque::from([
                Ok(vec![
                    StreamingAsrEvent::partial("seg-1", "first"),
                    StreamingAsrEvent::partial("seg-2", "second"),
                    StreamingAsrEvent::partial("seg-3", "third"),
                ]),
                Ok(vec![StreamingAsrEvent::partial("seg-4", "fourth")]),
            ]);

            let (first_batch, first_error) =
                take_streaming_asr_events_for_hud_refresh(&mut pending, 2);
            assert_eq!(
                first_batch
                    .iter()
                    .map(StreamingAsrEvent::text)
                    .collect::<Vec<_>>(),
                vec!["first", "second"]
            );
            assert_eq!(first_error, None);

            let (second_batch, second_error) =
                take_streaming_asr_events_for_hud_refresh(&mut pending, 8);
            assert_eq!(
                second_batch
                    .iter()
                    .map(StreamingAsrEvent::text)
                    .collect::<Vec<_>>(),
                vec!["third", "fourth"]
            );
            assert_eq!(second_error, None);
            assert!(pending.is_empty());

            let source = include_str!("main.rs");
            let start = source
                .find("    fn take_streaming_asr_events_for_hud_refresh")
                .expect("streaming ASR HUD event budget helper");
            let end = source[start..]
                .find("    fn process_recording_hud_refresh")
                .map(|offset| start + offset)
                .expect("function following streaming ASR HUD event budget helper");
            let helper_source = &source[start..end];
            assert!(helper_source.contains("events.extend(available.drain(..remaining_capacity))"));
            assert!(helper_source.contains("pending_results.push_front(Ok(available))"));
            assert!(!helper_source.contains("split_off("));
        }

        #[test]
        fn recording_hud_event_budget_stops_at_error_and_keeps_later_results() {
            let mut pending = VecDeque::from([
                Ok(vec![StreamingAsrEvent::partial("seg-1", "before")]),
                Err("pump failed".to_string()),
                Ok(vec![StreamingAsrEvent::partial("seg-2", "after")]),
            ]);

            let (events, error) =
                take_streaming_asr_events_for_hud_refresh(&mut pending, usize::MAX);
            assert_eq!(
                events
                    .iter()
                    .map(StreamingAsrEvent::text)
                    .collect::<Vec<_>>(),
                vec!["before"]
            );
            assert_eq!(error.as_deref(), Some("pump failed"));

            let (remaining, remaining_error) =
                take_streaming_asr_events_for_hud_refresh(&mut pending, usize::MAX);
            assert_eq!(
                remaining
                    .iter()
                    .map(StreamingAsrEvent::text)
                    .collect::<Vec<_>>(),
                vec!["after"]
            );
            assert_eq!(remaining_error, None);
            assert!(pending.is_empty());
        }

        #[test]
        fn recording_hud_zero_event_budget_does_not_consume_pending_results() {
            let mut pending =
                VecDeque::from([Ok(vec![StreamingAsrEvent::partial("seg-1", "pending")])]);

            let (events, error) = take_streaming_asr_events_for_hud_refresh(&mut pending, 0);

            assert!(events.is_empty());
            assert_eq!(error, None);
            assert_eq!(pending.len(), 1);
        }

        #[test]
        fn recording_hud_processing_runs_between_generation_guarded_state_locks() {
            let source = include_str!("main.rs");
            let processing_start = source
                .find("    fn process_recording_hud_refresh")
                .expect("recording HUD processing function");
            let refresh_start = source[processing_start..]
                .find("    fn refresh_recording_hud_level")
                .map(|offset| processing_start + offset)
                .expect("recording HUD refresh function");
            let refresh_end = source[refresh_start..]
                .find("    fn refresh_thinking_hud_progress")
                .map(|offset| refresh_start + offset)
                .expect("function following recording HUD refresh");
            let processing_source = &source[processing_start..refresh_start];
            let refresh_source = &source[refresh_start..refresh_end];
            let process_call = refresh_source
                .find("let processed = process_recording_hud_refresh(work);")
                .expect("lock-free HUD processing call");
            let first_lock = refresh_source
                .find("lock_recovering(&state.shared")
                .expect("HUD snapshot lock");
            let commit_lock = refresh_source[process_call..]
                .find("lock_recovering(&state.shared")
                .map(|offset| process_call + offset)
                .expect("HUD generation-guarded commit lock");

            assert!(!processing_source.contains("state.shared.lock()"));
            assert!(!processing_source.contains("lock_recovering(&state.shared"));
            assert!(first_lock < process_call);
            assert!(process_call < commit_lock);
            assert!(refresh_source[commit_lock..].contains("active.generation != generation"));
        }

        #[test]
        fn recording_hud_idle_snapshot_defers_dispatch_metadata_clones_until_commit() {
            let source = include_str!("main.rs");
            let refresh_start = source
                .find("    fn refresh_recording_hud_level")
                .expect("recording HUD refresh function");
            let refresh_end = source[refresh_start..]
                .find("    fn refresh_thinking_hud_progress")
                .map(|offset| refresh_start + offset)
                .expect("function following recording HUD refresh");
            let refresh_source = &source[refresh_start..refresh_end];
            let process_call = refresh_source
                .find("let processed = process_recording_hud_refresh(work);")
                .expect("lock-free HUD processing call");
            let snapshot_source = &refresh_source[..process_call];
            let commit_source = &refresh_source[process_call..];
            let dispatch_seed_guard = commit_source
                .find("let config = live_dispatch_seed")
                .expect("conditional dispatch metadata snapshot");
            let anchor_clone = commit_source
                .find("active.live_streaming_inserted_anchors.clone()")
                .expect("changed-transcript anchor snapshot");
            let commit_complete = commit_source
                .find("if !committed")
                .expect("generation-guarded commit completion");
            let pipeline_config = commit_source
                .find("desktop_speculative_pipeline_config(&metadata.config)")
                .expect("lock-free pipeline config construction");

            assert!(!snapshot_source.contains("shared.config.clone()"));
            assert!(!snapshot_source.contains("active.origin_insert_target.clone()"));
            assert!(!snapshot_source.contains("active.live_streaming_inserted_anchors.clone()"));
            assert!(!snapshot_source.contains("active.live_streaming_correction_sender.clone()"));
            assert!(!snapshot_source.contains("active.streaming_unavailable_hint.clone()"));
            assert!(snapshot_source.contains("active.streaming_unavailable_hint.take()"));
            assert!(dispatch_seed_guard < anchor_clone);
            assert!(commit_complete < pipeline_config);
            assert!(commit_source.contains(".and_then(|_| shared.config.clone())"));
            assert!(commit_source.contains(".zip(live_dispatch_metadata)"));
            assert!(commit_source.contains("live_dispatch_metadata.is_none()"));
            assert!(commit_source.contains("dispatch metadata is unavailable during recording"));
            assert!(commit_source
                .contains("active.streaming_unavailable_hint = streaming_unavailable_hint"));
        }

        #[test]
        fn recording_hud_hot_path_reuses_waveform_storage_and_consumes_pumped_events() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn process_recording_hud_refresh")
                .expect("recording HUD processing function");
            let end = source[start..]
                .find("    fn refresh_recording_hud_level")
                .map(|offset| start + offset)
                .expect("function following recording HUD processing");
            let processing_source = &source[start..end];

            assert!(processing_source.contains("current_waveform_into(&mut work.raw_waveform)"));
            assert!(processing_source.contains("for event in pumped_asr_events"));
            assert!(processing_source.contains("Vec::with_capacity(pumped_asr_events.len())"));
            assert!(
                processing_source.contains("last_streaming_asr_event_at = Some(Instant::now())")
            );
            assert!(!processing_source.contains("let now = Instant::now()"));
            assert!(!processing_source.contains("pumped_asr_events.iter().cloned()"));
            assert!(!processing_source.contains("current_waveform(9)"));
        }

        #[test]
        fn overlay_glyph_utf16_encoding_handles_bmp_and_supplementary_characters() {
            let (ascii, ascii_len) = encode_overlay_glyph_utf16('A');
            assert_eq!(&ascii[..ascii_len], &[0x0041]);

            let (cjk, cjk_len) = encode_overlay_glyph_utf16('界');
            assert_eq!(&cjk[..cjk_len], &[0x754c]);

            let (supplementary, supplementary_len) = encode_overlay_glyph_utf16('😀');
            assert_eq!(supplementary_len, 2);
            assert_eq!(
                String::from_utf16(&supplementary[..supplementary_len]).unwrap(),
                "😀"
            );
        }

        #[tokio::test]
        async fn streaming_pump_requests_coalesce_while_one_is_queued() {
            let (pump_sender, mut pump_receiver) = tokio::sync::mpsc::channel(1);
            let (terminal_sender, _terminal_receiver) = tokio::sync::mpsc::channel(1);
            let controller = LocalStreamingAsrPumpController {
                pump_sender,
                terminal_sender,
            };

            controller.request_pump();
            controller.request_pump();

            assert_eq!(pump_receiver.recv().await, Some(()));
            assert!(matches!(
                pump_receiver.try_recv(),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty)
            ));
        }

        #[test]
        fn streaming_terminal_command_has_a_separate_priority_queue() {
            let (pump_sender, _pump_receiver) = tokio::sync::mpsc::channel(1);
            let (terminal_sender, mut terminal_receiver) = tokio::sync::mpsc::channel(1);
            let controller = LocalStreamingAsrPumpController {
                pump_sender,
                terminal_sender,
            };
            controller.request_pump();
            let (reply_sender, _reply_receiver) = tokio::sync::oneshot::channel();

            controller
                .terminal_sender
                .try_send(LocalStreamingAsrPumpTerminalCommand::Cancel(reply_sender))
                .expect("terminal command must not share pump backpressure");

            assert!(matches!(
                terminal_receiver.try_recv(),
                Ok(LocalStreamingAsrPumpTerminalCommand::Cancel(_))
            ));
        }

        #[test]
        fn copy_popup_edit_style_exposes_native_vertical_scrolling() {
            let style = copy_popup_edit_control_style();
            assert_ne!(style & ES_MULTILINE as u32, 0);
            assert_ne!(style & ES_AUTOVSCROLL as u32, 0);
            assert_ne!(
                style & windows_sys::Win32::UI::WindowsAndMessaging::WS_VSCROLL,
                0
            );
        }

        #[test]
        fn remove_hud_streaming_segments_drops_only_named_ids_preserving_order() {
            let mut segments = vec![
                ("seg-1".to_string(), "第一句。".to_string()),
                ("seg-1#2".to_string(), "第二句。".to_string()),
                ("seg-1#3".to_string(), "第三句。".to_string()),
            ];

            assert!(remove_hud_streaming_segments(
                &mut segments,
                &["seg-1#2".to_string()]
            ));

            assert_eq!(
                segments,
                vec![
                    ("seg-1".to_string(), "第一句。".to_string()),
                    ("seg-1#3".to_string(), "第三句。".to_string()),
                ]
            );
        }

        #[test]
        fn remove_hud_streaming_segments_ignores_empty_id_list() {
            let mut segments = vec![("seg-1".to_string(), "第一句。".to_string())];

            assert!(!remove_hud_streaming_segments(&mut segments, &[]));

            assert_eq!(
                segments,
                vec![("seg-1".to_string(), "第一句。".to_string())]
            );
        }

        #[test]
        fn hud_streaming_segment_mutations_report_only_actual_changes() {
            let mut segments = Vec::new();

            assert!(upsert_hud_streaming_segment(
                &mut segments,
                "seg-1",
                "first"
            ));
            assert!(!upsert_hud_streaming_segment(
                &mut segments,
                "seg-1",
                "first"
            ));
            assert!(upsert_hud_streaming_segment(
                &mut segments,
                "seg-1",
                "updated"
            ));
            assert_eq!(segments, vec![("seg-1".to_string(), "updated".to_string())]);

            assert!(!remove_hud_streaming_segments(
                &mut segments,
                &["missing".to_string()]
            ));
            assert!(remove_hud_streaming_segments(
                &mut segments,
                &["seg-1".to_string()]
            ));
            assert!(segments.is_empty());
        }

        #[test]
        fn hud_streaming_segment_upsert_updates_tail_and_falls_back_for_older_segments() {
            let mut segments = vec![
                ("seg-1".to_string(), "first".to_string()),
                ("seg-2".to_string(), "second".to_string()),
            ];

            assert!(!upsert_hud_streaming_segment(
                &mut segments,
                "seg-2",
                "second"
            ));
            assert!(upsert_hud_streaming_segment(
                &mut segments,
                "seg-2",
                "second updated"
            ));
            assert!(upsert_hud_streaming_segment(
                &mut segments,
                "seg-1",
                "first revised"
            ));

            assert_eq!(
                segments,
                vec![
                    ("seg-1".to_string(), "first revised".to_string()),
                    ("seg-2".to_string(), "second updated".to_string()),
                ]
            );
        }

        #[test]
        fn hud_streaming_segment_upsert_checks_tail_before_linear_scan() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn upsert_hud_streaming_segment")
                .expect("HUD streaming segment upsert helper");
            let end = source[start..]
                .find("    fn update_hud_streaming_segment_text")
                .map(|offset| start + offset)
                .expect("function following HUD streaming segment upsert helper");
            let helper_source = &source[start..end];
            let tail_check = helper_source
                .find("segments.last_mut()")
                .expect("tail fast path");
            let fallback_scan = helper_source
                .find(".iter_mut()")
                .expect("older-segment fallback scan");

            assert!(tail_check < fallback_scan);
            assert!(helper_source.contains("segments[..search_end]"));
        }

        #[test]
        fn batch_hud_streaming_segment_removal_preserves_unrelated_order() {
            let mut segments = (0..128)
                .map(|index| (format!("seg-{index}"), format!("text-{index}")))
                .collect::<Vec<_>>();
            let mut removed = (0..128)
                .filter(|index| index % 3 == 0)
                .map(|index| format!("seg-{index}"))
                .collect::<Vec<_>>();
            removed.push("seg-0".to_string());
            removed.push("missing".to_string());

            assert!(remove_hud_streaming_segments(&mut segments, &removed));
            assert_eq!(segments.len(), 85);
            assert!(segments.iter().all(|(segment_id, _)| segment_id
                .strip_prefix("seg-")
                .and_then(|index| index.parse::<usize>().ok())
                .is_some_and(|index| index % 3 != 0)));
            assert!(segments
                .windows(2)
                .all(|pair| pair[0].0[4..].parse::<usize>().unwrap()
                    < pair[1].0[4..].parse::<usize>().unwrap()));
        }

        #[test]
        fn batch_hud_streaming_segment_removal_uses_borrowed_hash_set() {
            let source = include_str!("main.rs");
            let start = source
                .find("    fn remove_hud_streaming_segments")
                .expect("HUD streaming segment removal helper");
            let end = source[start..]
                .find("    fn display_text_units")
                .map(|offset| start + offset)
                .expect("function following HUD streaming segment removal helper");
            let helper_source = &source[start..end];

            assert!(helper_source.contains("if let [removed] = segment_ids"));
            assert!(helper_source.contains("collect::<HashSet<_>>()"));
            assert!(!helper_source.contains("segment_ids.iter().any"));
        }

        #[test]
        fn copy_popup_edit_background_is_opaque_for_native_scrolling() {
            assert_eq!(
                copy_popup_edit_background_mode(),
                windows_sys::Win32::Graphics::Gdi::OPAQUE as i32
            );
        }

        #[test]
        fn live_smart_route_update_waits_for_corrected_plain_transcription() {
            let long_transcript = "这是一段需要完整保留的转录内容。".repeat(12);
            assert_eq!(
                live_smart_route_for_transcript(VoiceMode::Smart, None, &long_transcript, 4,),
                Some(VoiceMode::Transcribe)
            );
            assert_eq!(
                live_smart_route_for_transcript(VoiceMode::Smart, None, "今天下午开会。", 1),
                None
            );
            assert_eq!(
                live_smart_route_for_corrected_transcript(
                    VoiceMode::Smart,
                    None,
                    "今天下午开会。",
                    1,
                ),
                Some(VoiceMode::Transcribe)
            );
            assert_eq!(
                live_smart_route_for_corrected_transcript(VoiceMode::Smart, None, "打开记事本", 1,),
                None
            );
            assert_eq!(
                live_smart_route_for_corrected_transcript(VoiceMode::Smart, None, "帮我。", 1,),
                None
            );
            assert_eq!(
                live_smart_route_for_corrected_transcript(
                    VoiceMode::Smart,
                    None,
                    "帮我写一封邮件。",
                    2,
                ),
                None
            );
            assert_eq!(
                live_smart_route_for_transcript(VoiceMode::Command, None, &long_transcript, 4,),
                None
            );
        }

        #[test]
        fn queued_smart_job_can_apply_after_live_route_locks_to_transcribe() {
            assert!(live_correction_job_target_apply_allowed(
                false,
                true,
                VoiceMode::Smart,
                Some(VoiceMode::Transcribe),
            ));
            assert!(!live_correction_job_target_apply_allowed(
                false,
                true,
                VoiceMode::Smart,
                None,
            ));
            assert!(!live_correction_job_target_apply_allowed(
                false,
                false,
                VoiceMode::Smart,
                Some(VoiceMode::Transcribe),
            ));
            assert!(live_correction_job_target_apply_allowed(
                true,
                true,
                VoiceMode::Transcribe,
                None,
            ));
            assert!(!live_correction_job_target_apply_allowed(
                false,
                true,
                VoiceMode::Command,
                None,
            ));
        }

        #[test]
        fn live_correction_backlog_drain_requires_dispatch_target_permission() {
            assert!(!live_correction_backlog_drain_allowed(
                false,
                true,
                false,
                VoiceMode::Transcribe,
                None,
            ));
            assert!(live_correction_backlog_drain_allowed(
                true,
                true,
                false,
                VoiceMode::Transcribe,
                None,
            ));
            assert!(!live_correction_backlog_drain_allowed(
                true,
                true,
                false,
                VoiceMode::Command,
                None,
            ));
        }

        #[tokio::test]
        async fn live_correction_apply_waits_for_registered_predecessors() {
            let tracker = Arc::new(LiveCorrectionTracker::new(1));
            assert!(tracker.register_job("seg-1", "first", None));
            assert!(tracker.register_job("seg-2", "second", None));

            let waiting_tracker = Arc::clone(&tracker);
            let mut waiter =
                tokio::spawn(async move { waiting_tracker.wait_until_apply_turn("seg-2").await });
            assert!(tokio::time::timeout(Duration::from_millis(20), &mut waiter)
                .await
                .is_err());

            tracker.complete_job("seg-1", None, None);
            assert!(tokio::time::timeout(Duration::from_millis(200), waiter)
                .await
                .expect("apply waiter should be released")
                .expect("apply waiter task should complete"));
        }

        #[tokio::test]
        async fn live_correction_cancel_releases_order_waiters_without_allowing_apply() {
            let tracker = Arc::new(LiveCorrectionTracker::new(1));
            assert!(tracker.register_job("seg-1", "first", None));
            assert!(tracker.register_job("seg-2", "second", None));

            let waiting_tracker = Arc::clone(&tracker);
            let waiter =
                tokio::spawn(async move { waiting_tracker.wait_until_apply_turn("seg-2").await });
            tracker.cancel();

            assert!(!tokio::time::timeout(Duration::from_millis(200), waiter)
                .await
                .expect("cancelled waiter should be released")
                .expect("cancelled waiter task should complete"));
        }

        #[test]
        fn live_correction_cancel_waits_for_inflight_apply_and_blocks_later_apply() {
            let tracker = Arc::new(LiveCorrectionTracker::new(1));
            assert!(tracker.register_job("seg-1", "first", None));
            let (apply_started_tx, apply_started_rx) = std::sync::mpsc::channel();
            let (release_apply_tx, release_apply_rx) = std::sync::mpsc::channel();

            let apply_tracker = Arc::clone(&tracker);
            let apply_thread = std::thread::spawn(move || {
                let _lease = apply_tracker
                    .acquire_apply_lease("seg-1")
                    .expect("registered live correction should acquire an apply lease");
                apply_started_tx.send(()).expect("announce apply start");
                release_apply_rx.recv().expect("release apply lease");
            });
            apply_started_rx
                .recv_timeout(Duration::from_millis(200))
                .expect("apply lease should be acquired");

            let cancel_tracker = Arc::clone(&tracker);
            let (cancel_started_tx, cancel_started_rx) = std::sync::mpsc::channel();
            let (cancelled_tx, cancelled_rx) = std::sync::mpsc::channel();
            let cancel_thread = std::thread::spawn(move || {
                cancel_started_tx.send(()).expect("announce cancel start");
                cancel_tracker.cancel();
                cancelled_tx.send(()).expect("announce cancellation");
            });
            cancel_started_rx
                .recv_timeout(Duration::from_millis(200))
                .expect("cancel should start while the apply lease is held");
            assert!(cancelled_rx
                .recv_timeout(Duration::from_millis(20))
                .is_err());

            release_apply_tx.send(()).expect("finish in-flight apply");
            apply_thread.join().expect("apply thread should finish");
            cancelled_rx
                .recv_timeout(Duration::from_millis(200))
                .expect("cancel should finish after the apply lease is released");
            cancel_thread.join().expect("cancel thread should finish");
            assert!(tracker.acquire_apply_lease("seg-1").is_none());
        }

        #[test]
        fn configured_or_installed_local_asr_can_run_while_default_model_downloads() {
            assert!(local_asr_bootstrap_allows_packaged_daemon(
                &LocalAsrBootstrapStatus::Downloading,
                true,
                false,
            ));
            assert!(local_asr_bootstrap_allows_packaged_daemon(
                &LocalAsrBootstrapStatus::Downloading,
                false,
                true,
            ));
            assert!(!local_asr_bootstrap_allows_packaged_daemon(
                &LocalAsrBootstrapStatus::Downloading,
                false,
                false,
            ));
            assert!(local_asr_bootstrap_allows_packaged_daemon(
                &LocalAsrBootstrapStatus::FallbackCloud("download failed".to_string()),
                false,
                true,
            ));
            assert!(local_asr_bootstrap_allows_packaged_daemon(
                &LocalAsrBootstrapStatus::Ready,
                false,
                false,
            ));
        }

        #[test]
        fn managed_local_asr_daemon_reuse_requires_the_same_hotword_revision() {
            let base_plan = DesktopLocalAsrDaemonLaunchPlan {
                executable_path: PathBuf::from("C:/Talk/talk-local-asr-sherpa.exe"),
                bind: "127.0.0.1:53171".to_string(),
                args: vec![
                    "--hotwords-file".to_string(),
                    "C:/Talk/hotwords.txt".to_string(),
                ],
                hotwords_content_hash: Some(11),
            };
            let mut updated_plan = base_plan.clone();
            updated_plan.hotwords_content_hash = Some(12);

            assert!(managed_local_asr_daemon_launch_is_current(
                "ws://127.0.0.1:53171/asr",
                &base_plan,
                "ws://127.0.0.1:53171/asr",
                &base_plan,
            ));
            assert!(!managed_local_asr_daemon_launch_is_current(
                "ws://127.0.0.1:53171/asr",
                &base_plan,
                "ws://127.0.0.1:53171/asr",
                &updated_plan,
            ));
        }

        #[test]
        fn product_local_asr_daemon_prewarm_request_exists_for_ready_streaming_product_runtime() {
            let temp_root = std::env::temp_dir().join(format!(
                "talk-product-local-asr-prewarm-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system time after epoch")
                    .as_nanos()
            ));
            fs::create_dir_all(&temp_root).expect("create temp root");
            let worker_path = temp_root.join("talk-local-asr-sherpa.exe");
            fs::write(&worker_path, b"fake exe").expect("write worker marker");
            let model_root = temp_root.join("models").join("sherpa-onnx");
            let model_dir = model_root.join("zipformer-zh-en-punct-int8-480ms");
            fs::create_dir_all(&model_dir).expect("create model dir");
            fs::write(model_dir.join("tokens.txt"), b"tokens").expect("write tokens");
            fs::write(model_dir.join("encoder.int8.onnx"), b"encoder").expect("write encoder");
            fs::write(model_dir.join("decoder.onnx"), b"decoder").expect("write decoder");
            fs::write(model_dir.join("joiner.int8.onnx"), b"joiner").expect("write joiner");

            let config = TalkConfig::from_toml_str(
                r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1
temp_dir = "C:/tmp"

[provider]
kind = "mock"
mock_transcript = "unused"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "C:/logs"

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "disabled"

[speculative.streaming_service]
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000
"#,
            )
            .expect("valid config");

            let request = product_local_asr_daemon_prewarm_request(
                &config,
                &LocalAsrBootstrapStatus::Ready,
                Some(worker_path.as_path()),
                Some(model_root.as_path()),
            )
            .expect("prewarm request should exist");

            assert_eq!(request.endpoint, "ws://127.0.0.1:53171/asr");
            assert_eq!(request.startup_timeout_ms, 15_000);
            assert_eq!(request.plan.bind, "127.0.0.1:53171");
            assert_eq!(request.plan.executable_path, worker_path);
        }

        #[test]
        fn product_local_asr_daemon_prewarm_request_is_none_without_ready_streaming_runtime() {
            let config = TalkConfig::from_toml_str(
                r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1
temp_dir = "C:/tmp"

[provider]
kind = "mock"
mock_transcript = "unused"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "C:/logs"

[speculative]
enabled = true
local_asr = "disabled"
cloud_correction = "disabled"
"#,
            )
            .expect("valid config");

            assert!(product_local_asr_daemon_prewarm_request(
                &config,
                &LocalAsrBootstrapStatus::Downloading,
                None,
                None,
            )
            .is_none());
        }

        #[test]
        fn recording_streaming_unavailable_hint_explains_why_live_text_is_missing() {
            assert_eq!(
                recording_streaming_unavailable_hint(
                    true,
                    false,
                    &LocalAsrBootstrapStatus::Downloading,
                ),
                Some("downloading local ASR model...")
            );
            assert_eq!(
                recording_streaming_unavailable_hint(
                    true,
                    false,
                    &LocalAsrBootstrapStatus::FallbackCloud("download failed".to_string()),
                ),
                Some("live transcript unavailable")
            );
            assert_eq!(
                recording_streaming_unavailable_hint(
                    true,
                    false,
                    &LocalAsrBootstrapStatus::NotStarted,
                ),
                Some("preparing local ASR...")
            );
            assert_eq!(
                recording_streaming_unavailable_hint(
                    true,
                    true,
                    &LocalAsrBootstrapStatus::Downloading,
                ),
                None
            );
        }

        #[test]
        fn recording_streaming_preflight_message_marks_packaged_local_asr_startup() {
            assert_eq!(
                recording_streaming_preflight_message(true),
                Some((
                    "Talk: preparing local ASR",
                    "starting local live transcript runtime..."
                ))
            );
            assert_eq!(recording_streaming_preflight_message(false), None);
        }

        #[test]
        fn paste_shortcut_modifier_release_wait_finishes_immediately_when_no_modifier_is_pressed() {
            let mut samples = vec![PasteShortcutModifierKeyState::default()].into_iter();
            let outcome = wait_for_paste_shortcut_modifier_release_with_sampler(
                Duration::from_millis(50),
                Duration::from_millis(1),
                || samples.next().unwrap_or_default(),
            );

            assert!(outcome.cleared);
            assert_eq!(outcome.poll_count, 1);
            assert_eq!(
                outcome.initial_state,
                PasteShortcutModifierKeyState::default()
            );
            assert_eq!(
                outcome.final_state,
                PasteShortcutModifierKeyState::default()
            );
        }

        #[test]
        fn paste_shortcut_modifier_release_wait_polls_until_modifiers_clear() {
            let mut samples = vec![
                PasteShortcutModifierKeyState {
                    control: true,
                    alt: true,
                    shift: false,
                },
                PasteShortcutModifierKeyState {
                    control: false,
                    alt: true,
                    shift: false,
                },
                PasteShortcutModifierKeyState::default(),
            ]
            .into_iter();
            let outcome = wait_for_paste_shortcut_modifier_release_with_sampler(
                Duration::from_millis(50),
                Duration::from_millis(1),
                || samples.next().unwrap_or_default(),
            );

            assert!(outcome.cleared);
            assert_eq!(outcome.poll_count, 3);
            assert_eq!(
                outcome.initial_state,
                PasteShortcutModifierKeyState {
                    control: true,
                    alt: true,
                    shift: false,
                }
            );
            assert_eq!(
                outcome.final_state,
                PasteShortcutModifierKeyState::default()
            );
        }

        #[test]
        fn paste_shortcut_modifier_release_wait_times_out_when_alt_stays_pressed() {
            let mut samples = vec![
                PasteShortcutModifierKeyState {
                    control: false,
                    alt: true,
                    shift: false,
                },
                PasteShortcutModifierKeyState {
                    control: false,
                    alt: true,
                    shift: false,
                },
                PasteShortcutModifierKeyState {
                    control: false,
                    alt: true,
                    shift: false,
                },
            ]
            .into_iter();
            let outcome = wait_for_paste_shortcut_modifier_release_with_sampler(
                Duration::from_millis(0),
                Duration::from_millis(1),
                || samples.next().unwrap_or_default(),
            );

            assert!(!outcome.cleared);
            assert_eq!(outcome.poll_count, 1);
            assert_eq!(
                outcome.final_state,
                PasteShortcutModifierKeyState {
                    control: false,
                    alt: true,
                    shift: false,
                }
            );
        }

        #[test]
        fn paste_shortcut_modifier_preparation_keeps_fast_clear_path() {
            let mut samples = vec![PasteShortcutModifierKeyState::default()].into_iter();
            let preparation = prepare_paste_shortcut_modifier_state_with_sampler(
                Duration::from_millis(40),
                Duration::from_millis(5),
                || samples.next().unwrap_or_default(),
            );

            assert!(preparation.wait_outcome.cleared);
            assert!(!preparation.force_release);
            assert_eq!(preparation.wait_outcome.poll_count, 1);
        }

        #[test]
        fn paste_shortcut_modifier_preparation_forces_release_after_short_grace_window() {
            let mut samples = vec![
                PasteShortcutModifierKeyState {
                    control: true,
                    alt: true,
                    shift: false,
                },
                PasteShortcutModifierKeyState {
                    control: true,
                    alt: true,
                    shift: false,
                },
            ]
            .into_iter();
            let preparation = prepare_paste_shortcut_modifier_state_with_sampler(
                Duration::from_millis(0),
                Duration::from_millis(5),
                || samples.next().unwrap_or_default(),
            );

            assert!(!preparation.wait_outcome.cleared);
            assert!(preparation.force_release);
            assert_eq!(
                preparation.wait_outcome.final_state,
                PasteShortcutModifierKeyState {
                    control: true,
                    alt: true,
                    shift: false,
                }
            );
        }

        #[test]
        fn direct_control_paste_skips_paste_shortcut_modifier_preparation() {
            assert!(!should_prepare_paste_shortcut_modifiers(true));
        }

        #[test]
        fn keyboard_shortcut_paste_still_prepares_modifier_release_path() {
            assert!(should_prepare_paste_shortcut_modifiers(false));
        }

        #[tokio::test]
        async fn final_correction_provider_wait_uses_timeout_without_live_tracker() {
            let result = tokio::time::timeout(
                Duration::from_millis(200),
                wait_for_speculative_provider(
                    None,
                    Duration::from_millis(10),
                    std::future::pending::<Result<String>>(),
                ),
            )
            .await
            .expect("final correction provider wait should not hang");

            assert!(matches!(result, SpeculativeProviderWait::TimedOut));
        }

        #[tokio::test]
        async fn live_correction_provider_wait_is_preempted_by_cancel() {
            let tracker = Arc::new(LiveCorrectionTracker::new(1));
            assert!(tracker.register_job("seg-1", "first", None));

            let waiting_tracker = Arc::clone(&tracker);
            let waiter = tokio::spawn(async move {
                wait_for_speculative_provider(
                    Some(waiting_tracker.as_ref()),
                    Duration::from_secs(1),
                    std::future::pending::<Result<String>>(),
                )
                .await
            });
            tokio::task::yield_now().await;
            tracker.cancel();

            let result = tokio::time::timeout(Duration::from_millis(200), waiter)
                .await
                .expect("cancelled provider wait should finish")
                .expect("provider wait task should complete");
            assert!(matches!(result, SpeculativeProviderWait::Cancelled));
        }

        #[test]
        fn live_correction_local_fallback_remains_eligible_for_ordered_apply() {
            let tracker = Arc::new(LiveCorrectionTracker::new(1));
            assert!(tracker.register_job("seg-1", "本地文本，", None));

            let job = SpeculativeCloudCorrectionJob {
                config: correction_job_test_config(),
                segment_id: "seg-1".to_string(),
                transcript: "本地文本，".to_string(),
                context_before: None,
                processing_mode: VoiceMode::Transcribe,
                requested_mode: VoiceMode::Transcribe,
                origin_insert_target: None,
                anchor: None,
                full_document_inserted_segments: Vec::new(),
                session_log_path: None,
                latest_live_segment_guard: Some(LatestLiveSegmentGuard { generation: 1 }),
                allow_target_apply: true,
                generation: 1,
                started_at: Instant::now(),
                hwnd_value: 0,
                hud_hwnd_value: 0,
            };

            let fallback = live_correction_local_fallback_output(Some(&tracker), &job)
                .expect("live provider failure should preserve local text");
            assert_eq!(fallback.text, "本地文本，");
            assert!(fallback.faithful_validation.is_none());
            assert!(tracker.can_process("seg-1"));
            assert!(tracker.snapshot()[0].corrected_text.is_none());
        }

        #[tokio::test]
        async fn live_and_final_corrections_share_one_permit_budget() {
            let gate = CorrectionWorkerGate::new(1);
            let first_permit = gate.acquire().await.expect("first correction permit");

            let waiting_gate = gate.clone();
            let mut waiter = tokio::spawn(async move { waiting_gate.acquire().await });
            assert!(tokio::time::timeout(Duration::from_millis(20), &mut waiter)
                .await
                .is_err());

            drop(first_permit);
            assert!(tokio::time::timeout(Duration::from_millis(200), waiter)
                .await
                .expect("second correction should acquire the released permit")
                .expect("permit waiter task should complete")
                .is_some());
        }

        #[test]
        fn foreground_apply_gate_holds_an_owned_lease_across_the_full_transaction() {
            let gate = ForegroundApplyGate::new();
            let first_lease = gate.acquire();
            let (second_started_tx, second_started_rx) = std::sync::mpsc::channel();
            let (second_acquired_tx, second_acquired_rx) = std::sync::mpsc::channel();

            let waiting_gate = gate.clone();
            let waiter = std::thread::spawn(move || {
                second_started_tx.send(()).expect("announce gate wait");
                let _lease = waiting_gate.acquire();
                second_acquired_tx.send(()).expect("announce gate acquire");
            });
            second_started_rx
                .recv_timeout(Duration::from_millis(200))
                .expect("second transaction should start waiting");
            assert!(second_acquired_rx
                .recv_timeout(Duration::from_millis(20))
                .is_err());

            drop(first_lease);
            second_acquired_rx
                .recv_timeout(Duration::from_millis(200))
                .expect("second transaction should acquire after release");
            waiter
                .join()
                .expect("foreground apply waiter should finish");
        }

        fn correction_job_test_config() -> Arc<TalkConfig> {
            Arc::new(TalkConfig {
                trigger: talk_core::TriggerConfig {
                    mode: TriggerMode::Toggle,
                    toggle_shortcut: "RightAlt".to_string(),
                },
                desktop: talk_core::DesktopConfig::default(),
                audio: talk_core::AudioConfig {
                    backend: AudioBackendMode::Silent,
                    input_device: None,
                    max_recording_seconds: 5,
                    sample_rate_hz: 16_000,
                    channels: 1,
                    temp_dir: PathBuf::from(".runtime/talk/audio"),
                },
                provider: talk_core::ProviderConfig {
                    kind: talk_core::ProviderKind::Mock,
                    mock_transcript: Some("test transcript".to_string()),
                    endpoint: None,
                    audio_transcriptions_endpoint: None,
                    chat_completions_endpoint: None,
                    transcription_transport:
                        talk_core::OpenAiTranscriptionTransport::AudioTranscriptions,
                    transcription_model: None,
                    chat_model: None,
                    api_key: None,
                    api_key_env: None,
                },
                output: talk_core::OutputConfig {
                    mode: OutputMode::DryRun,
                    restore_clipboard: true,
                    clipboard_backend: ClipboardBackendMode::Fallback,
                },
                logging: talk_core::LoggingConfig {
                    dir: PathBuf::from(".runtime/talk/logs"),
                },
                speculative: Default::default(),
                voice_mode: VoiceMode::Smart,
            })
        }

        #[test]
        fn live_correction_job_construction_always_uses_transcribe() {
            let config = correction_job_test_config();
            let pipeline = DesktopSpeculativePipelineConfig {
                enabled: true,
                local_asr: "streaming_service".to_string(),
                cloud_correction: "provider_text_processor".to_string(),
            };
            let event = SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id: "seg-1".to_string(),
                local_text: "打开记事本只是示例。".to_string(),
                context_before: String::new(),
            };

            let job = speculative_correction_job_for_live_segment(
                &config,
                &pipeline,
                &event,
                VoiceMode::Smart,
                None,
                None,
                None,
                false,
                7,
                0,
                0,
            )
            .expect("live correction job");

            assert_eq!(job.processing_mode, VoiceMode::Transcribe);
            assert_eq!(job.requested_mode, VoiceMode::Smart);
        }

        #[test]
        fn final_document_job_uses_resolved_mode_and_preserves_unchanged_target_text() {
            let config = correction_job_test_config();
            let target = ForegroundInsertTarget {
                window_handle: 0x707,
                focus_handle: Some(0x808),
                primary_focus_handle: None,
                fallback_focus_handle: None,
                focus_capture_source: None,
            };
            let transcript = "第一段。第二段。第三段。".to_string();
            let inserted_segments = vec![transcript.clone()];
            let session_log_path = PathBuf::from(".runtime/talk/logs/session.json");

            let job = speculative_correction_job_for_final_document(
                &config,
                "final-document",
                transcript.clone(),
                VoiceMode::Smart,
                Some(VoiceMode::Document),
                None,
                target,
                inserted_segments.clone(),
                session_log_path.clone(),
                7,
                0,
                0,
            )
            .expect("final document correction job");

            assert_eq!(job.processing_mode, VoiceMode::Document);
            assert_eq!(job.full_document_inserted_segments, inserted_segments);
            assert_eq!(job.session_log_path, Some(session_log_path));
            assert!(speculative_correction_unchanged_fast_path_allowed(
                &job,
                &transcript
            ));
        }

        #[test]
        fn final_document_job_still_patches_changed_text() {
            let config = correction_job_test_config();
            let target = ForegroundInsertTarget {
                window_handle: 0x707,
                focus_handle: Some(0x808),
                primary_focus_handle: None,
                fallback_focus_handle: None,
                focus_capture_source: None,
            };
            let transcript = "第一段。第二段。第三段。".to_string();
            let inserted_segments = vec![transcript.clone()];

            let job = speculative_correction_job_for_final_document(
                &config,
                "final-document",
                transcript.clone(),
                VoiceMode::Smart,
                Some(VoiceMode::Document),
                None,
                target,
                inserted_segments,
                PathBuf::from(".runtime/talk/logs/session.json"),
                7,
                0,
                0,
            )
            .expect("final document correction job");

            assert!(!speculative_correction_unchanged_fast_path_allowed(
                &job,
                "第一段。第二段。第三段！"
            ));
        }

        #[test]
        fn final_document_job_does_not_skip_when_live_baseline_is_partial() {
            let config = correction_job_test_config();
            let target = ForegroundInsertTarget {
                window_handle: 0x707,
                focus_handle: Some(0x808),
                primary_focus_handle: None,
                fallback_focus_handle: None,
                focus_capture_source: None,
            };
            let transcript = "第一段。第二段。".to_string();

            let job = speculative_correction_job_for_final_document(
                &config,
                "final-document",
                transcript.clone(),
                VoiceMode::Smart,
                Some(VoiceMode::Transcribe),
                None,
                target,
                vec!["第一段。".to_string()],
                PathBuf::from(".runtime/talk/logs/session.json"),
                7,
                0,
                0,
            )
            .expect("final document correction job");

            assert!(!speculative_correction_unchanged_fast_path_allowed(
                &job,
                &transcript
            ));
        }

        #[tokio::test]
        async fn abort_pending_document_correction_cancels_task_and_clears_slot() {
            let task = tokio::spawn(std::future::pending::<()>());
            let mut pending = Some((7, task));

            abort_pending_task(&mut pending);

            assert!(pending.is_none());
        }

        #[test]
        fn model_bootstrap_side_effects_reject_stale_and_shutdown_results() {
            assert!(model_bootstrap_side_effects_allowed(false, 7, 7));
            assert!(!model_bootstrap_side_effects_allowed(false, 8, 7));
            assert!(!model_bootstrap_side_effects_allowed(true, 7, 7));
        }

        #[test]
        fn correction_foreground_side_effects_require_current_generation_and_live_window() {
            assert!(correction_foreground_side_effects_allowed(false, 8, 7));
            assert!(!correction_foreground_side_effects_allowed(true, 8, 7));
            assert!(!correction_foreground_side_effects_allowed(false, 9, 7));
        }

        #[test]
        fn stop_worker_side_effects_require_current_generation_and_live_window() {
            assert!(stop_worker_side_effects_allowed(false, Some(7), 8, 7));
            assert!(!stop_worker_side_effects_allowed(true, Some(7), 8, 7));
            assert!(!stop_worker_side_effects_allowed(false, None, 8, 7));
            assert!(!stop_worker_side_effects_allowed(false, Some(8), 9, 7));
        }

        #[test]
        fn copy_popup_tab_navigation_cycles_editor_copy_and_close_in_both_directions() {
            assert_eq!(
                copy_popup_keyboard_tab_target(CopyPopupHoveredControl::None, false),
                CopyPopupHoveredControl::Copy
            );
            assert_eq!(
                copy_popup_keyboard_tab_target(CopyPopupHoveredControl::Copy, false),
                CopyPopupHoveredControl::Close
            );
            assert_eq!(
                copy_popup_keyboard_tab_target(CopyPopupHoveredControl::Close, false),
                CopyPopupHoveredControl::None
            );
            assert_eq!(
                copy_popup_keyboard_tab_target(CopyPopupHoveredControl::None, true),
                CopyPopupHoveredControl::Close
            );
            assert_eq!(
                copy_popup_keyboard_tab_target(CopyPopupHoveredControl::Close, true),
                CopyPopupHoveredControl::Copy
            );
            assert_eq!(
                copy_popup_keyboard_tab_target(CopyPopupHoveredControl::Copy, true),
                CopyPopupHoveredControl::None
            );
        }

        struct ShortcutHelpTestWindow(HWND);

        impl Drop for ShortcutHelpTestWindow {
            fn drop(&mut self) {
                if let Ok(mut overlay) = overlay_ui_state().lock() {
                    overlay.shortcut_help = None;
                }
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }

        #[test]
        fn shortcut_help_real_window_show_paint_and_hide_path_is_operational() {
            static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
            let _serial = lock_recovering(
                TEST_LOCK.get_or_init(|| Mutex::new(())),
                "shortcut help test lock",
            );
            let instance = unsafe { GetModuleHandleW(ptr::null()) };
            register_named_window_class(
                instance,
                SHORTCUT_HELP_WINDOW_CLASS_NAME,
                shortcut_help_window_proc,
            )
            .expect("register shortcut help window class");
            let window = ShortcutHelpTestWindow(
                create_shortcut_help_window(instance, ptr::null_mut())
                    .expect("create shortcut help window"),
            );
            let model = DesktopShortcutHelpModel {
                title: "RightAlt".to_string(),
                detail: String::new(),
                entries: vec![
                    DesktopShortcutHelpEntry {
                        title: "输入".to_string(),
                        shortcut: "松开".to_string(),
                        detail: String::new(),
                    },
                    DesktopShortcutHelpEntry {
                        title: "翻译".to_string(),
                        shortcut: "/".to_string(),
                        detail: String::new(),
                    },
                    DesktopShortcutHelpEntry {
                        title: "提问".to_string(),
                        shortcut: "Space".to_string(),
                        detail: String::new(),
                    },
                ],
            };

            assert_eq!(unsafe { IsWindowVisible(window.0) }, 0);
            show_shortcut_help_window(window.0, model.clone()).expect("show shortcut help window");
            assert_ne!(unsafe { IsWindowVisible(window.0) }, 0);

            let mut window_rect = RECT::default();
            assert_ne!(unsafe { GetWindowRect(window.0, &mut window_rect) }, 0);
            let expected_metrics = scale_shortcut_help_metrics_for_dpi(
                desktop_shortcut_help_metrics_for_entry_count(model.entries.len()),
                overlay_dpi_for_window(window.0),
            );
            assert_eq!(window_rect.right - window_rect.left, expected_metrics.width);
            assert_eq!(
                window_rect.bottom - window_rect.top,
                expected_metrics.height
            );
            assert_eq!(
                lock_recovering(overlay_ui_state(), "shortcut help overlay state").shortcut_help,
                Some(model)
            );

            unsafe {
                SendMessageW(window.0, WM_PAINT, 0, 0);
            }
            let mut client_rect = RECT::default();
            assert_ne!(unsafe { GetClientRect(window.0, &mut client_rect) }, 0);
            let hdc = unsafe { GetDC(window.0) };
            assert!(!hdc.is_null());
            let painted_pixel = unsafe {
                GetPixel(
                    hdc,
                    (client_rect.right - client_rect.left) / 2,
                    scale_desktop_overlay_length(10, overlay_dpi_for_window(window.0)),
                )
            };
            unsafe {
                ReleaseDC(window.0, hdc);
            }
            assert_ne!(painted_pixel, u32::MAX);

            hide_shortcut_help_window(window.0).expect("hide shortcut help window");
            assert_eq!(unsafe { IsWindowVisible(window.0) }, 0);
            assert!(
                lock_recovering(overlay_ui_state(), "shortcut help overlay state")
                    .shortcut_help
                    .is_none()
            );
        }
    }
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_app::run() {
        eprintln!("talk-desktop failed: {error:#}");
        std::process::exit(1);
    }
}
