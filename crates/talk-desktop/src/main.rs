#[cfg(not(windows))]
fn main() {
    eprintln!("talk-desktop is only available on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
mod windows_app {
    use anyhow::{Context, Result};
    use clap::Parser;
    use serde_json::Value;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::fs;
    use std::mem;
    use std::net::{SocketAddr, TcpStream};
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::ptr;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};
    use talk_audio::{
        probe_native_windows_audio_readiness_for_device, start_recording, AudioCaptureRequest,
        RecordingSession, WavSettings,
    };
    use talk_client::{
        final_transcript_from_streaming_asr_events, FrontContext, StreamingAsrEvent,
    };
    use talk_core::{
        AudioBackendMode, ClipboardBackendMode, DesktopPasteShortcut, OutputMode, TalkConfig,
        TriggerMode, VoiceEvent, VoiceMode, VoiceSession,
    };
    use talk_desktop::{
        default_zipformer_model_spec, download_and_install_model,
        build_desktop_insert_target_diagnostic_with_trace,
        build_desktop_insert_target_trace_diagnostic, build_status_report, compose_hud_message,
        config_status_message, decide_speculative_patch_application, desktop_action_binding_label,
        desktop_action_bindings, desktop_copy_popup_action_for_virtual_key,
        desktop_copy_popup_activation_policy,
        desktop_copy_popup_close_button_rect as popup_close_button_layout_rect,
        desktop_copy_popup_copy_button_rect as popup_copy_button_layout_rect,
        desktop_copy_popup_copy_shows_follow_up_hud, desktop_copy_popup_editor_content_rect,
        desktop_copy_popup_editor_frame_rect as popup_editor_frame_layout_rect,
        desktop_copy_popup_metrics, desktop_copy_popup_model,
        desktop_copy_popup_model_for_mode_text_result, desktop_copy_popup_pane_layouts,
        desktop_copy_popup_position, desktop_document_recorrection_session_decision,
        desktop_hud_activation_policy, desktop_hud_metrics_for_view_model,
        desktop_hud_presentation_for_phase, desktop_hud_thinking_palette,
        desktop_hud_thinking_progress_model, desktop_hud_thinking_text_wave_offsets,
        desktop_hud_view_model_for_listening_waveform_with_partial,
        desktop_hud_view_model_for_phase, desktop_insert_target_restore_requested,
        desktop_listening_hud_action_for_point, desktop_listening_hud_cancel_button_rect,
        desktop_listening_hud_complete_button_rect, desktop_listening_hud_partial_text_layout,
        desktop_listening_hud_visible_partial_text, desktop_listening_hud_waveform_rect,
        desktop_local_asr_daemon_bind_from_endpoint, desktop_mode_dropdown_model,
        desktop_mode_text_result_model, desktop_output_plan, desktop_overlay_scale_factor_for_dpi,
        desktop_effective_streaming_asr_enabled,
        desktop_packaged_local_asr_daemon_launch_plan_with_config,
        desktop_product_local_asr_daemon_launch_plan_with_config,
        desktop_preferred_paste_shortcut_for_target, desktop_runtime_insert_directive_for_mode,
        desktop_shortcut_help_activation_policy, desktop_shortcut_help_metrics,
        desktop_shortcut_help_model, desktop_shortcut_help_position,
        desktop_speculative_cloud_correction_enabled, desktop_speculative_correction_job_model,
        desktop_speculative_local_asr_route, desktop_speculative_replacement_selection_count,
        desktop_streaming_hud_transcript, desktop_streaming_latest_segment_allows_auto_patch,
        desktop_streaming_stop_policy, desktop_streaming_stop_tail_text,
        foreground_target_refresh_requested, foreground_target_stability_satisfied,
        hotkey_status_message, hud_message_for_phase, hydrate_foreground_insert_target_focus,
        idle_status_detail, live_streaming_local_segment_plan, native_status_message,
        observe_foreground_target_stability, parse_desktop_window_handle,
        extract_embedded_runtime_payload, recording_stop_watcher_policy,
        resolve_default_desktop_config_path, resolve_talk_data_root, validate_installed_model,
        resolve_desktop_audio_file_override, resolve_foreground_focus_capture,
        resolve_hotkey_origin_insert_target, resolve_hotkey_recording_origin_enrichment,
        resolve_pending_hotkey_origin_capture, scale_desktop_overlay_length,
        select_foreground_insert_target, select_windows_hotkey_binding_strategy, tray_menu_model,
        windows_hotkey_binding_registration_plan, write_desktop_insert_target_diagnostic,
        ConfigAvailability, DesktopActionBinding, DesktopActionRoute, DesktopCopyPopupAction,
        DesktopCopyPopupMetrics, DesktopCopyPopupModel, DesktopCopyPopupPaneModel,
        DesktopDocumentRecorrectionDecision, DesktopHudMetrics, DesktopHudPresentation,
        DesktopHudViewModel, DesktopHudVisualState, DesktopInsertTargetContext,
        DesktopInsertTargetRestoreDiagnostic, DesktopListeningHudAction,
        DesktopLiveStreamingLocalSegmentPlan, DesktopOutputStrategy,
        DesktopOverlayActivationPolicy, DesktopRecordingStopWatcherPolicy,
        DesktopRuntimeInsertDirective, DesktopShortcutHelpMetrics, DesktopShortcutHelpModel,
        DesktopSpeculativeCorrectionOutputTarget, DesktopSpeculativeLocalAsrRoute,
        DesktopSpeculativePipelineConfig, DesktopTextLifecycleState, ForegroundInsertTarget,
        ForegroundTargetReleaseReason, ForegroundTargetStabilityProgress, HotkeyBindingState,
        HotkeySpec, LastSessionStatus, LowLevelHotkeyTracker, LowLevelHotkeyTransition,
        NativeBackendSnapshot, NativeReadinessSnapshot, ShellState, SpeculativeInsertAnchor,
        SpeculativePatchApplication, SpeculativePatchCandidate, StatusSnapshot,
        ToggleDesktopHotkeyRouter, ToggleDesktopHotkeyRouterPendingHold,
        WindowsHotkeyBindingRegistrationPlan, WindowsHotkeyBindingStrategy,
        TALK_PACKAGED_LOCAL_ASR_DAEMON_EXE_NAME,
        TALK_DESKTOP_AUDIO_FILE_OVERRIDE_ENV, TALK_DESKTOP_INSERT_TARGET_FOCUS_ENV,
        TALK_DESKTOP_INSERT_TARGET_WINDOW_ENV,
    };
    use talk_insert::{
        probe_native_windows_clipboard_readiness, ClipboardBackend, ClipboardPasteInserter,
        ClipboardRestorePolicy, TextInserter, WindowsClipboardBackend, WindowsPasteShortcut,
        TALK_WINDOWS_PASTE_SHORTCUT_ENV,
    };
    use talk_runtime::{
        complete_cancelled_session, complete_failed_session, load_effective_config,
        process_voice_transcript_text, run_local_streaming_asr_service_from_recording,
        run_mock_speculative_session, run_voice_session_from_audio_artifact_with_insert_hooks,
        run_voice_session_from_external_asr_command_with_insert_hooks,
        run_voice_session_from_local_transcript_with_insert_hooks,
        run_voice_session_from_transcript_with_insert_hooks, runtime_voice_text_result,
        LocalStreamingAsrLiveSession, RuntimeInsertDirective, RuntimePhase, SegmenterConfig,
        SpeculativeRuntimeEvent, SpeculativeRuntimeState,
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
        BeginPaint, CreateFontW, CreatePen, CreateRectRgn, CreateRoundRectRgn, CreateSolidBrush,
        DeleteObject, DrawTextW, Ellipse, EndPaint, GetDC, GetStockObject, GetTextExtentPoint32W,
        GradientFill, InvalidateRect, LineTo, MoveToEx, ReleaseDC, RoundRect, SelectObject,
        SetBkColor, SetBkMode, SetTextColor, SetWindowRgn, TextOutW, CLEARTYPE_QUALITY,
        CLIP_DEFAULT_PRECIS, COLOR_WINDOW, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH,
        DT_CALCRECT, DT_CENTER, DT_EDITCONTROL, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
        DT_WORDBREAK, FF_DONTCARE, FW_BOLD, GRADIENT_FILL_RECT_H, GRADIENT_FILL_RECT_V,
        GRADIENT_RECT, HOLLOW_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
        TRIVERTEX,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::HiDpi::{
        GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetFocus, RegisterHotKey, SendInput, SetActiveWindow, SetFocus, UnregisterHotKey, INPUT,
        INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LEFT, VK_SHIFT,
    };
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, BringWindowToTop, CallNextHookEx, CreatePopupMenu, CreateWindowExW,
        DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
        GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetSystemMetrics, GetWindowLongPtrW,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindow, KillTimer,
        LoadCursorW, LoadIconW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassW,
        SendMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos,
        SetWindowTextW, SetWindowsHookExW, ShowWindow, TrackPopupMenu, TranslateMessage,
        UnhookWindowsHookEx, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, EN_CHANGE,
        ES_AUTOVSCROLL, ES_CENTER, ES_MULTILINE, GUITHREADINFO, GWLP_USERDATA, HC_ACTION, HHOOK,
        IDC_ARROW, IDI_APPLICATION, KBDLLHOOKSTRUCT, MB_ICONINFORMATION, MB_OK, MF_CHECKED,
        MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNOACTIVATE,
        TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WH_KEYBOARD_LL, WM_APP, WM_COMMAND,
        WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_DESTROY, WM_ERASEBKGND, WM_GETFONT, WM_HOTKEY,
        WM_KEYDOWN, WM_KEYUP, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
        WM_RBUTTONUP, WM_SETFONT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WNDCLASSW, WS_CHILD,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP,
        WS_TABSTOP, WS_VISIBLE,
    };

    const WINDOW_CLASS_NAME: &str = "TalkDesktopMessageWindow";
    const HUD_WINDOW_CLASS_NAME: &str = "TalkDesktopHudWindow";
    const COPY_POPUP_WINDOW_CLASS_NAME: &str = "TalkDesktopCopyPopupWindow";
    const SHORTCUT_HELP_WINDOW_CLASS_NAME: &str = "TalkDesktopShortcutHelpWindow";
    const COPY_POPUP_EDIT_CLASS_NAME: &str = "EDIT";
    const HOTKEY_ID: i32 = 1;
    const COPY_POPUP_EDIT_CONTROL_ID: isize = 2001;
    const COPY_POPUP_MAX_PANES: usize = 4;
    const EM_SETREADONLY_MESSAGE: u32 = 0x00CF;
    const VK_CONTROL_KEY: u16 = 0x11;
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
    const TIMER_HIDE_HUD: usize = 1;
    const TIMER_SHORTCUT_HELP_HOLD: usize = 2;
    const TIMER_RECORDING_LEVEL: usize = 3;
    const TIMER_THINKING_PROGRESS: usize = 4;
    const HUD_RECORDING_LEVEL_REFRESH_MS: u32 = 48;
    const HUD_THINKING_PROGRESS_REFRESH_MS: u32 = 72;
    const COPY_POPUP_CORNER_RADIUS: i32 = 0;
    const SHORTCUT_HELP_CORNER_RADIUS: i32 = 0;
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
            streaming_session: Option<LocalStreamingAsrLiveSession>,
        },
        ExplicitAudioFile(PathBuf),
    }

    enum StoppedRecordingSource {
        AudioFile(PathBuf),
        StreamingRecording {
            recording: RecordingSession,
            streaming_session: Option<LocalStreamingAsrLiveSession>,
        },
    }

    struct ActiveRecording {
        action_index: usize,
        mode_override: Option<VoiceMode>,
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
        hud_streaming_segments: Vec<(String, String)>,
        last_streaming_asr_event: Option<StreamingAsrEvent>,
        last_streaming_asr_event_at: Option<Instant>,
    }

    struct SharedState {
        config: Option<TalkConfig>,
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
        worker_generation: Option<u64>,
        pending_worker_error: Option<(u64, String)>,
        pending_copy_popup: Option<PendingCopyPopup>,
        pending_hotkey_origin_insert_target: Option<DesktopInsertTargetContext>,
        local_asr_daemon: Option<ManagedLocalAsrDaemon>,
        local_asr_bootstrap_status: LocalAsrBootstrapStatus,
        product_runtime_worker: Option<PathBuf>,
        product_model_root: Option<PathBuf>,
        runtime_handle: tokio::runtime::Handle,
        next_generation: u64,
    }

    struct ManagedLocalAsrDaemon {
        endpoint: String,
        child: std::process::Child,
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

    struct SpeculativeCloudCorrectionJob {
        config: TalkConfig,
        transcript: String,
        context_before: Option<String>,
        mode_override: Option<VoiceMode>,
        anchor: SpeculativeInsertAnchor,
        full_document_inserted_segments: Vec<String>,
        latest_live_segment_guard: Option<LatestLiveSegmentGuard>,
        generation: u64,
        started_at: Instant,
        hwnd_value: usize,
        hud_hwnd_value: usize,
    }

    #[derive(Debug, Clone, Copy)]
    struct LatestLiveSegmentGuard {
        generation: u64,
    }

    struct PendingLiveStreamingDispatch {
        config: TalkConfig,
        pipeline_config: DesktopSpeculativePipelineConfig,
        runtime_handle: tokio::runtime::Handle,
        mode_override: Option<VoiceMode>,
        generation: u64,
        origin_insert_target: Option<DesktopInsertTargetContext>,
        existing_anchors: HashMap<String, SpeculativeInsertAnchor>,
        events: Vec<SpeculativeRuntimeEvent>,
        hwnd_value: usize,
        hud_hwnd_value: usize,
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
        hud_hwnd: HWND,
        copy_popup_hwnd: HWND,
        copy_popup_edit_hwnd: HWND,
        copy_popup_pane_edit_hwnds: Vec<HWND>,
        shortcut_help_hwnd: HWND,
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
        let mut state = low_level_hook_state()
            .lock()
            .expect("Talk desktop low-level hook state");
        let LowLevelHookState::ToggleRouter { hwnd_value, router } = state.as_mut()? else {
            return None;
        };
        Some(f(router, *hwnd_value as HWND))
    }

    #[derive(Debug, Clone)]
    struct CopyPopupRenderState {
        model: DesktopCopyPopupModel,
        hovered_control: CopyPopupHoveredControl,
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
        hud_meter_bins: [f32; 9],
        hud_streaming_partial_text: Option<String>,
        hud_thinking_pulse_tick: u32,
        copy_popup: Option<CopyPopupRenderState>,
        shortcut_help: Option<DesktopShortcutHelpModel>,
    }

    fn overlay_ui_state() -> &'static Mutex<OverlayUiState> {
        static STATE: OnceLock<Mutex<OverlayUiState>> = OnceLock::new();
        STATE.get_or_init(|| Mutex::new(OverlayUiState::default()))
    }

    fn upsert_hud_streaming_segment(
        segments: &mut Vec<(String, String)>,
        segment_id: &str,
        text: &str,
    ) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        if let Some((_, existing_text)) = segments
            .iter_mut()
            .find(|(existing_segment_id, _)| existing_segment_id == segment_id)
        {
            existing_text.clear();
            existing_text.push_str(text);
        } else {
            segments.push((segment_id.to_string(), text.to_string()));
        }
    }

    fn hud_streaming_transcript_from_segments(segments: &[(String, String)]) -> Option<String> {
        let borrowed_segments = segments
            .iter()
            .map(|(segment_id, text)| (segment_id.as_str(), text.as_str()))
            .collect::<Vec<_>>();
        let transcript = desktop_streaming_hud_transcript(&borrowed_segments, None);

        (!transcript.is_empty()).then_some(transcript)
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
        let (config, config_status, hotkey, desktop_actions, native_readiness) =
            load_desktop_startup_state(runtime.handle(), &config_path);
        let selected_voice_mode = config
            .as_ref()
            .map(TalkConfig::default_voice_mode)
            .unwrap_or(VoiceMode::Smart);

        let shared = Arc::new(Mutex::new(SharedState {
            config,
            config_status,
            config_path,
            hotkey,
            desktop_actions,
            selected_voice_mode,
            native_readiness,
            shell_state: ShellState::idle(),
            current_phase: None,
            last_session: None,
            active_recording: None,
            worker_generation: None,
            pending_worker_error: None,
            pending_copy_popup: None,
            pending_hotkey_origin_insert_target: None,
            local_asr_daemon: None,
            local_asr_bootstrap_status: LocalAsrBootstrapStatus::NotStarted,
            product_runtime_worker: None,
            product_model_root: None,
            runtime_handle: runtime.handle().clone(),
            next_generation: 1,
        }));

        let window = Box::new(WindowState {
            shared: Arc::clone(&shared),
            hud_hwnd: ptr::null_mut(),
            copy_popup_hwnd: ptr::null_mut(),
            copy_popup_edit_hwnd: ptr::null_mut(),
            copy_popup_pane_edit_hwnds: Vec::new(),
            shortcut_help_hwnd: ptr::null_mut(),
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
        start_product_bootstrap(hwnd, Arc::clone(&shared));

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
            hbrBackground: (COLOR_WINDOW as isize + 1) as _,
            ..unsafe { mem::zeroed() }
        };
        let atom = unsafe { RegisterClassW(&class) };
        if atom == 0 {
            anyhow::bail!("register Talk desktop window class '{class_name}'");
        }
        Ok(())
    }

    fn initialize_window(hwnd: HWND, instance: HINSTANCE) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
        state.hud_hwnd = create_hud_window(instance, hwnd)?;
        state.copy_popup_hwnd = create_copy_popup_window(instance, hwnd)?;
        state.shortcut_help_hwnd = create_shortcut_help_window(instance, hwnd)?;

        let (startup_message, tray_status) = {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
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
        if let Ok(state) = unsafe { get_window_state_mut(owner_hwnd) } {
            state.copy_popup_edit_hwnd = edit_hwnd;
            state.copy_popup_pane_edit_hwnds = vec![edit_hwnd];
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
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | (ES_CENTER as u32)
                    | (ES_MULTILINE as u32)
                    | (ES_AUTOVSCROLL as u32),
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

    fn start_product_bootstrap(hwnd: HWND, shared: Arc<Mutex<SharedState>>) {
        let configured_for_streaming = {
            let shared_state = shared.lock().expect("Talk desktop shared state");
            shared_state
                .config
                .as_ref()
                .map(|config| {
                    desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(config))
                        == DesktopSpeculativeLocalAsrRoute::StreamingService
                })
                .unwrap_or(false)
        };
        if !configured_for_streaming {
            return;
        }

        let data_root = match resolve_talk_data_root() {
            Ok(root) => root,
            Err(error) => {
                set_local_asr_bootstrap_status(
                    &shared,
                    LocalAsrBootstrapStatus::FallbackCloud(error),
                );
                return;
            }
        };
        let model_root = data_root.join("models").join("sherpa-onnx");
        let executable_path = match std::env::current_exe() {
            Ok(path) => path,
            Err(error) => {
                set_local_asr_bootstrap_status(
                    &shared,
                    LocalAsrBootstrapStatus::FallbackCloud(format!(
                        "resolve Talk executable for local ASR payload: {error}"
                    )),
                );
                return;
            }
        };
        let executable_bytes = match fs::read(&executable_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                set_local_asr_bootstrap_status(
                    &shared,
                    LocalAsrBootstrapStatus::FallbackCloud(format!(
                        "read Talk executable for local ASR payload: {error}"
                    )),
                );
                return;
            }
        };
        let has_embedded_payload = executable_bytes
            .windows(b"TLPAY001".len())
            .any(|window| window == b"TLPAY001");
        if !has_embedded_payload {
            set_local_asr_bootstrap_status(&shared, LocalAsrBootstrapStatus::EngineeringFallback);
            return;
        }
        let runtime_root = data_root.join("runtime");
        let worker_path = match extract_embedded_runtime_payload(&executable_bytes, &runtime_root) {
            Ok(runtime_dir) => runtime_dir.join(TALK_PACKAGED_LOCAL_ASR_DAEMON_EXE_NAME),
            Err(error) => {
                set_local_asr_bootstrap_status(
                    &shared,
                    LocalAsrBootstrapStatus::FallbackCloud(format!(
                        "verify embedded Talk local ASR runtime: {error}"
                    )),
                );
                return;
            }
        };
        {
            let mut shared_state = shared.lock().expect("Talk desktop shared state");
            shared_state.product_runtime_worker = Some(worker_path);
            shared_state.product_model_root = Some(model_root.clone());
        }

        let spec = default_zipformer_model_spec();
        let model_dir = model_root.join(&spec.id);
        if validate_installed_model(&spec, &model_dir).is_ok() {
            set_local_asr_bootstrap_status(&shared, LocalAsrBootstrapStatus::Ready);
            return;
        }

        set_local_asr_bootstrap_status(&shared, LocalAsrBootstrapStatus::Downloading);
        let runtime_handle = {
            let shared_state = shared.lock().expect("Talk desktop shared state");
            shared_state.runtime_handle.clone()
        };
        let shared_for_task = Arc::clone(&shared);
        let hwnd_value = hwnd as usize;
        runtime_handle.spawn(async move {
            let status = match download_and_install_model(&spec, &model_root).await {
                Ok(_) => LocalAsrBootstrapStatus::Ready,
                Err(error) => LocalAsrBootstrapStatus::FallbackCloud(format!(
                    "download local ASR model: {error}"
                )),
            };
            set_local_asr_bootstrap_status(&shared_for_task, status);
            unsafe {
                let _ = PostMessageW(hwnd_value as HWND, MODEL_BOOTSTRAP_MESSAGE, 0, 0);
            }
        });
        let _ = update_tray_icon(hwnd, "Talk: downloading local ASR model");
    }

    fn set_local_asr_bootstrap_status(
        shared: &Arc<Mutex<SharedState>>,
        status: LocalAsrBootstrapStatus,
    ) {
        let mut shared_state = shared.lock().expect("Talk desktop shared state");
        shared_state.local_asr_bootstrap_status = status;
    }

    fn handle_model_bootstrap_status(hwnd: HWND) {
        let status = unsafe { get_window_state_mut(hwnd) }
            .ok()
            .and_then(|state| state.shared.lock().ok().map(|shared| shared.local_asr_bootstrap_status.clone()));
        match status {
            Some(LocalAsrBootstrapStatus::Ready) => {
                let _ = update_tray_icon(hwnd, "Talk: local ASR ready");
                let _ = show_hud_text(hwnd, "Talk: local ASR ready", Some(1200));
            }
            Some(LocalAsrBootstrapStatus::FallbackCloud(reason)) => {
                let _ = update_tray_icon(hwnd, "Talk: cloud ASR fallback");
                let _ = show_hud_text(hwnd, &format!("Talk: cloud ASR fallback\n{reason}"), Some(2400));
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

    fn register_low_level_origin_capture(hwnd: HWND, hotkey: HotkeySpec) -> Result<()> {
        unregister_low_level_hotkey();

        {
            let mut state = low_level_hook_state()
                .lock()
                .expect("Talk desktop low-level hook state");
            *state = Some(LowLevelHookState::OriginCapture {
                hwnd_value: hwnd as isize,
                tracker: LowLevelHotkeyTracker::new(hotkey),
            });
        }

        let module = unsafe { GetModuleHandleW(ptr::null()) };
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0) };
        if hook.is_null() {
            let mut state = low_level_hook_state()
                .lock()
                .expect("Talk desktop low-level hook state");
            *state = None;
            anyhow::bail!("register Talk desktop origin capture hook failed");
        }

        let mut handle = low_level_hook_handle()
            .lock()
            .expect("Talk desktop low-level hook handle");
        *handle = Some(hook as isize);
        Ok(())
    }

    fn register_low_level_hotkey(
        hwnd: HWND,
        trigger_mode: TriggerMode,
        hotkey: HotkeySpec,
    ) -> Result<()> {
        unregister_low_level_hotkey();

        {
            let mut state = low_level_hook_state()
                .lock()
                .expect("Talk desktop low-level hook state");
            *state = Some(LowLevelHookState::Single {
                hwnd_value: hwnd as isize,
                trigger_mode,
                tracker: LowLevelHotkeyTracker::new(hotkey),
            });
        }

        let module = unsafe { GetModuleHandleW(ptr::null()) };
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0) };
        if hook.is_null() {
            let mut state = low_level_hook_state()
                .lock()
                .expect("Talk desktop low-level hook state");
            *state = None;
            anyhow::bail!("register Talk desktop low-level keyboard hook failed");
        }

        let mut handle = low_level_hook_handle()
            .lock()
            .expect("Talk desktop low-level hook handle");
        *handle = Some(hook as isize);
        Ok(())
    }

    fn register_low_level_action_router(
        hwnd: HWND,
        bindings: &[DesktopActionBinding],
    ) -> Result<()> {
        unregister_low_level_hotkey();

        {
            let mut state = low_level_hook_state()
                .lock()
                .expect("Talk desktop low-level hook state");
            *state = Some(LowLevelHookState::ToggleRouter {
                hwnd_value: hwnd as isize,
                router: ToggleDesktopHotkeyRouter::new(bindings),
            });
        }

        let module = unsafe { GetModuleHandleW(ptr::null()) };
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0) };
        if hook.is_null() {
            let mut state = low_level_hook_state()
                .lock()
                .expect("Talk desktop low-level hook state");
            *state = None;
            anyhow::bail!("register Talk desktop low-level keyboard hook failed");
        }

        let mut handle = low_level_hook_handle()
            .lock()
            .expect("Talk desktop low-level hook handle");
        *handle = Some(hook as isize);
        Ok(())
    }

    fn unregister_low_level_hotkey() {
        if let Some(hook) = low_level_hook_handle()
            .lock()
            .expect("Talk desktop low-level hook handle")
            .take()
        {
            unsafe {
                let _ = UnhookWindowsHookEx(hook as HHOOK);
            }
        }

        let mut state = low_level_hook_state()
            .lock()
            .expect("Talk desktop low-level hook state");
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

    fn load_desktop_startup_state(
        runtime_handle: &tokio::runtime::Handle,
        config_path: &Path,
    ) -> (
        Option<TalkConfig>,
        ConfigAvailability,
        HotkeyBindingState,
        Vec<DesktopActionBinding>,
        Option<NativeReadinessSnapshot>,
    ) {
        match runtime_handle.block_on(load_effective_config(config_path)) {
            Ok(config) => match initial_hotkey_binding(&config) {
                Ok((desktop_actions, hotkey)) => (
                    Some(config.clone()),
                    ConfigAvailability::ready(),
                    hotkey,
                    desktop_actions,
                    Some(configured_native_readiness(&config)),
                ),
                Err(error) => (
                    Some(config.clone()),
                    ConfigAvailability::ready(),
                    HotkeyBindingState::invalid_config(
                        desktop_shortcut_label_from_config(&config),
                        error,
                    ),
                    Vec::new(),
                    Some(configured_native_readiness(&config)),
                ),
            },
            Err(error) => (
                None,
                ConfigAvailability::unavailable(error.to_string()),
                HotkeyBindingState::Unconfigured,
                Vec::new(),
                None,
            ),
        }
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
            native_readiness: shared.native_readiness.clone(),
        }
    }

    fn refresh_idle_tray_status(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
        let shared = state.shared.lock().expect("Talk desktop shared state");
        update_tray_icon(hwnd, current_idle_status(&shared))
    }

    fn cancel_active_recording(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
        let (config, runtime_handle, active) = {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
            let Some(active) = shared.active_recording.take() else {
                return Ok(());
            };
            let Some(config) = shared.config.clone() else {
                shared.shell_state = shared.shell_state.complete();
                shared.current_phase = None;
                return Ok(());
            };
            shared.shell_state = shared.shell_state.complete();
            shared.current_phase = None;
            (config, shared.runtime_handle.clone(), active)
        };

        let ActiveRecording {
            session,
            trigger_events,
            source,
            ..
        } = active;
        if let ActiveRecordingSource::Live {
            recording,
            streaming_session,
        } = source
        {
            if let Some(streaming_session) = streaming_session {
                runtime_handle.spawn(async move {
                    if let Err(error) = streaming_session.cancel().await {
                        eprintln!("Talk local streaming ASR cancel failed: {error:#}");
                    }
                });
            }
            recording.cancel()?;
        }
        let _ = complete_cancelled_session(&config, session, trigger_events, |_| {})?;
        {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
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

    unsafe fn get_window_state_mut(hwnd: HWND) -> Result<&'static mut WindowState> {
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if pointer.is_null() {
            anyhow::bail!("Talk desktop window state is unavailable");
        }
        Ok(&mut *pointer)
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
                handle_model_bootstrap_status(hwnd);
                0
            }
            CORRECTION_COPY_POPUP_MESSAGE => {
                handle_correction_copy_popup(hwnd, wparam as u64);
                0
            }
            WM_DESTROY => {
                unsafe {
                    KillTimer(hwnd, TIMER_SHORTCUT_HELP_HOLD);
                    KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                    KillTimer(hwnd, TIMER_THINKING_PROGRESS);
                }
                if let Ok(state) = get_window_state_mut(hwnd) {
                    if let Ok(mut shared) = state.shared.lock() {
                        if let Some(active) = shared.active_recording.take() {
                            let ActiveRecording {
                                session,
                                trigger_events,
                                source,
                                ..
                            } = active;
                            if let ActiveRecordingSource::Live {
                                recording,
                                streaming_session,
                            } = source
                            {
                                if let Some(streaming_session) = streaming_session {
                                    let runtime_handle = shared.runtime_handle.clone();
                                    runtime_handle.spawn(async move {
                                        if let Err(error) = streaming_session.cancel().await {
                                            eprintln!(
                                                "Talk local streaming ASR cancel failed: {error:#}"
                                            );
                                        }
                                    });
                                }
                                let _ = recording.cancel();
                            }
                            if let Some(config) = shared.config.as_ref() {
                                let _ = complete_cancelled_session(
                                    config,
                                    session,
                                    trigger_events,
                                    |_| {},
                                );
                            }
                        }
                        if let Some(daemon) = shared.local_asr_daemon.take() {
                            stop_managed_local_asr_daemon(daemon);
                        }
                        unregister_bound_hotkey(hwnd);
                    }
                    if !state.hud_hwnd.is_null() {
                        let _ = DestroyWindow(state.hud_hwnd);
                    }
                    if !state.copy_popup_hwnd.is_null() {
                        let _ = DestroyWindow(state.copy_popup_hwnd);
                        state.copy_popup_hwnd = ptr::null_mut();
                        state.copy_popup_edit_hwnd = ptr::null_mut();
                        state.copy_popup_pane_edit_hwnds.clear();
                    }
                    if !state.shortcut_help_hwnd.is_null() {
                        let _ = DestroyWindow(state.shortcut_help_hwnd);
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
            WM_LBUTTONUP => {
                handle_listening_hud_click(hwnd, point_from_lparam(lparam));
                0
            }
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
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
            WM_MOUSEMOVE => {
                refresh_copy_popup_hover(hwnd, point_from_lparam(lparam));
                0
            }
            WM_LBUTTONUP => {
                if let Some(owner_hwnd) = copy_popup_owner(hwnd) {
                    let click = point_from_lparam(lparam);
                    let dpi = overlay_dpi_for_window(hwnd);
                    if point_in_rect(click, copy_popup_copy_button_rect(hwnd, dpi)) {
                        let _ = copy_popup_text_to_clipboard(owner_hwnd);
                    } else if point_in_rect(click, copy_popup_close_button_rect(hwnd, dpi)) {
                        let _ = hide_copy_popup(owner_hwnd);
                    } else if point_in_rect(click, copy_popup_editor_frame_rect(hwnd, dpi)) {
                        clear_copy_popup_hover(hwnd);
                        if let Ok(state) = get_window_state_mut(owner_hwnd) {
                            let _ = SetForegroundWindow(hwnd);
                            let _ = SetFocus(state.copy_popup_edit_hwnd);
                        }
                    }
                }
                0
            }
            WM_COMMAND => {
                let control_id = (wparam & 0xFFFF) as isize;
                let notify_code = ((wparam >> 16) & 0xFFFF) as u32;
                if copy_popup_edit_control_index(control_id).is_some() && notify_code == EN_CHANGE {
                    if let Some(owner_hwnd) = copy_popup_owner(hwnd) {
                        let _ = update_copy_popup_edit_layout(owner_hwnd, hwnd);
                        InvalidateRect(hwnd, ptr::null(), 1);
                    }
                }
                0
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(owner_hwnd) = copy_popup_owner(hwnd) {
                    match desktop_copy_popup_action_for_virtual_key(wparam as u32) {
                        DesktopCopyPopupAction::CopyToClipboard => {
                            let _ = copy_popup_text_to_clipboard(owner_hwnd);
                        }
                        DesktopCopyPopupAction::Close => {
                            let _ = hide_copy_popup(owner_hwnd);
                        }
                        DesktopCopyPopupAction::Ignore => {}
                    }
                }
                0
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
                let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
                SetTextColor(hdc, typeless_popup_editor_text_color());
                SetBkColor(hdc, typeless_popup_editor_fill_color());
                SetBkMode(hdc, TRANSPARENT as i32);
                copy_popup_edit_brush() as isize
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
        let state = match unsafe { get_window_state_mut(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let (can_start, can_stop, active_generation, active_action_index) = {
            let shared = state.shared.lock().expect("Talk desktop shared state");
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
        let generation = unsafe { get_window_state_mut(hwnd) }
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
            request_stop_recording(hwnd, generation);
        }
    }

    fn ensure_packaged_local_asr_daemon(
        shared: &mut SharedState,
        config: &TalkConfig,
    ) -> Result<bool> {
        let Some(service) = config.speculative.streaming_service.as_ref() else {
            return Ok(false);
        };
        let endpoint = service.endpoint.clone();

        match &shared.local_asr_bootstrap_status {
            LocalAsrBootstrapStatus::Downloading
            | LocalAsrBootstrapStatus::NotStarted
            | LocalAsrBootstrapStatus::FallbackCloud(_) => return Ok(false),
            LocalAsrBootstrapStatus::EngineeringFallback
            | LocalAsrBootstrapStatus::Ready => {}
        }

        if let Some(mut daemon) = shared.local_asr_daemon.take() {
            if daemon.endpoint == endpoint && managed_local_asr_daemon_is_running(&mut daemon) {
                shared.local_asr_daemon = Some(daemon);
                return Ok(true);
            }
            stop_managed_local_asr_daemon(daemon);
        }

        if local_asr_endpoint_accepts_tcp(&endpoint, Duration::from_millis(80)) {
            return Ok(true);
        }

        let plan = if let (Some(worker_path), Some(model_root)) = (
            shared.product_runtime_worker.as_ref(),
            shared.product_model_root.as_ref(),
        ) {
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
        let Some(plan) = plan else {
            return Ok(false);
        };

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

        let deadline = Instant::now() + Duration::from_millis(800);
        loop {
            if local_asr_endpoint_accepts_tcp(&endpoint, Duration::from_millis(40)) {
                shared.local_asr_daemon = Some(ManagedLocalAsrDaemon { endpoint, child });
                return Ok(true);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    anyhow::bail!("packaged local ASR daemon exited before readiness: {status}");
                }
                Ok(None) => {}
                Err(error) => {
                    anyhow::bail!("check packaged local ASR daemon status: {error}");
                }
            }
            if Instant::now() >= deadline {
                shared.local_asr_daemon = Some(ManagedLocalAsrDaemon { endpoint, child });
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(40));
        }
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

    fn begin_recording(
        hwnd: HWND,
        state: &mut WindowState,
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
            shell_state,
            config_path,
            pending_hotkey_origin_insert_target,
        ) = {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
            if !shared.shell_state.can_start_session() {
                anyhow::bail!("Talk desktop is already busy");
            }
            shared.pending_copy_popup = None;
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
            shared.next_generation += 1;
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
                shared.shell_state,
                shared.config_path.clone(),
                if source == ActivationSource::Hotkey {
                    shared.pending_hotkey_origin_insert_target.take()
                } else {
                    None
                },
            )
        };

        let mut session = VoiceSession::new(session_id.clone());
        session
            .apply(VoiceEvent::TriggerStart)
            .context("start Talk desktop voice session")?;
        let trigger_events = vec!["trigger_start"];
        let (origin_insert_target, origin_insert_target_source, release_time_origin_target) =
            if source == ActivationSource::Hotkey {
                let release_time_origin_target =
                    capture_foreground_insert_target_context(hwnd, state.hud_hwnd);
                let origin_insert_target = resolve_hotkey_origin_insert_target(
                    pending_hotkey_origin_insert_target.as_ref(),
                    release_time_origin_target.as_ref(),
                );
                let origin_insert_target_source = if pending_hotkey_origin_insert_target.is_some() {
                    Some("hotkey_pending_pretrigger".to_string())
                } else if release_time_origin_target.is_some() {
                    Some("hotkey_release_time".to_string())
                } else {
                    None
                };
                (
                    origin_insert_target,
                    origin_insert_target_source,
                    release_time_origin_target,
                )
            } else {
                (
                    capture_foreground_insert_target_context(hwnd, state.hud_hwnd),
                    Some("record_start_capture".to_string()),
                    None,
                )
            };
        let speculative_local_asr_route =
            desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(&config));
        let configured_streaming_speculative_asr =
            speculative_local_asr_route == DesktopSpeculativeLocalAsrRoute::StreamingService;
        let local_asr_ready = if configured_streaming_speculative_asr {
            let ensure_result = {
                let mut shared = state.shared.lock().expect("Talk desktop shared state");
                ensure_packaged_local_asr_daemon(&mut shared, &config)
            };
            match ensure_result {
                Ok(ready) => ready,
                Err(error) => {
                    {
                        let mut shared = state.shared.lock().expect("Talk desktop shared state");
                        shared.local_asr_bootstrap_status =
                            LocalAsrBootstrapStatus::FallbackCloud(error.to_string());
                    }
                    false
                }
            }
        } else {
            false
        };
        let use_streaming_speculative_asr = desktop_effective_streaming_asr_enabled(
            speculative_local_asr_route,
            local_asr_ready,
        );
        if configured_streaming_speculative_asr && !use_streaming_speculative_asr {
            let _ = update_tray_icon(hwnd, "Talk: cloud ASR fallback");
            if !matches!(
                state
                    .shared
                    .lock()
                    .expect("Talk desktop shared state")
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
        let audio_override_raw = std::env::var(TALK_DESKTOP_AUDIO_FILE_OVERRIDE_ENV).ok();
        let recording_source = match resolve_desktop_audio_file_override(
            audio_override_raw.as_deref(),
            &config_path,
        ) {
            Ok(Some(audio_path)) => ActiveRecordingSource::ExplicitAudioFile(audio_path),
            Ok(None) => {
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

                match start_recording(&recording_request) {
                    Ok(recording) => {
                        let streaming_session = if use_streaming_speculative_asr {
                            match runtime_handle.block_on(LocalStreamingAsrLiveSession::start(
                                &config,
                                &session_id,
                                None,
                            )) {
                                Ok(streaming_session) => Some(streaming_session),
                                Err(error) => {
                                    let _ = recording.cancel();
                                    let _ = complete_failed_session(
                                        &config,
                                        session,
                                        trigger_events,
                                        error,
                                        false,
                                        |_| {},
                                    );
                                    let _ = refresh_idle_tray_status(hwnd);
                                    let _ = show_hud_text(
                                        hwnd,
                                        &compose_hud_message(
                                            "Talk: failed",
                                            Some("local streaming ASR unavailable"),
                                        ),
                                        Some(1800),
                                    );
                                    return Ok(());
                                }
                            }
                        } else {
                            None
                        };
                        ActiveRecordingSource::Live {
                            recording,
                            streaming_session,
                        }
                    }
                    Err(error) => {
                        let _ = complete_failed_session(
                            &config,
                            session,
                            trigger_events,
                            anyhow::anyhow!(error.to_string()),
                            false,
                            |_| {},
                        );
                        let _ = refresh_idle_tray_status(hwnd);
                        let _ = show_hud_text(
                            hwnd,
                            &compose_hud_message("Talk: failed", Some(&error.to_string())),
                            Some(1800),
                        );
                        return Ok(());
                    }
                }
            }
            Err(error) => {
                let _ = complete_failed_session(
                    &config,
                    session,
                    trigger_events,
                    anyhow::anyhow!(error.clone()),
                    false,
                    |_| {},
                );
                let _ = refresh_idle_tray_status(hwnd);
                let _ = show_hud_text(
                    hwnd,
                    &compose_hud_message("Talk: failed", Some(&error)),
                    Some(1800),
                );
                return Ok(());
            }
        };

        {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
            shared.shell_state = shell_state
                .begin_recording()
                .expect("idle shell state should begin recording");
            shared.current_phase = Some(RuntimePhase::Recording);
            shared.active_recording = Some(ActiveRecording {
                action_index,
                mode_override,
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
                speculative_segmenter_config: SegmenterConfig::default(),
                live_streaming_inserted_anchors: HashMap::new(),
                live_streaming_inserted_segment_ids: Vec::new(),
                hud_streaming_segments: Vec::new(),
                last_streaming_asr_event: None,
                last_streaming_asr_event_at: None,
            });
        }

        if source == ActivationSource::Hotkey {
            spawn_hotkey_origin_enrichment(hwnd, state.hud_hwnd, generation);
        }

        update_tray_icon(hwnd, "Talk: listening")?;
        show_hud_text(hwnd, hud_message_for_phase(RuntimePhase::Recording), None)?;

        if trigger_mode == TriggerMode::PushToTalk && source == ActivationSource::Hotkey {
            if let Some(hotkey) = hotkey {
                match select_windows_hotkey_binding_strategy(&hotkey) {
                    WindowsHotkeyBindingStrategy::RegisterHotKey => {
                        spawn_release_watcher(hwnd, generation, hotkey, max_recording_seconds);
                    }
                    WindowsHotkeyBindingStrategy::LowLevelHook => {
                        spawn_timeout_watcher(hwnd, generation, max_recording_seconds);
                    }
                }
            } else {
                spawn_timeout_watcher(hwnd, generation, max_recording_seconds);
            }
        } else {
            match recording_stop_watcher_policy(trigger_mode, max_recording_seconds) {
                DesktopRecordingStopWatcherPolicy::ManualOnly => {}
                DesktopRecordingStopWatcherPolicy::TimeoutAfterSeconds(seconds) => {
                    spawn_timeout_watcher(hwnd, generation, seconds);
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
                SpeculativeRuntimeEvent::CorrectionRequested { .. } => {}
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn apply_speculative_correction_patch_if_safe(
        hwnd: HWND,
        hud_hwnd: HWND,
        anchor: &SpeculativeInsertAnchor,
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
        let inserter = ClipboardPasteInserter::new(
            WindowsClipboardBackend,
            WindowsPasteShortcut,
            restore_policy,
        );
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
        if let Ok(automation) = UIAutomation::new() {
            if let Ok(element) = automation.get_focused_element() {
                if let Some(text) = automation_element_current_text(&element) {
                    return Some(text);
                }
            }

            let candidate_hwnd = target.focus_handle.unwrap_or(target.window_handle) as HWND;
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

        target
            .focus_handle
            .or(Some(target.window_handle))
            .and_then(|handle| window_text(handle as HWND))
    }

    fn apply_document_recorrection_patch_if_safe(
        hwnd: HWND,
        hud_hwnd: HWND,
        anchor: &SpeculativeInsertAnchor,
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
        let target_still_safe = anchor.window_handle == current_target.window_handle
            && anchor.focus_handle == current_target.focus_handle;
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
        let inserter = ClipboardPasteInserter::new(
            WindowsClipboardBackend,
            WindowsPasteShortcut,
            restore_policy,
        );
        inserter.insert_text(corrected_text)?;
        Ok(true)
    }

    fn spawn_speculative_cloud_correction(
        runtime_handle: tokio::runtime::Handle,
        shared: Arc<Mutex<SharedState>>,
        job: SpeculativeCloudCorrectionJob,
    ) {
        runtime_handle.spawn(async move {
            let mut front_context = FrontContext::default();
            if let Some(context_before) = job
                .context_before
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                front_context.extra.insert(
                    "contextBefore".to_string(),
                    Value::String(context_before.to_string()),
                );
            }
            let corrected_text = match process_voice_transcript_text(
                &job.config,
                job.transcript.clone(),
                job.mode_override,
                front_context,
            )
            .await
            {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("Talk speculative cloud correction failed: {error:#}");
                    return;
                }
            };

            if corrected_text.trim().is_empty() || corrected_text == job.transcript {
                return;
            }

            let received_at_ms = job.started_at.elapsed().as_millis() as u64;
            let patched = if !job.full_document_inserted_segments.is_empty() {
                match apply_document_recorrection_patch_if_safe(
                    job.hwnd_value as HWND,
                    job.hud_hwnd_value as HWND,
                    &job.anchor,
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
            } else if job.latest_live_segment_guard.is_none_or(|guard| {
                live_streaming_correction_anchor_still_latest(
                    &shared,
                    guard,
                    job.anchor.segment_id.as_str(),
                )
            }) {
                match apply_speculative_correction_patch_if_safe(
                    job.hwnd_value as HWND,
                    job.hud_hwnd_value as HWND,
                    &job.anchor,
                    job.anchor.segment_id.as_str(),
                    &corrected_text,
                    received_at_ms,
                    job.config.speculative.max_patch_age_ms,
                    job.config.speculative.max_auto_patch_edit_ratio,
                    job.config.output.restore_clipboard,
                ) {
                    Ok(patched) => {
                        if patched {
                            if let Some(guard) = job.latest_live_segment_guard {
                                update_live_streaming_inserted_anchor_text(
                                    &shared,
                                    guard.generation,
                                    job.anchor.segment_id.as_str(),
                                    &corrected_text,
                                );
                            }
                        }
                        patched
                    }
                    Err(error) => {
                        eprintln!("Talk speculative correction patch failed: {error:#}");
                        false
                    }
                }
            } else {
                false
            };

            if patched {
                return;
            }

            if let Ok(mut shared) = shared.lock() {
                shared.pending_copy_popup = Some(PendingCopyPopup {
                    generation: job.generation,
                    model: desktop_copy_popup_model(&corrected_text),
                });
            }
            unsafe {
                let _ = PostMessageW(
                    job.hwnd_value as HWND,
                    CORRECTION_COPY_POPUP_MESSAGE,
                    job.generation as usize,
                    0,
                );
            }
        });
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

    fn insert_live_streaming_local_segment_if_safe(
        config: &TalkConfig,
        hwnd: HWND,
        hud_hwnd: HWND,
        origin_insert_target: Option<&DesktopInsertTargetContext>,
        segment_id: &str,
        text: &str,
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
        let plan = live_streaming_local_segment_plan(
            config.output.mode,
            &event,
            origin_insert_target,
            current_context.as_ref(),
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
        let previous_paste_shortcut_env = desktop_preferred_paste_shortcut_for_target(
            &config.desktop.paste.shortcut_overrides,
            process_name.as_deref(),
            current_context.as_ref(),
        )
        .map(|preferred_mode| {
            let previous = std::env::var_os(TALK_WINDOWS_PASTE_SHORTCUT_ENV);
            std::env::set_var(
                TALK_WINDOWS_PASTE_SHORTCUT_ENV,
                desktop_paste_shortcut_env_value(preferred_mode),
            );
            previous
        });

        let restore_policy = if config.output.restore_clipboard {
            ClipboardRestorePolicy::RestoreOriginal
        } else {
            ClipboardRestorePolicy::LeaveInsertedText
        };
        let inserter = ClipboardPasteInserter::new(
            WindowsClipboardBackend,
            WindowsPasteShortcut,
            restore_policy,
        );
        let insert_result = inserter.insert_text(&text);

        if let Some(previous) = previous_paste_shortcut_env {
            match previous {
                Some(previous) => std::env::set_var(TALK_WINDOWS_PASTE_SHORTCUT_ENV, previous),
                None => std::env::remove_var(TALK_WINDOWS_PASTE_SHORTCUT_ENV),
            }
        }

        insert_result?;
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

    fn speculative_correction_job_for_live_inserted_segment(
        config: &TalkConfig,
        pipeline_config: &DesktopSpeculativePipelineConfig,
        event: &SpeculativeRuntimeEvent,
        anchor: &SpeculativeInsertAnchor,
        mode_override: Option<VoiceMode>,
        generation: u64,
        hwnd_value: usize,
        hud_hwnd_value: usize,
    ) -> Option<SpeculativeCloudCorrectionJob> {
        let insert_target = ForegroundInsertTarget {
            window_handle: anchor.window_handle,
            focus_handle: anchor.focus_handle,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        };
        let model = desktop_speculative_correction_job_model(
            pipeline_config,
            event,
            Some(insert_target),
            anchor.inserted_at_ms,
        )?;
        let DesktopSpeculativeCorrectionOutputTarget::PatchInsertedText(anchor) =
            model.output_target
        else {
            return None;
        };
        Some(SpeculativeCloudCorrectionJob {
            config: config.clone(),
            transcript: model.local_text,
            context_before: Some(model.context_before),
            mode_override,
            anchor,
            full_document_inserted_segments: Vec::new(),
            latest_live_segment_guard: Some(LatestLiveSegmentGuard { generation }),
            generation,
            started_at: Instant::now(),
            hwnd_value,
            hud_hwnd_value,
        })
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

    fn request_stop_recording(hwnd: HWND, generation: u64) {
        let state = match unsafe { get_window_state_mut(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let (config, runtime_handle, mut active) = {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
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
            let Some(config) = shared.config.clone() else {
                shared.shell_state = shared.shell_state.complete();
                shared.current_phase = None;
                shared.worker_generation = None;
                return;
            };
            (config, shared.runtime_handle.clone(), active)
        };

        if let Err(error) = active.session.apply(VoiceEvent::TriggerStop) {
            let _ = complete_failed_session(
                &config,
                active.session,
                active.trigger_events,
                anyhow::anyhow!(error.to_string()),
                false,
                |_| {},
            );
            let _ = mark_idle_after_terminal_state(hwnd);
            return;
        }
        active.trigger_events.push("trigger_stop");

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
        let live_inserted_anchors_for_stop = active
            .live_streaming_inserted_segment_ids
            .iter()
            .filter_map(|segment_id| active.live_streaming_inserted_anchors.get(segment_id))
            .cloned()
            .collect::<Vec<_>>();
        let streaming_stop_policy =
            desktop_streaming_stop_policy(live_inserted_anchors_for_stop.len());

        let stopped_source = match active.source {
            ActiveRecordingSource::Live {
                recording,
                streaming_session,
            } if use_streaming_speculative_asr => StoppedRecordingSource::StreamingRecording {
                recording,
                streaming_session,
            },
            ActiveRecordingSource::Live {
                recording,
                streaming_session,
            } => {
                if let Some(streaming_session) = streaming_session {
                    runtime_handle.spawn(async move {
                        if let Err(cancel_error) = streaming_session.cancel().await {
                            eprintln!(
                                "Talk local streaming ASR cancel failed after route change: {cancel_error:#}"
                            );
                        }
                    });
                }
                match recording.finish() {
                    Ok(artifact) => StoppedRecordingSource::AudioFile(artifact.path),
                    Err(error) => {
                        let _ = complete_failed_session(
                            &config,
                            active.session,
                            active.trigger_events,
                            anyhow::anyhow!(error.to_string()),
                            false,
                            |_| {},
                        );
                        let _ = show_hud_text(
                            hwnd,
                            &compose_hud_message("Talk: failed", Some(&error.to_string())),
                            Some(1800),
                        );
                        let _ = mark_idle_after_terminal_state(hwnd);
                        return;
                    }
                }
            }
            ActiveRecordingSource::ExplicitAudioFile(audio_path)
                if use_streaming_speculative_asr =>
            {
                let error = "streaming_service local ASR requires a live recording source";
                let _ = complete_failed_session(
                    &config,
                    active.session,
                    active.trigger_events,
                    anyhow::anyhow!(error),
                    false,
                    |_| {},
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
            let shared = Arc::clone(&state.shared);
            let hwnd_value = hwnd as usize;
            let error_message = format!(
                "unsupported speculative.local_asr value: {}",
                speculative_pipeline_config.local_asr
            );
            runtime_handle.spawn(async move {
                let result = complete_failed_session(
                    &config,
                    active.session,
                    active.trigger_events,
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
            && desktop_speculative_cloud_correction_enabled(&speculative_pipeline_config);
        let local_asr_correction_segment_id = if use_streaming_speculative_asr {
            "streaming-service-final"
        } else {
            "external-asr-final"
        };

        let shared = Arc::clone(&state.shared);
        let correction_runtime_handle = runtime_handle.clone();
        let hwnd_value = hwnd as usize;
        let hud_hwnd_value = state.hud_hwnd as usize;
        let output_mode = config.output.mode;
        let mode_override = active.mode_override;
        let origin_insert_target = active.origin_insert_target.clone();
        let paste_shortcut_overrides = config.desktop.paste.shortcut_overrides.clone();
        let origin_insert_target_for_before_hook = origin_insert_target.clone();
        let origin_insert_target_for_tail_insert = origin_insert_target.clone();
        let origin_insert_target_for_report = origin_insert_target.clone();
        let origin_insert_target_source = active.origin_insert_target_source.clone();
        let pending_hotkey_origin_insert_target =
            active.pending_hotkey_origin_insert_target.clone();
        let release_time_origin_insert_target = active.release_time_origin_insert_target.clone();
        let restore_diagnostic = Arc::new(Mutex::new(None::<DesktopInsertTargetRestoreDiagnostic>));
        let restored_insert_target = Arc::new(Mutex::new(None::<ForegroundInsertTarget>));
        let selected_output_strategy = Arc::new(Mutex::new(None::<DesktopOutputStrategy>));
        let selected_show_result_in_gui = Arc::new(Mutex::new(false));
        let captured_insert_target_context =
            Arc::new(Mutex::new(None::<DesktopInsertTargetContext>));
        let paste_shortcut_env_restore = Arc::new(Mutex::new(None::<Option<OsString>>));
        let insert_final_transcript_at_stop = streaming_stop_policy.insert_final_transcript;
        let runtime_voice_mode = mode_override.unwrap_or_else(|| config.default_voice_mode());
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
        let paste_shortcut_env_restore_for_before_hook = Arc::clone(&paste_shortcut_env_restore);
        let paste_shortcut_env_restore_for_after_hook = Arc::clone(&paste_shortcut_env_restore);
        runtime_handle.spawn(async move {
            let before_insert = move |insert_context: &talk_runtime::RuntimeInsertContext| {
                if !insert_final_transcript_at_stop {
                    return RuntimeInsertDirective::DryRunOnly;
                }

                let current_context = capture_foreground_insert_target_context(
                    hwnd_value as HWND,
                    hud_hwnd_value as HWND,
                );
                if let Ok(mut slot) = captured_insert_target_context_for_before_hook.lock() {
                    *slot = current_context.clone();
                }
                let output_plan = desktop_output_plan(
                    output_mode,
                    origin_insert_target_for_before_hook.as_ref(),
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

                if output_mode == OutputMode::ClipboardPaste
                    && output_plan.strategy == DesktopOutputStrategy::HonorConfiguredOutput
                {
                    if let Some(preferred_mode) = desktop_preferred_paste_shortcut_for_target(
                        &paste_shortcut_overrides,
                        output_plan
                            .insert_target
                            .and_then(|target| {
                                resolve_window_process_base_name(target.window_handle)
                            })
                            .as_deref(),
                        current_context.as_ref(),
                    ) {
                        if let Ok(mut slot) = paste_shortcut_env_restore_for_before_hook.lock() {
                            if slot.is_none() {
                                *slot = Some(std::env::var_os(TALK_WINDOWS_PASTE_SHORTCUT_ENV));
                            }
                        }
                        std::env::set_var(
                            TALK_WINDOWS_PASTE_SHORTCUT_ENV,
                            desktop_paste_shortcut_env_value(preferred_mode),
                        );
                    }
                }

                if let Some(target) = output_plan.insert_target {
                    if desktop_insert_target_restore_requested(target, current_context.as_ref()) {
                        let (effective_target, diagnostic) = begin_restore_foreground_insert_target(
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
                    }
                }
                RuntimeInsertDirective::UseConfiguredOutput
            };
            let after_insert = move || {
                if let Ok(mut slot) = paste_shortcut_env_restore_for_after_hook.lock() {
                    if let Some(previous) = slot.take() {
                        match previous {
                            Some(previous) => {
                                std::env::set_var(TALK_WINDOWS_PASTE_SHORTCUT_ENV, previous);
                            }
                            None => {
                                std::env::remove_var(TALK_WINDOWS_PASTE_SHORTCUT_ENV);
                            }
                        }
                    }
                }

                let effective_target = restored_insert_target_for_after_hook
                    .lock()
                    .ok()
                    .and_then(|slot| *slot);
                if let Some(target) = effective_target {
                    let post_insert_hold = end_restore_foreground_insert_target(target);
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
            let streaming_session_id = session.id().to_string();
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
                    StoppedRecordingSource::StreamingRecording {
                        recording,
                        streaming_session,
                    },
                ) => {
                    let events_result = if let Some(streaming_session) = streaming_session {
                        streaming_session.stop(recording).await
                    } else {
                        let events_result = run_local_streaming_asr_service_from_recording(
                            &config,
                            &streaming_session_id,
                            &recording,
                            None,
                        )
                        .await;
                        let cancel_result = recording.cancel();
                        if let Err(error) = cancel_result {
                            Err(anyhow::anyhow!(error.to_string()))
                        } else {
                            events_result
                        }
                    };
                    match events_result {
                        Ok(events) => {
                            let selected_event = events
                                .iter()
                                .rev()
                                .find(|event| event.is_final())
                                .or_else(|| events.last())
                                .cloned();
                            if let Some(selected_event) = selected_event {
                                let transcript = final_transcript_from_streaming_asr_events(&events)
                                    .unwrap_or_else(|_| selected_event.text().to_string());
                                if streaming_stop_policy.insert_final_transcript {
                                    run_voice_session_from_transcript_with_insert_hooks(
                                        &config,
                                        session,
                                        trigger_events,
                                        transcript,
                                        mode_override,
                                        FrontContext::default(),
                                        before_insert,
                                        after_insert,
                                        phase_callback,
                                    )
                                    .await
                                } else {
                                    let tail_text = desktop_streaming_stop_tail_text(
                                        selected_event.segment_id(),
                                        selected_event.text(),
                                        &live_inserted_anchors_for_stop,
                                    );
                                    let mut session_transcript = live_inserted_anchors_for_stop
                                        .iter()
                                        .map(|anchor| anchor.inserted_text.as_str())
                                        .collect::<String>();
                                    if let Some(tail_text) = tail_text.as_deref() {
                                        session_transcript.push_str(tail_text);
                                    }
                                    if session_transcript.trim().is_empty() {
                                        session_transcript = transcript;
                                    }

                                    if let Some(tail_text) = tail_text {
                                        match insert_live_streaming_local_segment_if_safe(
                                            &config,
                                            hwnd_value as HWND,
                                            hud_hwnd_value as HWND,
                                            origin_insert_target_for_tail_insert.as_ref(),
                                            selected_event.segment_id(),
                                            &tail_text,
                                        ) {
                                            Ok(Some(_)) => {}
                                            Ok(None) => {
                                                if let Ok(mut shared) = shared.lock() {
                                                    shared.pending_copy_popup =
                                                        Some(PendingCopyPopup {
                                                            generation,
                                                            model: desktop_copy_popup_model(
                                                                &tail_text,
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
                                                                &tail_text,
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

                                    run_voice_session_from_local_transcript_with_insert_hooks(
                                        &config,
                                        session,
                                        trigger_events,
                                        session_transcript,
                                        mode_override,
                                        before_insert,
                                        after_insert,
                                        phase_callback,
                                    )
                                }
                            } else {
                                complete_failed_session(
                                    &config,
                                    session,
                                    trigger_events,
                                    anyhow::anyhow!(
                                        "external streaming ASR command produced no events"
                                    ),
                                    false,
                                    phase_callback,
                                )
                            }
                        }
                        Err(error) => complete_failed_session(
                            &config,
                            session,
                            trigger_events,
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
                    StoppedRecordingSource::StreamingRecording {
                        recording,
                        streaming_session,
                    },
                ) => {
                    if let Some(streaming_session) = streaming_session {
                        let _ = streaming_session.cancel().await;
                    }
                    let _ = recording.cancel();
                    complete_failed_session(
                        &config,
                        session,
                        trigger_events,
                        anyhow::anyhow!(
                            "streaming recording source was selected for a non-streaming ASR route"
                        ),
                        false,
                        phase_callback,
                    )
                }
                (_, true, StoppedRecordingSource::AudioFile(_)) => complete_failed_session(
                    &config,
                    session,
                    trigger_events,
                    anyhow::anyhow!("streaming_service local ASR requires a live recording source"),
                    false,
                    phase_callback,
                ),
            };

            let mut correction_job = None;
            if let Ok(mut shared) = shared.lock() {
                match result {
                    Ok(report) => {
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
                            && streaming_stop_policy.allow_final_correction_job
                        {
                            if let (Some(target), Some(output_text)) =
                                (persisted_insert_target, report.session.output_text())
                            {
                                let local_text = output_text.to_string();
                                match SpeculativeInsertAnchor::new(
                                    target.window_handle,
                                    target.focus_handle,
                                    local_asr_correction_segment_id,
                                    local_text.clone(),
                                    0,
                                ) {
                                    Ok(anchor) => {
                                        correction_job = Some(SpeculativeCloudCorrectionJob {
                                            config: config.clone(),
                                            transcript: local_text.clone(),
                                            context_before: None,
                                            mode_override,
                                            anchor,
                                            full_document_inserted_segments: vec![local_text.clone()],
                                            latest_live_segment_guard: None,
                                            generation,
                                            started_at: Instant::now(),
                                            hwnd_value,
                                            hud_hwnd_value,
                                        });
                                    }
                                    Err(error) => {
                                        eprintln!(
                                            "Talk speculative correction anchor skipped: {error}"
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
                        shared.pending_worker_error = Some((generation, error.to_string()));
                        set_last_session(&mut shared, "failed", Some(error.to_string()));
                    }
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
    }

    fn apply_runtime_phase(hwnd: HWND, phase: RuntimePhase, generation: u64) {
        let state = match unsafe { get_window_state_mut(hwnd) } {
            Ok(state) => state,
            Err(_) => return,
        };
        let active_generation = {
            let shared = state.shared.lock().expect("Talk desktop shared state");
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
        let (unexpected_error, pending_copy_popup) = {
            let state = match unsafe { get_window_state_mut(hwnd) } {
                Ok(state) => state,
                Err(_) => return,
            };
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
            if shared.worker_generation != Some(generation) {
                return;
            }
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
            let unexpected_error = match shared.pending_worker_error.take() {
                Some((error_generation, error)) if error_generation == generation => Some(error),
                Some(other) => {
                    shared.pending_worker_error = Some(other);
                    None
                }
                None => None,
            };
            (unexpected_error, pending_copy_popup)
        };

        if unexpected_error.is_some() {
            let _ = show_hud_text(
                hwnd,
                &compose_hud_message("Talk: failed", unexpected_error.as_deref()),
                Some(1800),
            );
        }
        if let Some(popup) = pending_copy_popup {
            let _ = hide_hud(hwnd);
            let _ = show_copy_popup(hwnd, popup.model);
        }
        let _ = refresh_idle_tray_status(hwnd);
    }

    fn handle_correction_copy_popup(hwnd: HWND, generation: u64) {
        let pending_copy_popup = {
            let state = match unsafe { get_window_state_mut(hwnd) } {
                Ok(state) => state,
                Err(_) => return,
            };
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
            match shared.pending_copy_popup.take() {
                Some(popup) if popup.generation == generation => Some(popup),
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
        let state = unsafe { get_window_state_mut(hwnd)? };
        let mut shared = state.shared.lock().expect("Talk desktop shared state");
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
                if let Ok(state) = unsafe { get_window_state_mut(hwnd) } {
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
                let generation = unsafe { get_window_state_mut(hwnd) }
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
                let _ = reload_config(hwnd);
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
        let state = unsafe { get_window_state_mut(hwnd)? };
        let label = {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
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
        let state = unsafe { get_window_state_mut(hwnd)? };
        let shared = state.shared.lock().expect("Talk desktop shared state");
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
        std::process::Command::new("explorer")
            .arg(&logs_dir)
            .spawn()
            .with_context(|| format!("open Talk logs folder {}", logs_dir.display()))?;
        Ok(())
    }

    fn open_config_file(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
        let shared = state.shared.lock().expect("Talk desktop shared state");
        std::process::Command::new("notepad.exe")
            .arg(&shared.config_path)
            .spawn()
            .with_context(|| format!("open Talk config {}", shared.config_path.display()))?;
        Ok(())
    }

    fn show_status_dialog(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
        let shared = state.shared.lock().expect("Talk desktop shared state");
        let report = build_status_report(&status_snapshot(&shared));
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

    fn reload_config(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
        let (config_path, runtime_handle) = {
            let shared = state.shared.lock().expect("Talk desktop shared state");
            (shared.config_path.clone(), shared.runtime_handle.clone())
        };

        let (status_text, tray_status) = {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
            match runtime_handle
                .block_on(load_effective_config(&config_path))
                .with_context(|| format!("reload Talk config {}", config_path.display()))
            {
                Ok(config) => {
                    let selected_voice_mode = config.default_voice_mode();
                    shared.config = Some(config.clone());
                    shared.config_status = ConfigAvailability::ready();
                    shared.selected_voice_mode = selected_voice_mode;
                    match initial_hotkey_binding(&config) {
                        Ok((desktop_actions, hotkey)) => {
                            shared.desktop_actions = desktop_actions;
                            shared.hotkey = hotkey;
                        }
                        Err(error) => {
                            shared.desktop_actions = Vec::new();
                            shared.hotkey = HotkeyBindingState::invalid_config(
                                desktop_shortcut_label_from_config(&config),
                                error,
                            );
                        }
                    }
                    shared.native_readiness = Some(configured_native_readiness(&config));
                    if desktop_speculative_local_asr_route(&desktop_speculative_pipeline_config(
                        &config,
                    )) != DesktopSpeculativeLocalAsrRoute::StreamingService
                    {
                        if let Some(daemon) = shared.local_asr_daemon.take() {
                            stop_managed_local_asr_daemon(daemon);
                        }
                    }
                }
                Err(error) => {
                    if shared.config.is_none() {
                        shared.config_status = ConfigAvailability::unavailable(error.to_string());
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
            )
        };

        update_tray_icon(hwnd, &tray_status)?;
        show_hud_text(hwnd, &status_text, Some(1800))?;
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
        let state = unsafe { get_window_state_mut(hwnd)? };
        let (shell_state, config_status, hotkey_state, native_readiness, selected_voice_mode) = {
            let shared = state.shared.lock().expect("Talk desktop shared state");
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
        let state = unsafe { get_window_state_mut(hwnd)? };
        if state.hud_hwnd.is_null() {
            anyhow::bail!("Talk desktop HUD is unavailable");
        }

        let dpi = overlay_dpi_for_window(state.hud_hwnd);
        let metrics = scale_hud_metrics_for_dpi(desktop_hud_metrics_for_view_model(&model), dpi);
        let (screen_width, screen_height) = current_screen_size();
        let x = ((screen_width - metrics.width).max(0)) / 2;
        let y = (screen_height - metrics.height - metrics.bottom_margin).max(0);
        unsafe {
            KillTimer(hwnd, TIMER_HIDE_HUD);
            if matches!(model.visual_state, DesktopHudVisualState::Listening) {
                SetTimer(
                    hwnd,
                    TIMER_RECORDING_LEVEL,
                    HUD_RECORDING_LEVEL_REFRESH_MS,
                    None,
                );
                KillTimer(hwnd, TIMER_THINKING_PROGRESS);
            } else if matches!(model.visual_state, DesktopHudVisualState::Thinking) {
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
            if let Ok(mut overlay) = overlay_ui_state().lock() {
                let was_listening = matches!(
                    overlay.hud_model.as_ref().map(|hud| hud.visual_state),
                    Some(DesktopHudVisualState::Listening)
                );
                let was_thinking = matches!(
                    overlay.hud_model.as_ref().map(|hud| hud.visual_state),
                    Some(DesktopHudVisualState::Thinking)
                );
                if model.visual_state != DesktopHudVisualState::Listening || !was_listening {
                    overlay.hud_meter_bins = [0.0; 9];
                    overlay.hud_streaming_partial_text = None;
                }
                if model.visual_state != DesktopHudVisualState::Thinking || !was_thinking {
                    overlay.hud_thinking_pulse_tick = 0;
                }
                overlay.hud_model = Some(model);
            }
            SetWindowPos(
                state.hud_hwnd,
                (-1isize) as HWND,
                x,
                y,
                metrics.width,
                metrics.height,
                SWP_NOACTIVATE,
            );
            apply_rounded_window_region(
                state.hud_hwnd,
                metrics.width,
                metrics.height,
                metrics.corner_radius,
            );
            InvalidateRect(state.hud_hwnd, ptr::null(), 1);
            ShowWindow(state.hud_hwnd, SW_SHOWNOACTIVATE);
            if let Some(timeout) = auto_hide_ms {
                SetTimer(hwnd, TIMER_HIDE_HUD, timeout, None);
            }
        }
        Ok(())
    }

    fn show_hud_text(hwnd: HWND, text: &str, auto_hide_ms: Option<u32>) -> Result<()> {
        show_hud_model(hwnd, desktop_hud_view_model_for_text(text), auto_hide_ms)
    }

    fn refresh_recording_hud_level(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
        let (raw_waveform, latest_hud_transcript, live_dispatch) = {
            let mut shared = state.shared.lock().expect("Talk desktop shared state");
            if shared.current_phase != Some(RuntimePhase::Recording) {
                unsafe {
                    KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                }
                return Ok(());
            }

            let config = shared.config.clone();
            let runtime_handle = shared.runtime_handle.clone();
            let Some(active) = shared.active_recording.as_mut() else {
                unsafe {
                    KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                }
                return Ok(());
            };

            let mut pumped_asr_events = Vec::<StreamingAsrEvent>::new();
            let raw_waveform = match &mut active.source {
                ActiveRecordingSource::Live {
                    recording,
                    streaming_session,
                } => {
                    if let Some(streaming_session) = streaming_session.as_mut() {
                        match runtime_handle.block_on(
                            streaming_session
                                .pump_available_audio(recording, Duration::from_millis(1)),
                        ) {
                            Ok(events) => {
                                pumped_asr_events = events;
                            }
                            Err(error) => {
                                eprintln!("Talk local streaming ASR live pump failed: {error:#}");
                            }
                        }
                    }
                    recording
                        .current_waveform(9)
                        .unwrap_or_else(|_| vec![0.0; 9])
                }
                ActiveRecordingSource::ExplicitAudioFile(_) => vec![0.0; 9],
            };

            let now = Instant::now();
            let mut runtime_events = Vec::<SpeculativeRuntimeEvent>::new();
            for event in pumped_asr_events.iter().cloned() {
                active.last_streaming_asr_event = Some(event.clone());
                active.last_streaming_asr_event_at = Some(now);
                match active
                    .speculative_runtime_state
                    .accept_asr_event_with_segmentation(
                        event,
                        0,
                        &active.speculative_segmenter_config,
                    ) {
                    Ok(events) => runtime_events.extend(events),
                    Err(error) => eprintln!("Talk speculative ASR event rejected: {error}"),
                }
            }

            if pumped_asr_events.is_empty() {
                if let (Some(last_event), Some(last_event_at)) = (
                    active.last_streaming_asr_event.clone(),
                    active.last_streaming_asr_event_at,
                ) {
                    if !last_event.is_final() {
                        let trailing_silence_ms = last_event_at.elapsed().as_millis() as u64;
                        match active
                            .speculative_runtime_state
                            .accept_asr_event_with_segmentation(
                                last_event,
                                trailing_silence_ms,
                                &active.speculative_segmenter_config,
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
                        upsert_hud_streaming_segment(
                            &mut active.hud_streaming_segments,
                            segment_id,
                            text,
                        );
                    }
                    SpeculativeRuntimeEvent::CorrectionRequested { .. } => {}
                }
            }

            let live_dispatch = config.filter(|_| !runtime_events.is_empty()).map(|config| {
                PendingLiveStreamingDispatch {
                    pipeline_config: desktop_speculative_pipeline_config(&config),
                    config,
                    runtime_handle: runtime_handle.clone(),
                    mode_override: active.mode_override,
                    generation: active.generation,
                    origin_insert_target: active.origin_insert_target.clone(),
                    existing_anchors: active.live_streaming_inserted_anchors.clone(),
                    events: runtime_events,
                    hwnd_value: hwnd as usize,
                    hud_hwnd_value: state.hud_hwnd as usize,
                }
            });

            let latest_hud_transcript =
                hud_streaming_transcript_from_segments(&active.hud_streaming_segments);

            (raw_waveform, latest_hud_transcript, live_dispatch)
        };

        if let Some(dispatch) = live_dispatch {
            let mut known_anchors = dispatch.existing_anchors;
            let mut newly_inserted_anchors = Vec::<SpeculativeInsertAnchor>::new();
            let mut correction_jobs = Vec::<SpeculativeCloudCorrectionJob>::new();

            for event in &dispatch.events {
                match event {
                    SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text } => {
                        if known_anchors.contains_key(segment_id) {
                            continue;
                        }
                        match insert_live_streaming_local_segment_if_safe(
                            &dispatch.config,
                            dispatch.hwnd_value as HWND,
                            dispatch.hud_hwnd_value as HWND,
                            dispatch.origin_insert_target.as_ref(),
                            segment_id,
                            text,
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
                        if let Some(anchor) = known_anchors.get(segment_id) {
                            if let Some(job) = speculative_correction_job_for_live_inserted_segment(
                                &dispatch.config,
                                &dispatch.pipeline_config,
                                event,
                                anchor,
                                dispatch.mode_override,
                                dispatch.generation,
                                dispatch.hwnd_value,
                                dispatch.hud_hwnd_value,
                            ) {
                                correction_jobs.push(job);
                            }
                        }
                    }
                    SpeculativeRuntimeEvent::DraftUpdated { .. } => {}
                }
            }

            if !newly_inserted_anchors.is_empty() {
                if let Ok(mut shared) = state.shared.lock() {
                    if let Some(active) = shared.active_recording.as_mut() {
                        if active.generation == dispatch.generation {
                            for anchor in newly_inserted_anchors {
                                if !active
                                    .live_streaming_inserted_segment_ids
                                    .iter()
                                    .any(|segment_id| segment_id == &anchor.segment_id)
                                {
                                    active
                                        .live_streaming_inserted_segment_ids
                                        .push(anchor.segment_id.clone());
                                }
                                active
                                    .live_streaming_inserted_anchors
                                    .insert(anchor.segment_id.clone(), anchor);
                            }
                        }
                    }
                }
            }

            for job in correction_jobs {
                spawn_speculative_cloud_correction(
                    dispatch.runtime_handle.clone(),
                    Arc::clone(&state.shared),
                    job,
                );
            }
        }

        let updated_hud_model = if let Ok(mut overlay) = overlay_ui_state().lock() {
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
            if let Some(partial_text) = latest_hud_transcript {
                overlay.hud_streaming_partial_text = Some(partial_text);
            }
            let partial_text = overlay.hud_streaming_partial_text.clone();
            if let Some(hud_model) = overlay.hud_model.as_mut() {
                if hud_model.visual_state == DesktopHudVisualState::Listening {
                    *hud_model = desktop_hud_view_model_for_listening_waveform_with_partial(
                        next_bins,
                        partial_text.as_deref(),
                    );
                    Some(hud_model.clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(updated_hud_model) = updated_hud_model {
            if state.hud_hwnd.is_null() {
                return Ok(());
            }
            let dpi = overlay_dpi_for_window(state.hud_hwnd);
            let metrics = scale_hud_metrics_for_dpi(
                desktop_hud_metrics_for_view_model(&updated_hud_model),
                dpi,
            );
            let (screen_width, screen_height) = current_screen_size();
            let x = ((screen_width - metrics.width).max(0)) / 2;
            let y = (screen_height - metrics.height - metrics.bottom_margin).max(0);
            unsafe {
                SetWindowPos(
                    state.hud_hwnd,
                    (-1isize) as HWND,
                    x,
                    y,
                    metrics.width,
                    metrics.height,
                    SWP_NOACTIVATE,
                );
                apply_rounded_window_region(
                    state.hud_hwnd,
                    metrics.width,
                    metrics.height,
                    metrics.corner_radius,
                );
                InvalidateRect(state.hud_hwnd, ptr::null(), 1);
            }
        }
        Ok(())
    }

    fn refresh_thinking_hud_progress(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
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
        if should_invalidate && !state.hud_hwnd.is_null() {
            unsafe {
                InvalidateRect(state.hud_hwnd, ptr::null(), 1);
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
        let automation_focus = capture_automation_focus_target(foreground);
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
        if automation_ui_element_looks_editable(&window_element) {
            candidates.push(window_element.clone());
        }

        if let Ok(elements) = automation
            .create_matcher()
            .from(window_element)
            .depth(32)
            .timeout(0)
            .filter_fn(Box::new(|element: &UIElement| {
                Ok(automation_ui_element_looks_editable(element))
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

        match element
            .get_control_type()
            .ok()
            .map(automation_control_type_label)
            .as_deref()
        {
            Some("edit") | Some("document") => true,
            _ => false,
        }
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

    fn desktop_paste_shortcut_env_value(shortcut: DesktopPasteShortcut) -> &'static str {
        match shortcut {
            DesktopPasteShortcut::ControlV => "ctrl_v",
            DesktopPasteShortcut::ControlShiftV => "ctrl_shift_v",
            DesktopPasteShortcut::ShiftInsert => "shift_insert",
        }
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

    fn begin_foreground_insert_target_restore(target_hwnd: HWND) {
        if target_hwnd.is_null() {
            return;
        }

        unsafe {
            if IsWindow(target_hwnd) == 0 {
                return;
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

        thread::sleep(Duration::from_millis(40));
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
        begin_foreground_insert_target_restore(target_hwnd);
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

        begin_foreground_insert_target_restore(target_hwnd);
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
    ) -> PostInsertForegroundHoldOutcome {
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
        let state = unsafe { get_window_state_mut(hwnd)? };
        if !state.hud_hwnd.is_null() {
            unsafe {
                KillTimer(hwnd, TIMER_HIDE_HUD);
                KillTimer(hwnd, TIMER_RECORDING_LEVEL);
                KillTimer(hwnd, TIMER_THINKING_PROGRESS);
                if let Ok(mut overlay) = overlay_ui_state().lock() {
                    overlay.hud_model = None;
                    overlay.hud_meter_bins = [0.0; 9];
                    overlay.hud_thinking_pulse_tick = 0;
                }
                ShowWindow(state.hud_hwnd, SW_HIDE);
            }
        }
        Ok(())
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
                let generation =
                    unsafe { get_window_state_mut(owner_hwnd) }
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
        let state = unsafe { get_window_state_mut(owner_hwnd)? };

        while state.copy_popup_pane_edit_hwnds.len() < pane_count {
            let index = state.copy_popup_pane_edit_hwnds.len();
            let edit_hwnd = create_copy_popup_edit_control(
                copy_popup_hwnd,
                dpi,
                copy_popup_edit_control_id(index),
            )?;
            state.copy_popup_pane_edit_hwnds.push(edit_hwnd);
        }

        for (index, edit_hwnd) in state.copy_popup_pane_edit_hwnds.iter().enumerate() {
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
        state.copy_popup_edit_hwnd = state.copy_popup_pane_edit_hwnds[default_index];
        Ok(())
    }

    fn show_copy_popup(hwnd: HWND, model: DesktopCopyPopupModel) -> Result<()> {
        let copy_popup_hwnd = {
            let state = unsafe { get_window_state_mut(hwnd)? };
            state.copy_popup_hwnd
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
            InvalidateRect(copy_popup_hwnd, ptr::null(), 1);
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
        let state = unsafe { get_window_state_mut(hwnd)? };
        if !state.copy_popup_hwnd.is_null() {
            if let Ok(mut overlay) = overlay_ui_state().lock() {
                overlay.copy_popup = None;
            }
            unsafe {
                for edit_hwnd in &state.copy_popup_pane_edit_hwnds {
                    if !edit_hwnd.is_null() {
                        ShowWindow(*edit_hwnd, SW_HIDE);
                    }
                }
                ShowWindow(state.copy_popup_hwnd, SW_HIDE);
            }
        }
        Ok(())
    }

    fn should_offer_shortcut_help(hwnd: HWND) -> bool {
        unsafe { get_window_state_mut(hwnd) }
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
        let state = unsafe { get_window_state_mut(hwnd) }?;
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
        let state = unsafe { get_window_state_mut(hwnd) }?;
        let context = capture_foreground_insert_target_context(hwnd, state.hud_hwnd);
        update_pending_hotkey_origin_insert_target(hwnd, context)
    }

    fn capture_pending_hotkey_origin_insert_target_from_hook(hwnd: HWND) {
        let context = unsafe { get_window_state_mut(hwnd) }
            .ok()
            .and_then(|state| capture_foreground_insert_target_context(hwnd, state.hud_hwnd));
        let _ = update_pending_hotkey_origin_insert_target(hwnd, context);
    }

    fn spawn_hotkey_origin_enrichment(hwnd: HWND, hud_hwnd: HWND, generation: u64) {
        let hwnd_value = hwnd as usize;
        let hud_hwnd_value = hud_hwnd as usize;
        thread::spawn(move || {
            for _ in 0..HOTKEY_ORIGIN_ENRICH_MAX_POLLS {
                thread::sleep(Duration::from_millis(HOTKEY_ORIGIN_ENRICH_POLL_INTERVAL_MS));

                let candidate_context = capture_foreground_insert_target_context(
                    hwnd_value as HWND,
                    hud_hwnd_value as HWND,
                );
                let should_stop = unsafe { get_window_state_mut(hwnd_value as HWND) }
                    .ok()
                    .and_then(|state| {
                        let mut shared = state.shared.lock().ok()?;
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

        let model = unsafe { get_window_state_mut(hwnd) }
            .ok()
            .and_then(|state| {
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
        let state = unsafe { get_window_state_mut(hwnd)? };
        if state.shortcut_help_hwnd.is_null() {
            anyhow::bail!("Talk desktop shortcut help window is unavailable");
        }

        let metrics = scale_shortcut_help_metrics_for_dpi(
            desktop_shortcut_help_metrics(),
            overlay_dpi_for_window(state.shortcut_help_hwnd),
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
                state.shortcut_help_hwnd,
                (-1isize) as HWND,
                position.x,
                position.y,
                metrics.width,
                metrics.height,
                SWP_NOACTIVATE,
            );
            apply_rounded_window_region(
                state.shortcut_help_hwnd,
                metrics.width,
                metrics.height,
                scale_desktop_overlay_length(
                    SHORTCUT_HELP_CORNER_RADIUS,
                    overlay_dpi_for_window(state.shortcut_help_hwnd),
                ),
            );
            InvalidateRect(state.shortcut_help_hwnd, ptr::null(), 1);
            match desktop_shortcut_help_activation_policy() {
                DesktopOverlayActivationPolicy::NoActivate => {
                    ShowWindow(state.shortcut_help_hwnd, SW_SHOWNOACTIVATE);
                }
                DesktopOverlayActivationPolicy::ActivateOnInteract => {
                    ShowWindow(state.shortcut_help_hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
        Ok(())
    }

    fn hide_shortcut_help(hwnd: HWND) -> Result<()> {
        let state = unsafe { get_window_state_mut(hwnd)? };
        if !state.shortcut_help_hwnd.is_null() {
            if let Ok(mut overlay) = overlay_ui_state().lock() {
                overlay.shortcut_help = None;
            }
            unsafe {
                ShowWindow(state.shortcut_help_hwnd, SW_HIDE);
            }
        }
        Ok(())
    }

    fn spawn_timeout_watcher(hwnd: HWND, generation: u64, max_recording_seconds: u64) {
        let hwnd_value = hwnd as usize;
        thread::spawn(move || {
            let hwnd = hwnd_value as HWND;
            thread::sleep(Duration::from_secs(max_recording_seconds));
            unsafe {
                let _ = PostMessageW(hwnd, STOP_MESSAGE, generation as usize, 0);
            }
        });
    }

    fn spawn_release_watcher(
        hwnd: HWND,
        generation: u64,
        hotkey: HotkeySpec,
        max_recording_seconds: u64,
    ) {
        let hwnd_value = hwnd as usize;
        thread::spawn(move || {
            let hwnd = hwnd_value as HWND;
            let deadline = Instant::now() + Duration::from_secs(max_recording_seconds);
            loop {
                if !hotkey.is_pressed() || Instant::now() >= deadline {
                    unsafe {
                        let _ = PostMessageW(hwnd, STOP_MESSAGE, generation as usize, 0);
                    }
                    break;
                }
                thread::sleep(Duration::from_millis(15));
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
                InvalidateRect(hwnd, ptr::null(), 1);
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
                InvalidateRect(hwnd, ptr::null(), 1);
            }
        }
    }

    fn desktop_overlay_rect_to_rect(rect: talk_desktop::DesktopOverlayRect) -> RECT {
        RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
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
            let state = unsafe { get_window_state_mut(owner_hwnd)? };
            (
                state.copy_popup_edit_hwnd,
                state.copy_popup_pane_edit_hwnds.clone(),
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

    unsafe fn measure_overlay_text_size(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        text: &str,
    ) -> SIZE {
        let wide = to_wide(text);
        let mut size = SIZE { cx: 0, cy: 0 };
        let text_len = wide.len().saturating_sub(1) as i32;
        if text_len > 0 {
            let _ = GetTextExtentPoint32W(hdc, wide.as_ptr(), text_len, &mut size);
        }
        size
    }

    unsafe fn draw_thinking_wave_text(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        text: &str,
        rect: RECT,
        dpi: u32,
        pulse_tick: u32,
    ) {
        let palette = desktop_hud_thinking_palette();
        let glyphs: Vec<String> = text.chars().map(|glyph| glyph.to_string()).collect();
        if glyphs.is_empty() {
            return;
        }

        let wave_offsets = desktop_hud_thinking_text_wave_offsets(glyphs.len(), pulse_tick);
        let letter_spacing = scale_desktop_overlay_length(1, dpi);
        let shadow_offset = scale_desktop_overlay_length(1, dpi).max(1);
        let mut glyph_widths = Vec::with_capacity(glyphs.len());
        let mut total_width = 0i32;
        let mut max_height = 0i32;

        for glyph in &glyphs {
            let size = measure_overlay_text_size(hdc, glyph);
            let width = size.cx.max(scale_desktop_overlay_length(6, dpi));
            let height = size.cy.max(scale_desktop_overlay_length(14, dpi));
            glyph_widths.push(width);
            total_width += width;
            max_height = max_height.max(height);
        }

        total_width += letter_spacing * (glyphs.len().saturating_sub(1) as i32);
        let mut current_x = rect.left + ((rect.right - rect.left - total_width).max(0) / 2);
        let base_y = rect.top + ((rect.bottom - rect.top - max_height).max(0) / 2);

        for ((glyph, width), wave_offset_px) in glyphs
            .iter()
            .zip(glyph_widths.iter())
            .zip(wave_offsets.into_iter())
        {
            let wave_direction = if wave_offset_px.is_negative() { -1 } else { 1 };
            let wave_magnitude = scale_desktop_overlay_length(i32::from(wave_offset_px.abs()), dpi);
            let y = base_y + (wave_direction * wave_magnitude);
            let wide = to_wide(glyph);
            let text_len = wide.len().saturating_sub(1) as i32;
            SetTextColor(hdc, rgb_triplet(palette.text_shadow_rgb));
            let _ = TextOutW(
                hdc,
                current_x + shadow_offset,
                y + shadow_offset,
                wide.as_ptr(),
                text_len,
            );
            SetTextColor(hdc, rgb_triplet(palette.text_rgb));
            let _ = TextOutW(hdc, current_x, y, wide.as_ptr(), text_len);
            current_x += *width + letter_spacing;
        }
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
        let state = unsafe { get_window_state_mut(owner_hwnd)? };
        if !state.copy_popup_edit_hwnd.is_null() {
            if let Some(text) = window_text(state.copy_popup_edit_hwnd) {
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
        if let Ok(state) = unsafe { get_window_state_mut(owner_hwnd) } {
            unsafe {
                let _ = SetForegroundWindow(state.copy_popup_hwnd);
                let _ = SetFocus(state.copy_popup_edit_hwnd);
            }
        }
        if desktop_copy_popup_copy_shows_follow_up_hud() {
            show_hud_text(
                owner_hwnd,
                &compose_hud_message("Talk: copied", Some("Copied latest transcript")),
                Some(1200),
            )?;
        }
        Ok(())
    }

    unsafe fn gradient_fill_client_rect(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        start_color: u32,
        end_color: u32,
        vertical: bool,
    ) {
        let vertices = [
            trivertex_at(rect.left, rect.top, start_color),
            trivertex_at(rect.right, rect.bottom, end_color),
        ];
        let gradient_rect = GRADIENT_RECT {
            UpperLeft: 0,
            LowerRight: 1,
        };
        let _ = GradientFill(
            hdc,
            vertices.as_ptr(),
            vertices.len() as u32,
            (&gradient_rect as *const GRADIENT_RECT).cast(),
            1,
            if vertical {
                GRADIENT_FILL_RECT_V
            } else {
                GRADIENT_FILL_RECT_H
            },
        );
    }

    unsafe fn draw_terminal_shell_chrome(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        accent_color: u32,
        dpi: u32,
        corner_radius: i32,
    ) {
        gradient_fill_client_rect(
            hdc,
            rect,
            terminal_shell_top_color(),
            terminal_shell_bottom_color(),
            true,
        );

        let grid_pen = CreatePen(PS_SOLID, 1, terminal_grid_color());
        let old_pen = SelectObject(hdc, grid_pen as _);
        let step_y = scale_desktop_overlay_length(16, dpi).max(12);
        let step_x = scale_desktop_overlay_length(24, dpi).max(20);
        let grid_inset = scale_desktop_overlay_length(14, dpi).max(10);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let mut y = rect.top + grid_inset;
        while y < rect.bottom - grid_inset {
            let _ = MoveToEx(hdc, rect.left + grid_inset, y, ptr::null_mut());
            let _ = LineTo(hdc, rect.right - grid_inset, y);
            y += step_y;
        }
        let mut x = rect.left + grid_inset;
        while x < rect.right - grid_inset {
            let _ = MoveToEx(hdc, x, rect.top + grid_inset, ptr::null_mut());
            let _ = LineTo(hdc, x, rect.bottom - grid_inset);
            x += step_x;
        }

        let _ = SelectObject(hdc, old_pen);
        DeleteObject(grid_pen as _);

        let border_pen = CreatePen(PS_SOLID, 1, terminal_border_color());
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
        DeleteObject(border_pen as _);

        let accent_brush = CreateSolidBrush(accent_color);
        let support_brush = CreateSolidBrush(terminal_secondary_accent_color());
        let old_brush = SelectObject(hdc, accent_brush as _);
        let old_pen = SelectObject(hdc, GetStockObject(HOLLOW_BRUSH) as _);
        let rail_height = scale_desktop_overlay_length(4, dpi).max(3);
        let rail_left = rect.left + scale_desktop_overlay_length(18, dpi);
        let rail_top = rect.top + scale_desktop_overlay_length(12, dpi);
        let rail_width = (width / 3)
            .min(scale_desktop_overlay_length(96, dpi))
            .max(48);
        RoundRect(
            hdc,
            rail_left,
            rail_top,
            rail_left + rail_width,
            rail_top + rail_height,
            rail_height,
            rail_height,
        );
        let _ = SelectObject(hdc, support_brush as _);
        let support_width = scale_desktop_overlay_length(40, dpi).max(28);
        RoundRect(
            hdc,
            rect.right - scale_desktop_overlay_length(18, dpi) - support_width,
            rail_top,
            rect.right - scale_desktop_overlay_length(18, dpi),
            rail_top + rail_height,
            rail_height,
            rail_height,
        );
        let _ = SelectObject(hdc, old_pen);
        let _ = SelectObject(hdc, old_brush);
        DeleteObject(accent_brush as _);
        DeleteObject(support_brush as _);

        let corner_pen = CreatePen(PS_SOLID, 1, accent_color);
        let old_pen = SelectObject(hdc, corner_pen as _);
        let corner_len = scale_desktop_overlay_length(18, dpi).max(12);
        let corner_left = rect.left + scale_desktop_overlay_length(18, dpi);
        let corner_top = rect.top + scale_desktop_overlay_length(18, dpi);
        let _ = MoveToEx(hdc, corner_left, corner_top + corner_len, ptr::null_mut());
        let _ = LineTo(hdc, corner_left, corner_top);
        let _ = LineTo(hdc, corner_left + corner_len, corner_top);
        let _ = SelectObject(hdc, old_pen);
        DeleteObject(corner_pen as _);

        let _ = width;
        let _ = height;
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
        let brush = CreateSolidBrush(fill_color);
        let pen = CreatePen(PS_SOLID, 1, border_color);
        let old_brush = SelectObject(hdc, brush as _);
        let old_pen = SelectObject(hdc, pen as _);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 0, 0);
        let _ = SelectObject(hdc, font as _);
        SetTextColor(hdc, text_color);
        let mut text_rect = rect;
        DrawTextW(
            hdc,
            to_wide(text).as_ptr(),
            -1,
            &mut text_rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        let _ = SelectObject(hdc, old_pen);
        let _ = SelectObject(hdc, old_brush);
        DeleteObject(brush as _);
        DeleteObject(pen as _);
    }

    unsafe fn draw_terminal_thinking_indicator(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        dpi: u32,
        accent_color: u32,
    ) {
        let shell_brush = CreateSolidBrush(terminal_panel_fill_color());
        let border_pen = CreatePen(PS_SOLID, 1, terminal_border_color());
        let old_brush = SelectObject(hdc, shell_brush as _);
        let old_pen = SelectObject(hdc, border_pen as _);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 0, 0);

        let pulse_brush = CreateSolidBrush(accent_color);
        let _ = SelectObject(hdc, pulse_brush as _);
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
        DeleteObject(shell_brush as _);
        DeleteObject(border_pen as _);
        DeleteObject(pulse_brush as _);
    }

    unsafe fn paint_hud_window(hwnd: HWND) {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        if hdc.is_null() {
            return;
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect);
        let (model, thinking_pulse_tick) = overlay_ui_state()
            .lock()
            .ok()
            .map(|overlay| {
                (
                    overlay.hud_model.clone().unwrap_or_else(|| {
                        desktop_hud_view_model_for_phase(RuntimePhase::Recording)
                    }),
                    overlay.hud_thinking_pulse_tick,
                )
            })
            .unwrap_or_else(|| (desktop_hud_view_model_for_phase(RuntimePhase::Recording), 0));

        let dpi = overlay_dpi_for_window(hwnd);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let title_font = create_overlay_font(dpi, 15, FW_BOLD as i32);
        let badge_font = create_overlay_font(dpi, 9, FW_BOLD as i32);
        let icon_font = create_overlay_font(dpi, 13, FW_BOLD as i32);
        let accent_color = accent_fill_color(model.visual_state);
        let old_font = SelectObject(hdc, title_font as _);
        SetBkMode(hdc, TRANSPARENT as i32);

        if model.visual_state == DesktopHudVisualState::Listening {
            let shell_brush = CreateSolidBrush(listening_shell_color());
            let border_pen = CreatePen(PS_SOLID, 1, listening_shell_border_color());
            let old_brush = SelectObject(hdc, shell_brush as _);
            let old_pen = SelectObject(hdc, border_pen as _);
            RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 0, 0);
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);
            DeleteObject(shell_brush as _);
            DeleteObject(border_pen as _);

            let cancel_rect = desktop_listening_hud_cancel_button_rect(width, height, dpi);
            let confirm_rect = desktop_listening_hud_complete_button_rect(width, height, dpi);
            let mut waveform_rect = desktop_listening_hud_waveform_rect(width, height, dpi);

            let cancel_brush = CreateSolidBrush(listening_cancel_fill_color());
            let cancel_pen = CreatePen(PS_SOLID, 1, listening_cancel_border_color());
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
            DeleteObject(cancel_brush as _);
            DeleteObject(cancel_pen as _);

            let confirm_brush = CreateSolidBrush(listening_confirm_fill_color());
            let confirm_pen = CreatePen(PS_SOLID, 1, listening_confirm_border_color());
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
            DeleteObject(confirm_brush as _);
            DeleteObject(confirm_pen as _);

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

            if let Some(partial_text) = model.detail.as_deref() {
                if let Some(partial_layout) = desktop_listening_hud_partial_text_layout(
                    width,
                    height,
                    dpi,
                    Some(partial_text),
                ) {
                    let partial_font = create_overlay_font(dpi, 9, FW_BOLD as i32);
                    SelectObject(hdc, partial_font as _);
                    SetTextColor(hdc, terminal_text_soft_color());
                    let mut partial_rect = RECT {
                        left: partial_layout.text_rect.left,
                        top: partial_layout.text_rect.top,
                        right: partial_layout.text_rect.right,
                        bottom: partial_layout.text_rect.bottom,
                    };
                    let draw_flags = if partial_layout.wraps_text {
                        DT_CENTER | DT_WORDBREAK | DT_EDITCONTROL | DT_NOPREFIX
                    } else {
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX
                    };
                    let visible_partial_text =
                        desktop_listening_hud_visible_partial_text(partial_text, &partial_layout);
                    DrawTextW(
                        hdc,
                        to_wide(&visible_partial_text).as_ptr(),
                        -1,
                        &mut partial_rect,
                        draw_flags,
                    );
                    if let Some(scrollbar_rect) = partial_layout.scrollbar_rect {
                        let track_brush = CreateSolidBrush(listening_shell_border_color());
                        let thumb_brush = CreateSolidBrush(terminal_text_soft_color());
                        let old_brush = SelectObject(hdc, track_brush as _);
                        RoundRect(
                            hdc,
                            scrollbar_rect.left,
                            scrollbar_rect.top,
                            scrollbar_rect.right,
                            scrollbar_rect.bottom,
                            0,
                            0,
                        );
                        let _ = SelectObject(hdc, old_brush);

                        let thumb_height = scale_desktop_overlay_length(18, dpi)
                            .max(8)
                            .min(scrollbar_rect.bottom - scrollbar_rect.top);
                        let thumb_top = scrollbar_rect.bottom - thumb_height;
                        let old_brush = SelectObject(hdc, thumb_brush as _);
                        RoundRect(
                            hdc,
                            scrollbar_rect.left,
                            thumb_top,
                            scrollbar_rect.right,
                            scrollbar_rect.bottom,
                            0,
                            0,
                        );
                        let _ = SelectObject(hdc, old_brush);
                        DeleteObject(track_brush as _);
                        DeleteObject(thumb_brush as _);
                    }
                    waveform_rect = partial_layout.waveform_rect;
                    DeleteObject(partial_font as _);
                }
            }

            if let Some(meter) = model.meter.as_ref() {
                let bar_brush = CreateSolidBrush(listening_waveform_color());
                let bar_pen = CreatePen(PS_SOLID, 1, listening_waveform_color());
                let old_brush = SelectObject(hdc, bar_brush as _);
                let old_pen = SelectObject(hdc, bar_pen as _);
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
                    let bottom = top + scaled_height;
                    RoundRect(hdc, x, top, x + bar_width, bottom, 0, 0);
                }
                let _ = SelectObject(hdc, old_pen);
                let _ = SelectObject(hdc, old_brush);
                DeleteObject(bar_brush as _);
                DeleteObject(bar_pen as _);
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
            gradient_fill_client_rect(
                hdc,
                progress_track_rect,
                rgb_triplet(palette.track_start_rgb),
                rgb_triplet(palette.track_end_rgb),
                true,
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
                gradient_fill_client_rect(
                    hdc,
                    fill_rect,
                    rgb_triplet(palette.fill_start_rgb),
                    rgb_triplet(palette.fill_end_rgb),
                    false,
                );

                let head_width = scale_desktop_overlay_length(8, dpi).max(4).min(fill_width);
                let head_rect = RECT {
                    left: fill_rect.right - head_width,
                    top: fill_rect.top,
                    right: fill_rect.right,
                    bottom: fill_rect.bottom,
                };
                gradient_fill_client_rect(
                    hdc,
                    head_rect,
                    rgb_triplet(palette.fill_end_rgb),
                    rgb_triplet(palette.fill_head_rgb),
                    false,
                );
            }

            let border_pen = CreatePen(PS_SOLID, 1, rgb_triplet(palette.border_rgb));
            let old_pen = SelectObject(hdc, border_pen as _);
            let old_brush = SelectObject(hdc, GetStockObject(HOLLOW_BRUSH) as _);
            RoundRect(
                hdc,
                progress_track_rect.left,
                progress_track_rect.top,
                progress_track_rect.right,
                progress_track_rect.bottom,
                0,
                0,
            );
            let _ = SelectObject(hdc, old_brush);
            let _ = SelectObject(hdc, old_pen);
            DeleteObject(border_pen as _);

            if fill_width > 0 {
                let edge_width = scale_desktop_overlay_length(1, dpi).max(1).min(fill_width);
                let edge_pen = CreatePen(PS_SOLID, edge_width, rgb_triplet(palette.fill_head_rgb));
                let old_pen = SelectObject(hdc, edge_pen as _);
                let edge_x = progress_track_rect.left + fill_width - (edge_width / 2);
                let _ = MoveToEx(hdc, edge_x, progress_track_rect.top, ptr::null_mut());
                let _ = LineTo(hdc, edge_x, progress_track_rect.bottom);
                let _ = SelectObject(hdc, old_pen);
                DeleteObject(edge_pen as _);
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
        }

        SelectObject(hdc, old_font);
        DeleteObject(title_font as _);
        DeleteObject(badge_font as _);
        DeleteObject(icon_font as _);
        EndPaint(hwnd, &paint);
    }

    unsafe fn paint_copy_popup_window(hwnd: HWND) {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        if hdc.is_null() {
            return;
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect);
        let popup = overlay_ui_state()
            .lock()
            .ok()
            .and_then(|overlay| overlay.copy_popup.clone());
        let dpi = overlay_dpi_for_window(hwnd);
        let title_font = create_overlay_font(dpi, 15, FW_BOLD as i32);
        let label_font = create_overlay_font(dpi, 10, FW_BOLD as i32);
        let button_font = create_overlay_font(dpi, 12, FW_BOLD as i32);
        let copy_button_rect = copy_popup_copy_button_rect(hwnd, dpi);
        let close_button_rect = copy_popup_close_button_rect(hwnd, dpi);
        let editor_frame_rect = copy_popup_editor_frame_rect(hwnd, dpi);
        let old_font = SelectObject(hdc, title_font as _);
        SetBkMode(hdc, TRANSPARENT as i32);
        let shell_brush = CreateSolidBrush(typeless_popup_fill_color());
        let shell_pen = CreatePen(PS_SOLID, 1, typeless_popup_border_color());
        let old_brush = SelectObject(hdc, shell_brush as _);
        let old_pen = SelectObject(hdc, shell_pen as _);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 0, 0);
        let _ = SelectObject(hdc, old_pen);
        let _ = SelectObject(hdc, old_brush);
        DeleteObject(shell_brush as _);
        DeleteObject(shell_pen as _);

        if let Some(popup) = popup {
            let close_fill = if popup.hovered_control == CopyPopupHoveredControl::Close {
                typeless_popup_close_button_hover_fill_color()
            } else {
                typeless_popup_close_button_fill_color()
            };
            let close_border = if popup.hovered_control == CopyPopupHoveredControl::Close {
                typeless_popup_close_button_hover_border_color()
            } else {
                typeless_popup_close_button_border_color()
            };
            let close_brush = CreateSolidBrush(close_fill);
            let close_pen = CreatePen(PS_SOLID, 1, close_border);
            let old_brush = SelectObject(hdc, close_brush as _);
            let old_pen = SelectObject(hdc, close_pen as _);
            RoundRect(
                hdc,
                close_button_rect.left,
                close_button_rect.top,
                close_button_rect.right,
                close_button_rect.bottom,
                0,
                0,
            );
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);
            DeleteObject(close_brush as _);
            DeleteObject(close_pen as _);

            SelectObject(hdc, title_font as _);
            SetTextColor(
                hdc,
                if popup.hovered_control == CopyPopupHoveredControl::Close {
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

            let editor_brush = CreateSolidBrush(typeless_popup_editor_fill_color());
            let editor_pen = CreatePen(PS_SOLID, 1, typeless_popup_editor_border_color());
            let old_brush = SelectObject(hdc, editor_brush as _);
            let old_pen = SelectObject(hdc, editor_pen as _);
            RoundRect(
                hdc,
                editor_frame_rect.left,
                editor_frame_rect.top,
                editor_frame_rect.right,
                editor_frame_rect.bottom,
                0,
                0,
            );
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);
            DeleteObject(editor_brush as _);
            DeleteObject(editor_pen as _);

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

            let copy_fill = if popup.hovered_control == CopyPopupHoveredControl::Copy {
                typeless_popup_button_hover_fill_color()
            } else {
                typeless_popup_button_fill_color()
            };
            let copy_border = if popup.hovered_control == CopyPopupHoveredControl::Copy {
                typeless_popup_button_hover_border_color()
            } else {
                typeless_popup_button_border_color()
            };
            let button_brush = CreateSolidBrush(copy_fill);
            let button_pen = CreatePen(PS_SOLID, 1, copy_border);
            let old_brush = SelectObject(hdc, button_brush as _);
            let old_pen = SelectObject(hdc, button_pen as _);
            RoundRect(
                hdc,
                copy_button_rect.left,
                copy_button_rect.top,
                copy_button_rect.right,
                copy_button_rect.bottom,
                0,
                0,
            );
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);
            DeleteObject(button_brush as _);
            DeleteObject(button_pen as _);

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
        }

        SelectObject(hdc, old_font);
        DeleteObject(title_font as _);
        DeleteObject(label_font as _);
        DeleteObject(button_font as _);
        EndPaint(hwnd, &paint);
    }

    unsafe fn paint_shortcut_help_window(hwnd: HWND) {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        if hdc.is_null() {
            return;
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect);
        let model = overlay_ui_state()
            .lock()
            .ok()
            .and_then(|overlay| overlay.shortcut_help.clone());
        let dpi = overlay_dpi_for_window(hwnd);
        let title_font = create_overlay_font(dpi, 15, FW_BOLD as i32);
        let row_title_font = create_overlay_font(dpi, 12, FW_BOLD as i32);
        let pill_font = create_overlay_font(dpi, 11, FW_BOLD as i32);
        let old_font = SelectObject(hdc, title_font as _);
        SetBkMode(hdc, TRANSPARENT as i32);
        draw_terminal_shell_chrome(hdc, rect, terminal_signal_color(), dpi, 0);

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

                gradient_fill_client_rect(
                    hdc,
                    row_rect,
                    terminal_panel_light_color(),
                    terminal_panel_fill_color(),
                    true,
                );
                let row_pen = CreatePen(PS_SOLID, 1, terminal_border_color());
                let old_pen = SelectObject(hdc, row_pen as _);
                let old_brush = SelectObject(hdc, GetStockObject(HOLLOW_BRUSH) as _);
                RoundRect(
                    hdc,
                    row_rect.left,
                    row_rect.top,
                    row_rect.right,
                    row_rect.bottom,
                    0,
                    0,
                );
                let _ = SelectObject(hdc, old_brush);
                let _ = SelectObject(hdc, old_pen);
                DeleteObject(row_pen as _);

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
        DeleteObject(title_font as _);
        DeleteObject(row_title_font as _);
        DeleteObject(pill_font as _);
        EndPaint(hwnd, &paint);
    }

    fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }

    fn rgb_triplet(color: [u8; 3]) -> u32 {
        rgb(color[0], color[1], color[2])
    }

    fn copy_popup_edit_brush() -> isize {
        static BRUSH: OnceLock<isize> = OnceLock::new();
        *BRUSH.get_or_init(|| unsafe {
            CreateSolidBrush(typeless_popup_editor_fill_color()) as isize
        })
    }

    fn trivertex_at(x: i32, y: i32, color: u32) -> TRIVERTEX {
        TRIVERTEX {
            x,
            y,
            Red: ((color & 0xFF) as u16) << 8,
            Green: (((color >> 8) & 0xFF) as u16) << 8,
            Blue: (((color >> 16) & 0xFF) as u16) << 8,
            Alpha: 0,
        }
    }

    fn terminal_signal_color() -> u32 {
        rgb(217, 255, 56)
    }

    fn terminal_signal_border_color() -> u32 {
        rgb(177, 207, 34)
    }

    fn terminal_secondary_accent_color() -> u32 {
        rgb(34, 197, 94)
    }

    fn terminal_shell_top_color() -> u32 {
        rgb(11, 14, 18)
    }

    fn terminal_shell_bottom_color() -> u32 {
        rgb(15, 19, 24)
    }

    fn terminal_panel_fill_color() -> u32 {
        rgb(11, 14, 18)
    }

    fn terminal_panel_light_color() -> u32 {
        rgb(20, 24, 30)
    }

    fn terminal_border_color() -> u32 {
        rgb(42, 48, 55)
    }

    fn terminal_grid_color() -> u32 {
        rgb(26, 30, 36)
    }

    fn terminal_text_color() -> u32 {
        rgb(247, 252, 230)
    }

    fn terminal_text_soft_color() -> u32 {
        rgb(193, 197, 179)
    }

    fn listening_shell_color() -> u32 {
        rgb(22, 25, 30)
    }

    fn listening_shell_border_color() -> u32 {
        rgb(58, 64, 72)
    }

    fn listening_cancel_fill_color() -> u32 {
        rgb(24, 29, 35)
    }

    fn listening_cancel_border_color() -> u32 {
        rgb(52, 59, 68)
    }

    fn listening_cancel_glyph_color() -> u32 {
        rgb(242, 244, 248)
    }

    fn listening_confirm_fill_color() -> u32 {
        rgb(249, 250, 252)
    }

    fn listening_confirm_border_color() -> u32 {
        rgb(214, 218, 224)
    }

    fn listening_confirm_glyph_color() -> u32 {
        rgb(20, 24, 29)
    }

    fn listening_waveform_color() -> u32 {
        rgb(244, 246, 250)
    }

    fn typeless_popup_fill_color() -> u32 {
        rgb(33, 27, 27)
    }

    fn typeless_popup_border_color() -> u32 {
        rgb(70, 63, 63)
    }

    fn typeless_popup_editor_fill_color() -> u32 {
        rgb(35, 39, 47)
    }

    fn typeless_popup_editor_border_color() -> u32 {
        rgb(86, 92, 101)
    }

    fn typeless_popup_editor_text_color() -> u32 {
        rgb(240, 243, 248)
    }

    fn typeless_popup_close_color() -> u32 {
        rgb(146, 139, 145)
    }

    fn typeless_popup_close_hover_color() -> u32 {
        rgb(240, 243, 248)
    }

    fn typeless_popup_close_button_fill_color() -> u32 {
        rgb(49, 44, 49)
    }

    fn typeless_popup_close_button_border_color() -> u32 {
        rgb(88, 82, 88)
    }

    fn typeless_popup_close_button_hover_fill_color() -> u32 {
        rgb(68, 61, 68)
    }

    fn typeless_popup_close_button_hover_border_color() -> u32 {
        rgb(118, 111, 118)
    }

    fn typeless_popup_button_fill_color() -> u32 {
        rgb(71, 66, 72)
    }

    fn typeless_popup_button_border_color() -> u32 {
        rgb(92, 86, 94)
    }

    fn typeless_popup_button_hover_fill_color() -> u32 {
        rgb(92, 86, 96)
    }

    fn typeless_popup_button_hover_border_color() -> u32 {
        rgb(126, 118, 130)
    }

    fn typeless_popup_button_text_color() -> u32 {
        rgb(246, 247, 250)
    }

    fn accent_fill_color(state: DesktopHudVisualState) -> u32 {
        match state {
            DesktopHudVisualState::Listening => terminal_signal_color(),
            DesktopHudVisualState::Thinking => terminal_secondary_accent_color(),
            DesktopHudVisualState::Success => rgb(34, 197, 94),
            DesktopHudVisualState::Error => rgb(239, 68, 68),
            DesktopHudVisualState::Cancelled => rgb(116, 128, 145),
            DesktopHudVisualState::Informational => terminal_signal_color(),
        }
    }

    fn accent_outline_color(state: DesktopHudVisualState) -> u32 {
        match state {
            DesktopHudVisualState::Listening => terminal_signal_border_color(),
            DesktopHudVisualState::Thinking => rgb(27, 150, 73),
            DesktopHudVisualState::Success => rgb(27, 150, 73),
            DesktopHudVisualState::Error => rgb(201, 54, 54),
            DesktopHudVisualState::Cancelled => rgb(86, 98, 114),
            DesktopHudVisualState::Informational => terminal_signal_border_color(),
        }
    }

    fn accent_badge_text_color(state: DesktopHudVisualState) -> u32 {
        match state {
            DesktopHudVisualState::Listening => rgb(12, 14, 18),
            DesktopHudVisualState::Thinking => rgb(9, 18, 22),
            DesktopHudVisualState::Success => rgb(10, 20, 14),
            DesktopHudVisualState::Error
            | DesktopHudVisualState::Cancelled
            | DesktopHudVisualState::Informational => terminal_text_color(),
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
                            capture_pending_hotkey_origin_insert_target_from_hook(
                                *hwnd_value as HWND,
                            );
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
                                capture_pending_hotkey_origin_insert_target_from_hook(
                                    *hwnd_value as HWND,
                                );
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
                                capture_pending_hotkey_origin_insert_target_from_hook(
                                    *hwnd_value as HWND,
                                );
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

        if consume {
            1
        } else {
            CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
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
