use std::fs;
use std::path::PathBuf;
use talk_core::NativeReadinessStatus;
use talk_core::{
    AudioBackendMode, AudioConfig, ClipboardBackendMode, DesktopConfig, DesktopPasteShortcut,
    DesktopPasteShortcutOverride, DesktopShortcutConfig, LoggingConfig,
    OpenAiTranscriptionTransport, OutputConfig, OutputMode, ProviderConfig, ProviderKind,
    SpeculativeLocalAsrDaemonConfig, SpeculativeLocalAsrDaemonMode,
    SpeculativeSherpaOnlineModelFamily, TalkConfig, TriggerConfig, TriggerMode, VoiceMode,
};
use talk_desktop::{
    build_desktop_insert_target_diagnostic, build_desktop_insert_target_diagnostic_with_trace,
    build_desktop_insert_target_trace_diagnostic, build_status_report, compose_hud_message,
    config_status_message, decide_desktop_output_strategy, decide_speculative_patch_application,
    desktop_action_bindings, desktop_copy_popup_action_for_virtual_key,
    desktop_copy_popup_activation_policy, desktop_copy_popup_close_button_rect,
    desktop_copy_popup_copy_button_rect, desktop_copy_popup_copy_shows_follow_up_hud,
    desktop_copy_popup_editor_content_rect, desktop_copy_popup_editor_frame_rect,
    desktop_copy_popup_metrics, desktop_copy_popup_model,
    desktop_copy_popup_model_for_mode_text_result, desktop_copy_popup_pane_layouts,
    desktop_copy_popup_position, desktop_document_recorrection_decision,
    desktop_document_recorrection_session_decision, desktop_effective_streaming_asr_enabled,
    desktop_hud_activation_policy, desktop_hud_audio_meter_model,
    desktop_hud_audio_meter_model_for_waveform, desktop_hud_metrics_for_view_model,
    desktop_hud_presentation_for_phase, desktop_hud_thinking_palette,
    desktop_hud_thinking_progress_model, desktop_hud_thinking_text_wave_offsets,
    desktop_hud_view_model_for_listening_level,
    desktop_hud_view_model_for_listening_waveform_with_partial, desktop_hud_view_model_for_phase,
    desktop_insert_target_diagnostic_path, desktop_insert_target_restore_requested,
    desktop_listening_hud_action_for_point, desktop_listening_hud_cancel_button_rect,
    desktop_listening_hud_complete_button_rect, desktop_listening_hud_partial_text_layout,
    desktop_listening_hud_visible_partial_text, desktop_mode_dropdown_model,
    desktop_mode_output_policy, desktop_mode_text_pane_layout, desktop_mode_text_result_model,
    desktop_mode_text_result_popup_text, desktop_output_plan, desktop_overlay_scale_factor_for_dpi,
    desktop_packaged_local_asr_daemon_launch_plan,
    desktop_packaged_local_asr_daemon_launch_plan_with_config,
    desktop_preferred_paste_shortcut_for_process_name, desktop_preferred_paste_shortcut_for_target,
    desktop_product_local_asr_daemon_launch_plan_with_config,
    desktop_runtime_insert_directive_for_mode, desktop_shortcut_help_activation_policy,
    desktop_shortcut_help_metrics, desktop_shortcut_help_model, desktop_shortcut_help_position,
    desktop_speculative_cloud_correction_enabled, desktop_speculative_correction_job_model,
    desktop_speculative_local_asr_route, desktop_speculative_pipeline_enabled,
    desktop_speculative_replacement_selection_count, desktop_speculative_transcript_view_model,
    desktop_streaming_hud_transcript, desktop_streaming_latest_segment_allows_auto_patch,
    desktop_streaming_stop_policy, desktop_streaming_stop_tail_text,
    desktop_text_lifecycle_view_model, foreground_target_refresh_requested,
    foreground_target_stability_satisfied, hotkey_status_message, hud_message_for_phase,
    hydrate_foreground_insert_target_focus, idle_status_detail, live_streaming_local_segment_plan,
    live_streaming_segment_plan_for_lifecycle, native_status_message,
    observe_foreground_target_stability, parse_desktop_window_handle, parse_hotkey,
    recording_stop_watcher_policy, resolve_default_desktop_config_path,
    resolve_desktop_audio_file_override, resolve_foreground_focus_capture,
    resolve_foreground_focus_handle, resolve_hotkey_origin_insert_target,
    resolve_hotkey_recording_origin_enrichment, resolve_pending_hotkey_origin_capture,
    scale_desktop_overlay_length, select_foreground_insert_target,
    select_windows_hotkey_binding_strategy, tray_menu_model,
    windows_hotkey_binding_registration_plan, ConfigAvailability, DesktopCopyPopupAction,
    DesktopCopyPopupMetrics, DesktopCopyPopupModel, DesktopCopyPopupPaneModel,
    DesktopDocumentRecorrectionDecision, DesktopHudMetrics, DesktopHudPresentation,
    DesktopHudVisualState, DesktopInsertTargetContext, DesktopInsertTargetRestoreDiagnostic,
    DesktopListeningHudAction, DesktopLiveStreamingLocalSegmentPlan,
    DesktopLocalAsrDaemonLaunchPlan, DesktopModeDropdownEntry, DesktopModeDropdownModel,
    DesktopModeOutputPolicy, DesktopModeTextPane, DesktopModeTextPaneLayout,
    DesktopModeTextResultModel, DesktopOutputPlan, DesktopOutputStrategy,
    DesktopOverlayActivationPolicy, DesktopOverlayPosition, DesktopOverlayRect,
    DesktopRecordingStopWatcherPolicy, DesktopRuntimeInsertDirective, DesktopRuntimeInsertPlan,
    DesktopShortcutHelpEntry, DesktopShortcutHelpMetrics, DesktopShortcutHelpModel,
    DesktopSpeculativeCorrectionJobModel, DesktopSpeculativeCorrectionOutputTarget,
    DesktopSpeculativeLocalAsrRoute, DesktopSpeculativePipelineConfig,
    DesktopSpeculativeTranscriptState, DesktopStreamingStopPolicy, DesktopTextLifecycleState,
    DesktopTextLifecycleViewModel, ForegroundFocusCaptureSource, ForegroundInsertTarget,
    ForegroundTargetReleaseReason, ForegroundTargetStabilityProgress, HotkeyBindingState,
    LastSessionStatus, LowLevelHotkeyTracker, LowLevelHotkeyTransition, NativeBackendSnapshot,
    NativeReadinessSnapshot, ShellState, SpeculativeInsertAnchor, SpeculativePatchApplication,
    SpeculativePatchCandidate, StatusSnapshot, ToggleDesktopHotkeyRouter,
    ToggleDesktopHotkeyRouterPendingHold, WindowsHotkeyBindingRegistrationPlan,
    WindowsHotkeyBindingStrategy,
};
use talk_runtime::{RuntimePhase, SpeculativeRuntimeEvent};

#[test]
fn hud_text_maps_runtime_phases_to_short_openless_style_messages() {
    assert_eq!(
        hud_message_for_phase(RuntimePhase::Recording),
        "Talk: listening"
    );
    assert_eq!(
        hud_message_for_phase(RuntimePhase::Transcribing),
        "Talk: transcribing"
    );
    assert_eq!(hud_message_for_phase(RuntimePhase::Completed), "Talk: done");
    assert_eq!(hud_message_for_phase(RuntimePhase::Failed), "Talk: failed");
}

#[test]
fn speculative_patch_applies_when_anchor_matches_and_edit_is_small() {
    let anchor =
        SpeculativeInsertAnchor::new(100, Some(200), "seg-1", "我下午三点有空", 1_000).unwrap();
    let candidate =
        SpeculativePatchCandidate::new(100, Some(200), "seg-1", "我下午三点有空。", 1_400).unwrap();
    assert_eq!(
        decide_speculative_patch_application(&anchor, &candidate, 2_000, 0.25),
        SpeculativePatchApplication::Apply
    );
}

#[test]
fn speculative_patch_defers_when_focus_changed() {
    let anchor =
        SpeculativeInsertAnchor::new(100, Some(200), "seg-1", "我下午三点有空", 1_000).unwrap();
    let candidate =
        SpeculativePatchCandidate::new(100, Some(201), "seg-1", "我下午三点有空。", 1_400).unwrap();
    assert_eq!(
        decide_speculative_patch_application(&anchor, &candidate, 2_000, 0.25),
        SpeculativePatchApplication::DeferToPopup
    );
}

#[test]
fn speculative_hud_marks_partial_text_as_draft() {
    let model = desktop_speculative_transcript_view_model(
        DesktopSpeculativeTranscriptState::Partial,
        "你好",
    );
    assert_eq!(model.text, "你好");
    assert_eq!(model.opacity_percent, 62);
    assert!(!model.show_cloud_corrected_mark);
}

#[test]
fn speculative_hud_marks_cloud_corrected_text_as_stable() {
    let model = desktop_speculative_transcript_view_model(
        DesktopSpeculativeTranscriptState::CloudCorrected,
        "你好呀。",
    );
    assert_eq!(model.text, "你好呀。");
    assert_eq!(model.opacity_percent, 100);
    assert!(model.show_cloud_corrected_mark);
}

#[test]
fn streaming_hud_accumulates_committed_segments_and_current_partial() {
    assert_eq!(
        desktop_streaming_hud_transcript(
            &[("seg-1", "你好。"), ("seg-2", "今天")],
            Some(("seg-3", "我们继续"))
        ),
        "你好。今天我们继续"
    );
}

#[test]
fn streaming_hud_replaces_partial_for_same_segment_instead_of_duplication() {
    assert_eq!(
        desktop_streaming_hud_transcript(
            &[("seg-1", "你好。"), ("seg-2", "今天")],
            Some(("seg-2", "今天我们继续"))
        ),
        "你好。今天我们继续"
    );
}

#[test]
fn streaming_hud_keeps_virtual_tail_segments_in_order_after_cumulative_asr_split() {
    assert_eq!(
        desktop_streaming_hud_transcript(
            &[
                ("seg-1", "第一句。"),
                ("seg-1#2", "第二句。"),
                ("seg-1#3", "第三句。")
            ],
            None
        ),
        "第一句。第二句。第三句。"
    );
}

#[test]
fn streaming_hud_skips_blank_segments_before_falling_back_to_placeholder() {
    assert_eq!(
        desktop_streaming_hud_transcript(&[("seg-1", "   ")], Some(("seg-2", ""))),
        ""
    );
}

#[test]
fn desktop_speculative_pipeline_is_disabled_by_default() {
    assert!(!desktop_speculative_pipeline_enabled(
        &DesktopSpeculativePipelineConfig::default()
    ));
}

#[test]
fn desktop_speculative_pipeline_enables_only_when_local_asr_is_configured() {
    let config = DesktopSpeculativePipelineConfig {
        enabled: true,
        local_asr: "mock".to_string(),
        cloud_correction: "disabled".to_string(),
    };
    assert!(desktop_speculative_pipeline_enabled(&config));
}

#[test]
fn desktop_speculative_cloud_correction_requires_provider_text_processor_mode() {
    let config = DesktopSpeculativePipelineConfig {
        enabled: true,
        local_asr: "external_command".to_string(),
        cloud_correction: "provider_text_processor".to_string(),
    };

    assert!(desktop_speculative_cloud_correction_enabled(&config));

    let disabled = DesktopSpeculativePipelineConfig {
        cloud_correction: "disabled".to_string(),
        ..config
    };
    assert!(!desktop_speculative_cloud_correction_enabled(&disabled));
}

#[test]
fn text_lifecycle_marks_audio_and_pre_recognition_as_not_insertable_until_corrected() {
    assert_eq!(
        desktop_text_lifecycle_view_model(DesktopTextLifecycleState::AudioWave, "你好"),
        DesktopTextLifecycleViewModel {
            text: None,
            text_rgb: None,
            insertable_to_target: false,
        }
    );
    assert_eq!(
        desktop_text_lifecycle_view_model(DesktopTextLifecycleState::PreRecognized, "你好"),
        DesktopTextLifecycleViewModel {
            text: Some("你好".to_string()),
            text_rgb: Some([245, 190, 72]),
            insertable_to_target: false,
        }
    );
    assert_eq!(
        desktop_text_lifecycle_view_model(DesktopTextLifecycleState::Corrected, "你好。"),
        DesktopTextLifecycleViewModel {
            text: Some("你好。".to_string()),
            text_rgb: Some([245, 247, 250]),
            insertable_to_target: true,
        }
    );
}

#[test]
fn mode_text_pane_layout_uses_single_pane_for_transcribe_and_document_only() {
    assert_eq!(
        desktop_mode_text_pane_layout(VoiceMode::Transcribe, None),
        DesktopModeTextPaneLayout::SingleProcessingText
    );
    assert_eq!(
        desktop_mode_text_pane_layout(VoiceMode::Document, None),
        DesktopModeTextPaneLayout::SingleProcessingText
    );
    assert_eq!(
        desktop_mode_text_pane_layout(VoiceMode::Generate, None),
        DesktopModeTextPaneLayout::DualTranscriptAndResult
    );
    assert_eq!(
        desktop_mode_text_pane_layout(VoiceMode::Command, None),
        DesktopModeTextPaneLayout::DualTranscriptAndResult
    );
    assert_eq!(
        desktop_mode_text_pane_layout(VoiceMode::Smart, Some(VoiceMode::Generate)),
        DesktopModeTextPaneLayout::DualTranscriptAndResult
    );
    assert_eq!(
        desktop_mode_text_pane_layout(VoiceMode::Smart, Some(VoiceMode::Document)),
        DesktopModeTextPaneLayout::SingleProcessingText
    );
}

#[test]
fn mode_output_policy_inserts_only_white_final_text_allowed_by_mode() {
    assert_eq!(
        desktop_mode_output_policy(VoiceMode::Transcribe, None),
        DesktopModeOutputPolicy {
            insert_corrected_segments: true,
            insert_generated_result: false,
            insert_command_result: false,
            show_command_result_in_gui: false,
        }
    );
    assert_eq!(
        desktop_mode_output_policy(VoiceMode::Document, None),
        DesktopModeOutputPolicy {
            insert_corrected_segments: true,
            insert_generated_result: false,
            insert_command_result: false,
            show_command_result_in_gui: false,
        }
    );
    assert_eq!(
        desktop_mode_output_policy(VoiceMode::Generate, None),
        DesktopModeOutputPolicy {
            insert_corrected_segments: false,
            insert_generated_result: true,
            insert_command_result: false,
            show_command_result_in_gui: false,
        }
    );
    assert_eq!(
        desktop_mode_output_policy(VoiceMode::Command, None),
        DesktopModeOutputPolicy {
            insert_corrected_segments: false,
            insert_generated_result: false,
            insert_command_result: false,
            show_command_result_in_gui: true,
        }
    );
    assert_eq!(
        desktop_mode_output_policy(VoiceMode::Smart, Some(VoiceMode::Generate)),
        desktop_mode_output_policy(VoiceMode::Generate, None)
    );
}

#[test]
fn runtime_insert_directive_enforces_mode_policy_and_text_lifecycle() {
    assert_eq!(
        desktop_runtime_insert_directive_for_mode(
            VoiceMode::Command,
            None,
            DesktopOutputStrategy::HonorConfiguredOutput,
            DesktopTextLifecycleState::Corrected,
        ),
        DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::DryRunOnly,
            show_result_in_gui: true,
        }
    );
    assert_eq!(
        desktop_runtime_insert_directive_for_mode(
            VoiceMode::Generate,
            None,
            DesktopOutputStrategy::HonorConfiguredOutput,
            DesktopTextLifecycleState::Corrected,
        ),
        DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::UseConfiguredOutput,
            show_result_in_gui: false,
        }
    );
    assert_eq!(
        desktop_runtime_insert_directive_for_mode(
            VoiceMode::Transcribe,
            None,
            DesktopOutputStrategy::HonorConfiguredOutput,
            DesktopTextLifecycleState::PreRecognized,
        ),
        DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::DryRunOnly,
            show_result_in_gui: false,
        }
    );
    assert_eq!(
        desktop_runtime_insert_directive_for_mode(
            VoiceMode::Document,
            None,
            DesktopOutputStrategy::ShowCopyPopupOnly,
            DesktopTextLifecycleState::Corrected,
        ),
        DesktopRuntimeInsertPlan {
            directive: DesktopRuntimeInsertDirective::DryRunOnly,
            show_result_in_gui: true,
        }
    );
}

#[test]
fn mode_text_result_model_uses_single_pane_for_transcribe_and_document() {
    assert_eq!(
        desktop_mode_text_result_model(
            VoiceMode::Transcribe,
            None,
            "你好",
            DesktopTextLifecycleState::PreRecognized,
            "你好。",
            DesktopTextLifecycleState::Corrected,
        ),
        DesktopModeTextResultModel {
            layout: DesktopModeTextPaneLayout::SingleProcessingText,
            panes: vec![DesktopModeTextPane {
                label: "文本".to_string(),
                text: "你好。".to_string(),
                lifecycle: DesktopTextLifecycleState::Corrected,
                text_rgb: [245, 247, 250],
                insertable_to_target: true,
            }],
        }
    );
    assert_eq!(
        desktop_mode_text_result_popup_text(&desktop_mode_text_result_model(
            VoiceMode::Document,
            None,
            "请写正式一点",
            DesktopTextLifecycleState::PreRecognized,
            "请以正式语气表述。",
            DesktopTextLifecycleState::Corrected,
        )),
        "请以正式语气表述。"
    );
}

#[test]
fn mode_text_result_model_uses_dual_panes_for_generate_and_command() {
    let model = desktop_mode_text_result_model(
        VoiceMode::Generate,
        None,
        "生成一段春天的散文",
        DesktopTextLifecycleState::Corrected,
        "春风拂过原野，万物在温柔的光里醒来。",
        DesktopTextLifecycleState::Corrected,
    );

    assert_eq!(
        model,
        DesktopModeTextResultModel {
            layout: DesktopModeTextPaneLayout::DualTranscriptAndResult,
            panes: vec![
                DesktopModeTextPane {
                    label: "转录".to_string(),
                    text: "生成一段春天的散文".to_string(),
                    lifecycle: DesktopTextLifecycleState::Corrected,
                    text_rgb: [245, 247, 250],
                    insertable_to_target: true,
                },
                DesktopModeTextPane {
                    label: "结果".to_string(),
                    text: "春风拂过原野，万物在温柔的光里醒来。".to_string(),
                    lifecycle: DesktopTextLifecycleState::Corrected,
                    text_rgb: [245, 247, 250],
                    insertable_to_target: true,
                },
            ],
        }
    );
    assert_eq!(
        desktop_mode_text_result_popup_text(&model),
        "转录\n生成一段春天的散文\n\n结果\n春风拂过原野，万物在温柔的光里醒来。"
    );
}

#[test]
fn mode_text_result_model_marks_pre_recognition_panes_yellow_and_not_insertable() {
    let model = desktop_mode_text_result_model(
        VoiceMode::Command,
        None,
        "打开记事本",
        DesktopTextLifecycleState::PreRecognized,
        "",
        DesktopTextLifecycleState::AudioWave,
    );

    assert_eq!(
        model.panes[0],
        DesktopModeTextPane {
            label: "转录".to_string(),
            text: "打开记事本".to_string(),
            lifecycle: DesktopTextLifecycleState::PreRecognized,
            text_rgb: [245, 190, 72],
            insertable_to_target: false,
        }
    );
}

#[test]
fn document_recorrection_auto_applies_only_when_user_has_not_edited_inserted_text() {
    assert_eq!(
        desktop_document_recorrection_decision(
            "今天我们讨论项目进度。",
            "今天我们讨论项目进度。",
            true
        ),
        DesktopDocumentRecorrectionDecision::AutoApplyToTarget
    );
    assert_eq!(
        desktop_document_recorrection_decision(
            "今天我们讨论项目进度。",
            "今天我们重点讨论项目进度。",
            true
        ),
        DesktopDocumentRecorrectionDecision::ShowInTalkGuiOnly
    );
    assert_eq!(
        desktop_document_recorrection_decision(
            "今天我们讨论项目进度。",
            "今天我们讨论项目进度。",
            false
        ),
        DesktopDocumentRecorrectionDecision::ShowInTalkGuiOnly
    );
}

#[test]
fn document_recorrection_session_decision_uses_all_inserted_stable_segments() {
    let inserted_segments = vec![
        "第一段已经插入。".to_string(),
        "第二段也已经插入。".to_string(),
    ];

    assert_eq!(
        desktop_document_recorrection_session_decision(
            &inserted_segments,
            "第一段已经插入。第二段也已经插入。",
            true,
        ),
        DesktopDocumentRecorrectionDecision::AutoApplyToTarget
    );
    assert_eq!(
        desktop_document_recorrection_session_decision(
            &inserted_segments,
            "第一段用户改过。第二段也已经插入。",
            true,
        ),
        DesktopDocumentRecorrectionDecision::ShowInTalkGuiOnly
    );
    assert_eq!(
        desktop_document_recorrection_session_decision(
            &inserted_segments,
            "第一段已经插入。第二段也已经插入。",
            false,
        ),
        DesktopDocumentRecorrectionDecision::ShowInTalkGuiOnly
    );
}

#[test]
fn mode_dropdown_model_lists_five_modes_and_marks_current_selection() {
    assert_eq!(
        desktop_mode_dropdown_model(VoiceMode::Generate),
        DesktopModeDropdownModel {
            title: "模式".to_string(),
            current_label: "生成".to_string(),
            entries: vec![
                DesktopModeDropdownEntry {
                    mode: VoiceMode::Smart,
                    label: "智能".to_string(),
                    shortcut_hint: Some("RightCtrl+5".to_string()),
                    selected: false,
                },
                DesktopModeDropdownEntry {
                    mode: VoiceMode::Transcribe,
                    label: "转录".to_string(),
                    shortcut_hint: Some("RightCtrl+1".to_string()),
                    selected: false,
                },
                DesktopModeDropdownEntry {
                    mode: VoiceMode::Document,
                    label: "公文".to_string(),
                    shortcut_hint: Some("RightCtrl+2".to_string()),
                    selected: false,
                },
                DesktopModeDropdownEntry {
                    mode: VoiceMode::Command,
                    label: "命令".to_string(),
                    shortcut_hint: Some("RightCtrl+3".to_string()),
                    selected: false,
                },
                DesktopModeDropdownEntry {
                    mode: VoiceMode::Generate,
                    label: "生成".to_string(),
                    shortcut_hint: Some("RightCtrl+4".to_string()),
                    selected: true,
                },
            ],
        }
    );
}

#[test]
fn desktop_correction_job_model_maps_ready_segment_to_patchable_insert_anchor() {
    let pipeline = DesktopSpeculativePipelineConfig {
        enabled: true,
        local_asr: "streaming_service".to_string(),
        cloud_correction: "provider_text_processor".to_string(),
    };
    let insert_target = ForegroundInsertTarget {
        window_handle: 0x707,
        focus_handle: Some(0x808),
        primary_focus_handle: None,
        fallback_focus_handle: None,
        focus_capture_source: None,
    };
    let event = SpeculativeRuntimeEvent::CorrectionRequested {
        segment_id: "seg-1".to_string(),
        local_text: "我下午三点有空。".to_string(),
        context_before: "前一句。".to_string(),
    };

    let model =
        desktop_speculative_correction_job_model(&pipeline, &event, Some(insert_target), 1_200)
            .expect("correction request should create desktop job model");

    assert_eq!(
        model,
        DesktopSpeculativeCorrectionJobModel {
            segment_id: "seg-1".to_string(),
            local_text: "我下午三点有空。".to_string(),
            context_before: "前一句。".to_string(),
            output_target: DesktopSpeculativeCorrectionOutputTarget::PatchInsertedText(
                SpeculativeInsertAnchor::new(
                    0x707,
                    Some(0x808),
                    "seg-1",
                    "我下午三点有空。",
                    1_200
                )
                .unwrap()
            ),
        }
    );
}

#[test]
fn desktop_correction_job_model_can_route_ready_segment_to_popup_only_without_insert_target() {
    let pipeline = DesktopSpeculativePipelineConfig {
        enabled: true,
        local_asr: "streaming_service".to_string(),
        cloud_correction: "provider_text_processor".to_string(),
    };
    let event = SpeculativeRuntimeEvent::CorrectionRequested {
        segment_id: "seg-1".to_string(),
        local_text: "没有激活输入框。".to_string(),
        context_before: String::new(),
    };

    let model = desktop_speculative_correction_job_model(&pipeline, &event, None, 1_200)
        .expect("correction request should still be usable for popup fallback");

    assert_eq!(
        model.output_target,
        DesktopSpeculativeCorrectionOutputTarget::CopyPopupOnly
    );
    assert_eq!(model.local_text, "没有激活输入框。");
}

#[test]
fn desktop_correction_job_model_ignores_non_correction_events_and_disabled_cloud_mode() {
    let disabled_pipeline = DesktopSpeculativePipelineConfig {
        enabled: true,
        local_asr: "streaming_service".to_string(),
        cloud_correction: "disabled".to_string(),
    };
    let enabled_pipeline = DesktopSpeculativePipelineConfig {
        cloud_correction: "provider_text_processor".to_string(),
        ..disabled_pipeline.clone()
    };
    let correction = SpeculativeRuntimeEvent::CorrectionRequested {
        segment_id: "seg-1".to_string(),
        local_text: "本地文本。".to_string(),
        context_before: String::new(),
    };
    let draft = SpeculativeRuntimeEvent::DraftUpdated {
        segment_id: "seg-1".to_string(),
        text: "本地".to_string(),
    };

    assert_eq!(
        desktop_speculative_correction_job_model(&disabled_pipeline, &correction, None, 0),
        None
    );
    assert_eq!(
        desktop_speculative_correction_job_model(&enabled_pipeline, &draft, None, 0),
        None
    );
}

fn editable_insert_target(window_handle: isize, focus_handle: isize) -> DesktopInsertTargetContext {
    DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle,
            focus_handle: Some(focus_handle),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(focus_handle),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    }
}

#[test]
fn live_streaming_local_segment_plan_inserts_only_when_original_editable_target_is_still_active() {
    let origin_target = editable_insert_target(0x707, 0x808);
    let current_target = editable_insert_target(0x707, 0x808);
    let event = SpeculativeRuntimeEvent::LocalSegmentCommitted {
        segment_id: "seg-1".to_string(),
        text: "你好呀。".to_string(),
    };

    assert_eq!(
        live_streaming_local_segment_plan(
            OutputMode::ClipboardPaste,
            &event,
            Some(&origin_target),
            Some(&current_target),
        ),
        DesktopLiveStreamingLocalSegmentPlan::DeferToStop {
            segment_id: "seg-1".to_string(),
            text: "你好呀。".to_string(),
        }
    );
}

#[test]
fn live_streaming_corrected_segment_plan_can_insert_when_original_target_is_still_active() {
    let origin_target = editable_insert_target(0x707, 0x808);
    let current_target = editable_insert_target(0x707, 0x808);
    let event = SpeculativeRuntimeEvent::LocalSegmentCommitted {
        segment_id: "seg-1".to_string(),
        text: "你好呀。".to_string(),
    };

    assert_eq!(
        live_streaming_segment_plan_for_lifecycle(
            OutputMode::ClipboardPaste,
            &event,
            Some(&origin_target),
            Some(&current_target),
            DesktopTextLifecycleState::Corrected,
        ),
        DesktopLiveStreamingLocalSegmentPlan::Insert {
            segment_id: "seg-1".to_string(),
            text: "你好呀。".to_string(),
            insert_target: ForegroundInsertTarget {
                window_handle: 0x707,
                focus_handle: Some(0x808),
                primary_focus_handle: None,
                fallback_focus_handle: None,
                focus_capture_source: None,
            },
        }
    );
}

#[test]
fn live_streaming_local_segment_plan_defers_when_focus_moved_to_another_control() {
    let origin_target = editable_insert_target(0x707, 0x808);
    let current_target = editable_insert_target(0x707, 0x909);
    let event = SpeculativeRuntimeEvent::LocalSegmentCommitted {
        segment_id: "seg-1".to_string(),
        text: "不要抢焦点。".to_string(),
    };

    assert_eq!(
        live_streaming_local_segment_plan(
            OutputMode::ClipboardPaste,
            &event,
            Some(&origin_target),
            Some(&current_target),
        ),
        DesktopLiveStreamingLocalSegmentPlan::DeferToStop {
            segment_id: "seg-1".to_string(),
            text: "不要抢焦点。".to_string(),
        }
    );
}

#[test]
fn streaming_stop_policy_skips_final_insert_but_keeps_final_correction_after_live_insertions() {
    assert_eq!(
        desktop_streaming_stop_policy(0),
        DesktopStreamingStopPolicy {
            insert_final_transcript: true,
            allow_final_correction_job: true,
        }
    );
    assert_eq!(
        desktop_streaming_stop_policy(2),
        DesktopStreamingStopPolicy {
            insert_final_transcript: false,
            allow_final_correction_job: true,
        }
    );
}

#[test]
fn desktop_streaming_stop_tail_text_inserts_only_uncommitted_remainder() {
    let inserted =
        vec![SpeculativeInsertAnchor::new(0x707, Some(0x808), "seg-1", "你好。", 0).unwrap()];

    assert_eq!(
        desktop_streaming_stop_tail_text("seg-1", "你好。", &inserted),
        None
    );
    assert_eq!(
        desktop_streaming_stop_tail_text("seg-1", "你好。今天继续。", &inserted),
        Some("今天继续。".to_string())
    );
    assert_eq!(
        desktop_streaming_stop_tail_text("seg-2", "这是尾句。", &inserted),
        Some("这是尾句。".to_string())
    );
    assert_eq!(
        desktop_streaming_stop_tail_text("seg-1", "完整句。", &[]),
        Some("完整句。".to_string())
    );
}

#[test]
fn desktop_streaming_latest_segment_allows_patch_only_for_current_tail_segment() {
    let inserted_segment_ids = vec!["seg-1".to_string(), "seg-2".to_string()];

    assert!(desktop_streaming_latest_segment_allows_auto_patch(
        &inserted_segment_ids,
        "seg-2"
    ));
    assert!(!desktop_streaming_latest_segment_allows_auto_patch(
        &inserted_segment_ids,
        "seg-1"
    ));
    assert!(!desktop_streaming_latest_segment_allows_auto_patch(
        &[],
        "seg-1"
    ));
}

#[test]
fn desktop_speculative_local_asr_route_recognizes_streaming_service_as_supported_runtime() {
    let config = DesktopSpeculativePipelineConfig {
        enabled: true,
        local_asr: "streaming_service".to_string(),
        cloud_correction: "provider_text_processor".to_string(),
    };

    assert_eq!(
        desktop_speculative_local_asr_route(&config),
        DesktopSpeculativeLocalAsrRoute::StreamingService
    );
}

#[test]
fn replacement_selection_count_counts_unicode_scalars() {
    assert_eq!(
        desktop_speculative_replacement_selection_count("你好呀。"),
        4
    );
}

#[test]
fn replacement_selection_count_ignores_empty_text() {
    assert_eq!(desktop_speculative_replacement_selection_count(""), 0);
}

#[test]
fn typeless_hud_view_model_maps_recording_to_listening_and_processing_to_thinking() {
    let listening = desktop_hud_view_model_for_phase(RuntimePhase::Recording);
    assert_eq!(listening.visual_state, DesktopHudVisualState::Listening);
    assert_eq!(listening.title, "Listening");
    assert_eq!(listening.detail.as_deref(), Some("..."));
    assert_eq!(
        listening
            .meter
            .as_ref()
            .expect("listening meter")
            .bar_heights,
        [4, 4, 4, 4, 4, 4, 4, 4, 4]
    );

    let thinking = desktop_hud_view_model_for_phase(RuntimePhase::Processing);
    assert_eq!(thinking.visual_state, DesktopHudVisualState::Thinking);
    assert_eq!(thinking.title, "Thinking");
    assert_eq!(thinking.detail, None);
    assert_eq!(thinking.meter, None);
    assert_eq!(thinking.progress_percent, Some(62));
    assert_eq!(
        desktop_hud_metrics_for_view_model(&thinking),
        DesktopHudMetrics {
            width: 188,
            height: 40,
            bottom_margin: 132,
            corner_radius: 0,
        }
    );
}

#[test]
fn thinking_progress_estimates_advance_with_runtime_phase() {
    assert_eq!(
        desktop_hud_view_model_for_phase(RuntimePhase::Transcribing).progress_percent,
        Some(28)
    );
    assert_eq!(
        desktop_hud_view_model_for_phase(RuntimePhase::Processing).progress_percent,
        Some(62)
    );
    assert_eq!(
        desktop_hud_view_model_for_phase(RuntimePhase::Inserting).progress_percent,
        Some(88)
    );
}

#[test]
fn thinking_progress_model_uses_a_single_soft_eta_pass_with_vertical_wave_only() {
    let start = desktop_hud_thinking_progress_model(Some(62), 0);
    let immediate = desktop_hud_thinking_progress_model(Some(62), 1);
    let halfway = desktop_hud_thinking_progress_model(Some(62), 22);
    let at_eta = desktop_hud_thinking_progress_model(Some(62), 42);
    let beyond_eta = desktop_hud_thinking_progress_model(Some(62), 60);

    assert_eq!(start.fill_percent, 0);
    assert!(immediate.fill_percent > start.fill_percent);
    assert!(immediate.fill_percent <= 8);
    assert!(halfway.fill_percent > start.fill_percent);
    assert!(at_eta.fill_percent >= halfway.fill_percent);
    assert!(beyond_eta.fill_percent >= at_eta.fill_percent);
    assert_eq!(start.text_wave_offset_px, 0);
    assert_eq!(start.text_wave_offset_px, immediate.text_wave_offset_px);
    assert!(halfway.text_wave_offset_px.abs() >= 1);
    assert_eq!(start.text_wave_offset_px, at_eta.text_wave_offset_px);
}

#[test]
fn thinking_progress_no_longer_wraps_back_to_the_left_after_the_soft_eta() {
    let before_eta = desktop_hud_thinking_progress_model(Some(62), 41);
    let at_eta = desktop_hud_thinking_progress_model(Some(62), 42);
    let after_eta = desktop_hud_thinking_progress_model(Some(62), 60);

    assert!(at_eta.fill_percent >= before_eta.fill_percent);
    assert!(after_eta.fill_percent >= at_eta.fill_percent);
}

#[test]
fn thinking_progress_uses_later_runtime_phases_to_push_the_fake_eta_forward_faster() {
    let transcribing = desktop_hud_thinking_progress_model(Some(28), 8);
    let processing = desktop_hud_thinking_progress_model(Some(62), 8);
    let inserting = desktop_hud_thinking_progress_model(Some(88), 8);

    assert!(processing.fill_percent > transcribing.fill_percent);
    assert!(inserting.fill_percent > processing.fill_percent);
}

#[test]
fn thinking_text_wave_offsets_are_per_character_and_advance_over_time() {
    let start = desktop_hud_thinking_text_wave_offsets("Thinking".chars().count(), 0);
    let next = desktop_hud_thinking_text_wave_offsets("Thinking".chars().count(), 1);
    let later = desktop_hud_thinking_text_wave_offsets("Thinking".chars().count(), 2);

    assert_eq!(start.len(), "Thinking".chars().count());
    assert!(start.windows(2).any(|pair| pair[0] != pair[1]));
    assert!(start.iter().any(|offset| offset.abs() >= 2));
    assert!(start.iter().all(|offset| offset.abs() <= 3));
    assert_eq!(start, next);
    assert_ne!(start, later);
}

#[test]
fn thinking_palette_uses_hook_theme_signal_over_a_dark_terminal_panel() {
    let palette = desktop_hud_thinking_palette();

    assert_eq!(palette.track_start_rgb, [11, 14, 18]);
    assert_eq!(palette.track_end_rgb, [20, 24, 30]);
    assert_eq!(palette.fill_start_rgb, [163, 204, 0]);
    assert_eq!(palette.fill_end_rgb, [217, 255, 56]);
    assert_eq!(palette.fill_head_rgb, [239, 255, 146]);
    assert_eq!(palette.text_rgb, [247, 252, 230]);
}

#[test]
fn thinking_palette_keeps_fill_and_track_visually_distinct_for_the_fake_progress_bar() {
    let palette = desktop_hud_thinking_palette();
    let track_sum = palette
        .track_end_rgb
        .iter()
        .map(|channel| u16::from(*channel))
        .sum::<u16>();
    let fill_sum = palette
        .fill_end_rgb
        .iter()
        .map(|channel| u16::from(*channel))
        .sum::<u16>();
    let head_sum = palette
        .fill_head_rgb
        .iter()
        .map(|channel| u16::from(*channel))
        .sum::<u16>();

    assert!(fill_sum > track_sum + 300);
    assert!(head_sum > fill_sum);
}

#[test]
fn listening_hud_compacts_into_typeless_style_capsule_with_audio_meter() {
    let listening = desktop_hud_view_model_for_listening_level(0.75);

    assert_eq!(listening.visual_state, DesktopHudVisualState::Listening);
    assert_eq!(
        listening
            .meter
            .as_ref()
            .expect("listening meter")
            .bar_heights,
        [6, 8, 11, 13, 14, 13, 11, 8, 6]
    );
    assert_eq!(
        desktop_hud_metrics_for_view_model(&listening),
        DesktopHudMetrics {
            width: 188,
            height: 52,
            bottom_margin: 130,
            corner_radius: 0,
        }
    );
}

#[test]
fn listening_hud_shows_initial_local_detection_placeholder_before_first_partial() {
    let model = desktop_hud_view_model_for_phase(RuntimePhase::Recording);

    assert_eq!(model.visual_state, DesktopHudVisualState::Listening);
    assert_eq!(model.detail.as_deref(), Some("..."));
    let layout = desktop_listening_hud_partial_text_layout(188, 52, 96, model.detail.as_deref())
        .expect("initial local detection text layout");
    assert_eq!(layout.line_count, 1);
    assert_eq!(layout.waveform_rect.top, 23);
}

#[test]
fn listening_hud_click_targets_map_to_cancel_and_complete_buttons() {
    let metrics = DesktopHudMetrics {
        width: 188,
        height: 52,
        bottom_margin: 130,
        corner_radius: 0,
    };
    let cancel = desktop_listening_hud_cancel_button_rect(metrics.width, metrics.height, 96);
    let complete = desktop_listening_hud_complete_button_rect(metrics.width, metrics.height, 96);

    assert_eq!(
        desktop_listening_hud_action_for_point(
            metrics.width,
            metrics.height,
            96,
            cancel.left + 2,
            cancel.top + 2
        ),
        DesktopListeningHudAction::Cancel
    );
    assert_eq!(
        desktop_listening_hud_action_for_point(
            metrics.width,
            metrics.height,
            96,
            complete.right - 2,
            complete.bottom - 2
        ),
        DesktopListeningHudAction::Complete
    );
    assert_eq!(
        desktop_listening_hud_action_for_point(metrics.width, metrics.height, 96, 94, 26),
        DesktopListeningHudAction::Ignore
    );
}

#[test]
fn desktop_overlay_scaling_tracks_window_dpi_instead_of_system_bitmap_stretching() {
    assert!((desktop_overlay_scale_factor_for_dpi(96) - 1.0).abs() < f32::EPSILON);
    assert!((desktop_overlay_scale_factor_for_dpi(144) - 1.5).abs() < f32::EPSILON);
    assert_eq!(scale_desktop_overlay_length(72, 96), 72);
    assert_eq!(scale_desktop_overlay_length(72, 144), 108);
}

#[test]
fn listening_hud_meter_model_clamps_levels_into_stable_bar_heights() {
    assert_eq!(
        desktop_hud_audio_meter_model(-0.5).bar_heights,
        [4, 4, 4, 4, 4, 4, 4, 4, 4]
    );
    assert_eq!(
        desktop_hud_audio_meter_model(1.0).bar_heights,
        [7, 10, 13, 16, 18, 16, 13, 10, 7]
    );
}

#[test]
fn listening_hud_waveform_model_uses_real_bin_shape_instead_of_fake_symmetry() {
    let meter = desktop_hud_audio_meter_model_for_waveform([
        0.0, 0.1, 0.85, 0.25, 0.65, 0.15, 0.4, 0.95, 0.05,
    ]);

    assert_eq!(meter.bar_heights, [4, 5, 16, 8, 13, 6, 10, 17, 4]);
}

#[test]
fn listening_hud_can_show_latest_streaming_partial_without_leaving_listening_state() {
    let model = desktop_hud_view_model_for_listening_waveform_with_partial(
        [0.0, 0.1, 0.85, 0.25, 0.65, 0.15, 0.4, 0.95, 0.05],
        Some("  你好呀  "),
    );

    assert_eq!(model.visual_state, DesktopHudVisualState::Listening);
    assert_eq!(model.title, "Listening");
    assert_eq!(model.detail.as_deref(), Some("你好呀"));
    assert_eq!(model.progress_percent, None);
    assert_eq!(
        model.meter.as_ref().expect("listening meter").bar_heights,
        [4, 5, 16, 8, 13, 6, 10, 17, 4]
    );
    assert_eq!(
        desktop_hud_metrics_for_view_model(&model),
        DesktopHudMetrics {
            width: 188,
            height: 52,
            bottom_margin: 130,
            corner_radius: 0,
        }
    );
}

#[test]
fn listening_hud_expands_vertically_for_long_streaming_partial_text() {
    let long_partial = "这是一个比较长的实时转录内容，用来模拟用户在一次按住右Alt录音过程中连续说出一整句话，而不是只说几个字。";
    let model = desktop_hud_view_model_for_listening_waveform_with_partial(
        [0.0, 0.1, 0.85, 0.25, 0.65, 0.15, 0.4, 0.95, 0.05],
        Some(long_partial),
    );

    let metrics = desktop_hud_metrics_for_view_model(&model);
    assert!(metrics.width > 188);
    assert!(metrics.width <= 340);
    assert!(metrics.height > 52);
    assert!(metrics.height >= 88);
    assert!(metrics.height <= 178);

    let layout = desktop_listening_hud_partial_text_layout(
        metrics.width,
        metrics.height,
        96,
        model.detail.as_deref(),
    )
    .expect("long partial text layout");

    assert!(layout.wraps_text);
    assert!(layout.line_count >= 2);
    assert!(!layout.scrolls_text);
    assert!(layout.text_rect.bottom - layout.text_rect.top >= 34);
    assert!(layout.waveform_rect.top > layout.text_rect.bottom);
    assert!(layout.waveform_rect.bottom <= metrics.height - 8);
}

#[test]
fn listening_hud_scrolls_tail_for_very_long_streaming_partial_text() {
    let very_long_partial = format!(
        "{}{}{}",
        "这是一个特别长的实时转录内容，用户可能还在持续说话，所以界面不应该横向撑满屏幕，而应该在保持合理宽度的前提下纵向扩展，",
        "当用户继续描述更多上下文、更多细节、更多需要被输入的内容时，界面应该继续保持录音状态，并把实时识别的最新内容放在可见区域，",
        "超过最大高度以后显示滚动条，并且优先展示最新的尾部转录内容，让用户知道识别仍然在继续同步更新。"
    );
    let model = desktop_hud_view_model_for_listening_waveform_with_partial(
        [0.0, 0.1, 0.85, 0.25, 0.65, 0.15, 0.4, 0.95, 0.05],
        Some(&very_long_partial),
    );
    let metrics = desktop_hud_metrics_for_view_model(&model);
    let layout = desktop_listening_hud_partial_text_layout(
        metrics.width,
        metrics.height,
        96,
        model.detail.as_deref(),
    )
    .expect("very long partial text layout");

    assert!(metrics.width <= 340);
    assert_eq!(metrics.height, 178);
    assert!(layout.scrolls_text);
    assert!(layout.scrollbar_rect.is_some());
    let visible_text = desktop_listening_hud_visible_partial_text(&very_long_partial, &layout);
    assert!(visible_text.len() < very_long_partial.len());
    assert!(visible_text.ends_with("识别仍然在继续同步更新。"));
}

#[test]
fn toggle_recording_uses_manual_stop_instead_of_short_timeout_watcher() {
    assert_eq!(
        recording_stop_watcher_policy(TriggerMode::Toggle, 15),
        DesktopRecordingStopWatcherPolicy::ManualOnly
    );
    assert_eq!(
        recording_stop_watcher_policy(TriggerMode::PushToTalk, 15),
        DesktopRecordingStopWatcherPolicy::TimeoutAfterSeconds(15)
    );
}

#[test]
fn copy_popup_model_uses_compact_editable_copy_strings_without_title_or_quotes() {
    let model = desktop_copy_popup_model("你好呀");
    assert_eq!(
        model,
        DesktopCopyPopupModel {
            title: String::new(),
            editable_text: "你好呀".to_string(),
            copy_label: "复制".to_string(),
            panes: vec![DesktopCopyPopupPaneModel {
                label: String::new(),
                text: "你好呀".to_string(),
                editable: true,
                copy_default: true,
            }],
        }
    );
}

#[test]
fn copy_popup_panes_preserve_single_and_dual_mode_identity() {
    let single_result = desktop_mode_text_result_model(
        VoiceMode::Transcribe,
        None,
        "你好呀",
        DesktopTextLifecycleState::Corrected,
        "",
        DesktopTextLifecycleState::PreRecognized,
    );
    let single_popup = desktop_copy_popup_model_for_mode_text_result(&single_result);
    assert_eq!(single_popup.editable_text, "你好呀");
    assert_eq!(
        single_popup.panes,
        vec![DesktopCopyPopupPaneModel {
            label: "文本".to_string(),
            text: "你好呀".to_string(),
            editable: true,
            copy_default: true,
        }]
    );

    let dual_result = desktop_mode_text_result_model(
        VoiceMode::Generate,
        None,
        "生成一篇春天散文",
        DesktopTextLifecycleState::Corrected,
        "春天来了。",
        DesktopTextLifecycleState::Corrected,
    );
    let dual_popup = desktop_copy_popup_model_for_mode_text_result(&dual_result);
    assert_eq!(dual_popup.editable_text, "春天来了。");
    assert_eq!(
        dual_popup.panes,
        vec![
            DesktopCopyPopupPaneModel {
                label: "转录".to_string(),
                text: "生成一篇春天散文".to_string(),
                editable: false,
                copy_default: false,
            },
            DesktopCopyPopupPaneModel {
                label: "结果".to_string(),
                text: "春天来了。".to_string(),
                editable: true,
                copy_default: true,
            },
        ]
    );
}

#[test]
fn copy_popup_model_keeps_full_transcript_text_available_for_manual_edits() {
    let model = desktop_copy_popup_model(
        "123456789012345678901234567890123456789012345678901234567890-long-tail",
    );
    assert_eq!(
        model.editable_text,
        "123456789012345678901234567890123456789012345678901234567890-long-tail".to_string()
    );
}

#[test]
fn copy_popup_virtual_keys_map_enter_to_copy_and_escape_to_close() {
    assert_eq!(
        desktop_copy_popup_action_for_virtual_key(0x0D),
        DesktopCopyPopupAction::CopyToClipboard
    );
    assert_eq!(
        desktop_copy_popup_action_for_virtual_key(0x1B),
        DesktopCopyPopupAction::Close
    );
}

#[test]
fn copy_popup_virtual_keys_ignore_unbound_keys() {
    assert_eq!(
        desktop_copy_popup_action_for_virtual_key(0x41),
        DesktopCopyPopupAction::Ignore
    );
    assert_eq!(
        desktop_copy_popup_action_for_virtual_key(0x20),
        DesktopCopyPopupAction::Ignore
    );
}

#[test]
fn hud_stays_non_activating_while_copy_popup_can_activate_when_user_wants_to_edit() {
    assert_eq!(
        desktop_hud_activation_policy(),
        DesktopOverlayActivationPolicy::NoActivate
    );
    assert_eq!(
        desktop_copy_popup_activation_policy(),
        DesktopOverlayActivationPolicy::ActivateOnInteract
    );
}

#[test]
fn terminal_success_and_cancelled_phases_hide_hud_instead_of_showing_extra_banner() {
    assert_eq!(
        desktop_hud_presentation_for_phase(RuntimePhase::Completed),
        DesktopHudPresentation::Hidden
    );
    assert_eq!(
        desktop_hud_presentation_for_phase(RuntimePhase::Cancelled),
        DesktopHudPresentation::Hidden
    );
    assert_eq!(
        desktop_hud_presentation_for_phase(RuntimePhase::Failed),
        DesktopHudPresentation::Visible {
            auto_hide_ms: Some(1500)
        }
    );
}

#[test]
fn copy_popup_is_positioned_like_a_bottom_toast_instead_of_center_modal() {
    assert_eq!(
        desktop_copy_popup_position(1920, 1080, 388, 156, 88),
        DesktopOverlayPosition { x: 766, y: 836 }
    );
}

#[test]
fn copy_popup_copy_does_not_spawn_follow_up_hud_feedback() {
    assert!(!desktop_copy_popup_copy_shows_follow_up_hud());
}

#[test]
fn copy_popup_metrics_use_compact_bottom_toast_dimensions() {
    assert_eq!(
        desktop_copy_popup_metrics(),
        DesktopCopyPopupMetrics {
            width: 388,
            height: 156,
            bottom_margin: 88,
        }
    );
}

#[test]
fn copy_popup_layout_keeps_the_close_button_clear_of_the_editor_frame() {
    let metrics = desktop_copy_popup_metrics();
    let close = desktop_copy_popup_close_button_rect(metrics.width, metrics.height, 96);
    let editor = desktop_copy_popup_editor_frame_rect(metrics.width, metrics.height, 96);

    assert!(editor.top >= close.bottom);
}

#[test]
fn copy_popup_layout_uses_a_large_centered_copy_button_below_the_editor_frame() {
    let metrics = desktop_copy_popup_metrics();
    let copy = desktop_copy_popup_copy_button_rect(metrics.width, metrics.height, 96);
    let editor = desktop_copy_popup_editor_frame_rect(metrics.width, metrics.height, 96);

    assert!(copy.top >= editor.bottom);
    assert_eq!(copy.left + copy.right, metrics.width);
}

#[test]
fn copy_popup_layout_tracks_bottom_controls_when_height_expands_for_dual_panes() {
    assert_eq!(
        desktop_copy_popup_copy_button_rect(388, 244, 96),
        DesktopOverlayRect {
            left: 150,
            top: 202,
            right: 238,
            bottom: 232,
        }
    );
    assert_eq!(
        desktop_copy_popup_editor_frame_rect(388, 244, 96),
        DesktopOverlayRect {
            left: 20,
            top: 46,
            right: 368,
            bottom: 196,
        }
    );
}

#[test]
fn copy_popup_editor_content_rect_wraps_inside_the_frame_and_centers_short_text() {
    let metrics = desktop_copy_popup_metrics();
    let frame = desktop_copy_popup_editor_frame_rect(metrics.width, metrics.height, 96);
    let content = desktop_copy_popup_editor_content_rect(metrics.width, metrics.height, 96, 20);

    assert!(content.left > frame.left);
    assert!(content.right < frame.right);
    assert!(content.top >= frame.top);
    assert!(content.bottom <= frame.bottom);

    let frame_center = frame.top + ((frame.bottom - frame.top) / 2);
    let content_center = content.top + ((content.bottom - content.top) / 2);
    assert!((frame_center - content_center).abs() <= 1);
}

#[test]
fn copy_popup_pane_layouts_stack_dual_panes_inside_the_expanded_editor_frame() {
    let frame = desktop_copy_popup_editor_frame_rect(388, 244, 96);
    let layouts = desktop_copy_popup_pane_layouts(388, 244, 96, &[40, 40]);

    assert_eq!(layouts.len(), 2);
    assert!(layouts[0].label_rect.top >= frame.top);
    assert!(layouts[0].label_rect.bottom <= layouts[0].editor_rect.top);
    assert!(layouts[0].editor_rect.bottom < layouts[1].label_rect.top);
    assert!(layouts[1].label_rect.bottom <= layouts[1].editor_rect.top);
    assert!(layouts[1].editor_rect.bottom <= frame.bottom);

    for layout in &layouts {
        assert!(layout.label_rect.left > frame.left);
        assert!(layout.editor_rect.left > frame.left);
        assert_eq!(layout.label_rect.left, layout.editor_rect.left);
        assert!(layout.label_rect.right < frame.right);
        assert!(layout.editor_rect.right < frame.right);
        assert_eq!(layout.label_rect.right, layout.editor_rect.right);
        assert!(layout.editor_rect.bottom > layout.editor_rect.top);
    }
}

#[test]
fn copy_popup_pane_layouts_keep_single_pane_centered_like_the_existing_editor() {
    let metrics = desktop_copy_popup_metrics();
    assert_eq!(
        desktop_copy_popup_pane_layouts(metrics.width, metrics.height, 96, &[20])
            .into_iter()
            .map(|layout| layout.editor_rect)
            .collect::<Vec<_>>(),
        vec![desktop_copy_popup_editor_content_rect(
            metrics.width,
            metrics.height,
            96,
            20
        )]
    );
}

#[test]
fn desktop_output_strategy_prefers_copy_popup_when_no_editable_target_is_available() {
    assert_eq!(
        decide_desktop_output_strategy(OutputMode::ClipboardPaste, None),
        DesktopOutputStrategy::ShowCopyPopupOnly
    );

    let edit_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x303,
            focus_handle: Some(0x404),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Button".to_string()),
        caret_window_handle: None,
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    assert_eq!(
        decide_desktop_output_strategy(OutputMode::ClipboardPaste, Some(&edit_target)),
        DesktopOutputStrategy::ShowCopyPopupOnly
    );
}

#[test]
fn desktop_output_strategy_keeps_direct_insert_when_focus_looks_editable() {
    let edit_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x303,
            focus_handle: Some(0x404),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: None,
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    assert_eq!(
        decide_desktop_output_strategy(OutputMode::ClipboardPaste, Some(&edit_target)),
        DesktopOutputStrategy::HonorConfiguredOutput
    );

    let browser_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x303,
            focus_handle: Some(0x404),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Chrome_RenderWidgetHostHWND".to_string()),
        caret_window_handle: Some(0x505),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    assert_eq!(
        decide_desktop_output_strategy(OutputMode::ClipboardPaste, Some(&browser_target)),
        DesktopOutputStrategy::HonorConfiguredOutput
    );
}

#[test]
fn desktop_output_plan_suppresses_insert_when_same_window_focus_moved_to_another_editable_control()
{
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x808),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x808),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x909),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x909),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::ShowCopyPopupOnly,
            insert_target: None,
        }
    );
}

#[test]
fn desktop_output_plan_suppresses_insert_when_current_same_window_focus_is_explicitly_noneditable()
{
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x808),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x808),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x909),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Button".to_string()),
        caret_window_handle: None,
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::ShowCopyPopupOnly,
            insert_target: None,
        }
    );
}

#[test]
fn desktop_output_plan_suppresses_insert_when_current_same_window_focus_signal_is_ambiguous() {
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x808),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("pane".to_string()),
        automation_framework_id: Some("Win32".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x909),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("pane".to_string()),
        automation_framework_id: Some("Win32".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::ShowCopyPopupOnly,
            insert_target: None,
        }
    );
}

#[test]
fn desktop_output_plan_keeps_insert_when_same_window_focus_handle_is_unchanged() {
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x808),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x808),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let current_target = origin_target.clone();

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::HonorConfiguredOutput,
            insert_target: current_target.target,
        }
    );
}

#[test]
fn desktop_output_plan_suppresses_insert_when_same_window_focus_handles_are_missing() {
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("pane".to_string()),
        automation_framework_id: Some("Win32".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("pane".to_string()),
        automation_framework_id: Some("Win32".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::ShowCopyPopupOnly,
            insert_target: None,
        }
    );
}

#[test]
fn desktop_output_plan_keeps_insert_when_browser_same_control_is_confirmed_via_matching_runtime_id()
{
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 159]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };
    let current_target = origin_target.clone();

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::HonorConfiguredOutput,
            insert_target: current_target.target,
        }
    );
}

#[test]
fn desktop_output_plan_suppresses_insert_when_browser_same_window_runtime_id_changed() {
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 159]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 160]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::ShowCopyPopupOnly,
            insert_target: None,
        }
    );
}

#[test]
fn desktop_output_plan_keeps_insert_when_same_control_is_confirmed_via_origin_focus_and_current_caret(
) {
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x808),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x808),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x808),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::HonorConfiguredOutput,
            insert_target: current_target.target,
        }
    );
}

#[test]
fn desktop_insert_target_restore_is_not_requested_when_current_focus_already_matches_target() {
    let target = ForegroundInsertTarget {
        window_handle: 0x707,
        focus_handle: Some(0x808),
        primary_focus_handle: None,
        fallback_focus_handle: None,
        focus_capture_source: None,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(target),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x808),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert!(!desktop_insert_target_restore_requested(
        target,
        Some(&current_target)
    ));
}

#[test]
fn desktop_insert_target_restore_is_requested_when_current_focus_moved_to_another_control() {
    let target = ForegroundInsertTarget {
        window_handle: 0x707,
        focus_handle: Some(0x808),
        primary_focus_handle: None,
        fallback_focus_handle: None,
        focus_capture_source: None,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x909),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x909),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert!(desktop_insert_target_restore_requested(
        target,
        Some(&current_target)
    ));
}

#[test]
fn desktop_insert_target_restore_is_not_requested_when_selected_target_already_matches_the_current_active_control(
) {
    let target = ForegroundInsertTarget {
        window_handle: 0x707,
        focus_handle: Some(0x909),
        primary_focus_handle: None,
        fallback_focus_handle: None,
        focus_capture_source: None,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(target),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x909),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert!(!desktop_insert_target_restore_requested(
        target,
        Some(&current_target)
    ));
}

#[test]
fn desktop_output_plan_suppresses_insert_when_foreground_switched_to_another_window() {
    let origin_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x808),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x808),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let current_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x999,
            focus_handle: Some(0xAAA),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0xAAA),
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Win32".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: true,
    };

    assert_eq!(
        desktop_output_plan(
            OutputMode::ClipboardPaste,
            Some(&origin_target),
            Some(&current_target)
        ),
        DesktopOutputPlan {
            strategy: DesktopOutputStrategy::ShowCopyPopupOnly,
            insert_target: None,
        }
    );
}

#[test]
fn hotkey_origin_insert_target_prefers_pretrigger_snapshot_over_release_time_focus() {
    let pending_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 159]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };
    let release_time_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("menu_bar".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 999]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        resolve_hotkey_origin_insert_target(Some(&pending_target), Some(&release_time_target)),
        Some(pending_target)
    );
}

#[test]
fn hotkey_origin_insert_target_falls_back_to_release_time_focus_when_no_pretrigger_snapshot_exists()
{
    let release_time_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: Some(0x808),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Edit".to_string()),
        caret_window_handle: Some(0x808),
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        resolve_hotkey_origin_insert_target(None, Some(&release_time_target)),
        Some(release_time_target)
    );
}

#[test]
fn pending_hotkey_origin_capture_keeps_richer_browser_snapshot_over_later_window_only_capture() {
    let rich_browser_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 159]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };
    let window_only_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        resolve_pending_hotkey_origin_capture(
            Some(&rich_browser_target),
            Some(&window_only_target)
        ),
        Some(rich_browser_target)
    );
}

#[test]
fn pending_hotkey_origin_capture_accepts_richer_candidate_when_existing_snapshot_is_window_only() {
    let window_only_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let rich_browser_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 159]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };

    assert_eq!(
        resolve_pending_hotkey_origin_capture(
            Some(&window_only_target),
            Some(&rich_browser_target)
        ),
        Some(rich_browser_target)
    );
}

#[test]
fn hotkey_recording_origin_enrichment_accepts_richer_same_window_browser_candidate() {
    let window_only_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let rich_browser_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 159]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };

    assert_eq!(
        resolve_hotkey_recording_origin_enrichment(
            Some(&window_only_target),
            Some(&rich_browser_target)
        ),
        Some(rich_browser_target)
    );
}

#[test]
fn hotkey_recording_origin_enrichment_rejects_other_window_candidate() {
    let window_only_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x707,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: None,
        automation_framework_id: None,
        automation_runtime_id: None,
        automation_is_keyboard_focusable: None,
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let other_window_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x808,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 314, 159]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };

    assert_eq!(
        resolve_hotkey_recording_origin_enrichment(
            Some(&window_only_target),
            Some(&other_window_target)
        ),
        Some(window_only_target)
    );
}

#[test]
fn desktop_preferred_paste_shortcut_uses_terminal_style_paste_for_tabby() {
    assert_eq!(
        desktop_preferred_paste_shortcut_for_process_name(Some("tabby")),
        Some("ctrl_shift_v")
    );
    assert_eq!(
        desktop_preferred_paste_shortcut_for_process_name(Some("Tabby.exe")),
        Some("ctrl_shift_v")
    );
}

#[test]
fn desktop_preferred_paste_shortcut_leaves_regular_apps_on_default_ctrl_v() {
    assert_eq!(
        desktop_preferred_paste_shortcut_for_process_name(Some("chrome")),
        None
    );
    assert_eq!(
        desktop_preferred_paste_shortcut_for_process_name(None),
        None
    );
}

#[test]
fn desktop_preferred_paste_shortcut_lets_config_override_force_default_ctrl_v_over_tabby_builtin() {
    let overrides = vec![DesktopPasteShortcutOverride {
        process_name: Some("tabby".to_string()),
        focus_class_name: None,
        automation_framework_id: None,
        automation_control_type: None,
        paste_shortcut: DesktopPasteShortcut::ControlV,
    }];

    assert_eq!(
        desktop_preferred_paste_shortcut_for_target(&overrides, Some("Tabby.exe"), None),
        Some(DesktopPasteShortcut::ControlV)
    );
}

#[test]
fn desktop_preferred_paste_shortcut_can_match_config_override_by_control_traits_without_process_name(
) {
    let overrides = vec![DesktopPasteShortcutOverride {
        process_name: None,
        focus_class_name: None,
        automation_framework_id: Some("WinUI".to_string()),
        automation_control_type: Some("custom".to_string()),
        paste_shortcut: DesktopPasteShortcut::ShiftInsert,
    }];
    let target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0xB0B,
            focus_handle: Some(0xC0C),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Windows.UI.Composition.DesktopWindowContentBridge".to_string()),
        caret_window_handle: None,
        automation_control_type: Some("custom".to_string()),
        automation_framework_id: Some("WinUI".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: true,
    };

    assert_eq!(
        desktop_preferred_paste_shortcut_for_target(&overrides, None, Some(&target)),
        Some(DesktopPasteShortcut::ShiftInsert)
    );
}

#[test]
fn desktop_preferred_paste_shortcut_falls_back_to_builtin_process_override_after_config_miss() {
    let overrides = vec![DesktopPasteShortcutOverride {
        process_name: Some("wezterm".to_string()),
        focus_class_name: None,
        automation_framework_id: None,
        automation_control_type: None,
        paste_shortcut: DesktopPasteShortcut::ShiftInsert,
    }];

    assert_eq!(
        desktop_preferred_paste_shortcut_for_target(&overrides, Some("Tabby.exe"), None),
        Some(DesktopPasteShortcut::ControlShiftV)
    );
}

#[test]
fn desktop_output_strategy_uses_ui_automation_document_signal_for_browser_editors_without_caret() {
    let browser_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x909,
            focus_handle: Some(0xA0A),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Chrome_RenderWidgetHostHWND".to_string()),
        caret_window_handle: None,
        automation_control_type: Some("document".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        decide_desktop_output_strategy(OutputMode::ClipboardPaste, Some(&browser_target)),
        DesktopOutputStrategy::HonorConfiguredOutput
    );
}

#[test]
fn desktop_output_strategy_rejects_non_focusable_document_even_if_ui_automation_reports_text_pattern(
) {
    let browser_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0x909,
            focus_handle: Some(0xA0A),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Chrome_RenderWidgetHostHWND".to_string()),
        caret_window_handle: None,
        automation_control_type: Some("document".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(false),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: false,
    };

    assert_eq!(
        decide_desktop_output_strategy(OutputMode::ClipboardPaste, Some(&browser_target)),
        DesktopOutputStrategy::ShowCopyPopupOnly
    );
}

#[test]
fn desktop_output_strategy_uses_ui_automation_value_pattern_for_custom_text_controls() {
    let custom_target = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0xB0B,
            focus_handle: Some(0xC0C),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: Some("Windows.UI.Composition.DesktopWindowContentBridge".to_string()),
        caret_window_handle: None,
        automation_control_type: Some("custom".to_string()),
        automation_framework_id: Some("WinUI".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: true,
    };

    assert_eq!(
        decide_desktop_output_strategy(OutputMode::ClipboardPaste, Some(&custom_target)),
        DesktopOutputStrategy::HonorConfiguredOutput
    );
}

#[test]
fn shell_state_exposes_start_when_idle_and_stop_when_recording() {
    let idle = ShellState::idle();
    assert!(idle.can_start_session());
    assert!(!idle.can_stop_session());

    let recording = idle.begin_recording().expect("idle should start recording");
    assert!(!recording.can_start_session());
    assert!(recording.can_stop_session());
}

#[test]
fn shell_ignores_duplicate_start_requests_while_busy() {
    let idle = ShellState::idle();
    let recording = idle.begin_recording().expect("idle should start recording");
    assert!(recording.begin_recording().is_none());
}

#[test]
fn hotkey_parser_accepts_ctrl_alt_space() {
    let spec = parse_hotkey("Ctrl+Alt+Space").expect("parse Ctrl+Alt+Space");
    assert_eq!(spec.trigger_key_name(), "Space");
    assert!(spec.has_ctrl());
    assert!(spec.has_alt());
}

#[test]
fn hotkey_parser_accepts_right_alt_as_a_single_toggle_key() {
    let compact = parse_hotkey("RightAlt").expect("parse RightAlt");
    assert_eq!(compact.trigger_key_name(), "RightAlt");
    assert!(!compact.has_ctrl());
    assert!(!compact.has_alt());
    assert!(compact.requires_low_level_hook());

    let spaced = parse_hotkey("Right Alt").expect("parse Right Alt");
    assert_eq!(spaced.trigger_key_name(), "RightAlt");
    assert_eq!(spaced.virtual_key(), compact.virtual_key());
}

#[test]
fn hotkey_parser_accepts_right_alt_slash_combo_as_side_specific_shortcut() {
    let spec = parse_hotkey("RightAlt+/").expect("parse RightAlt+/");
    assert_eq!(spec.display_name(), "RightAlt+Slash");
    assert_eq!(spec.trigger_key_name(), "Slash");
    assert!(spec.has_alt());
    assert!(spec.requires_low_level_hook());
}

#[test]
fn low_level_hotkey_tracker_treats_right_alt_as_a_single_key_toggle_trigger() {
    let spec = parse_hotkey("RightAlt").expect("parse RightAlt");
    let mut tracker = LowLevelHotkeyTracker::new(spec);

    let pressed = tracker.handle_key_event(0xA5, true);
    assert!(pressed.consume);
    assert_eq!(pressed.transition, Some(LowLevelHotkeyTransition::Pressed));

    let repeated = tracker.handle_key_event(0xA5, true);
    assert!(repeated.consume);
    assert_eq!(repeated.transition, None);

    let released = tracker.handle_key_event(0xA5, false);
    assert!(released.consume);
    assert_eq!(
        released.transition,
        Some(LowLevelHotkeyTransition::Released)
    );
}

#[test]
fn low_level_hotkey_tracker_waits_for_trigger_key_when_right_alt_is_a_modifier() {
    let spec = parse_hotkey("RightAlt+/").expect("parse RightAlt+/");
    let mut tracker = LowLevelHotkeyTracker::new(spec);

    let modifier_down = tracker.handle_key_event(0xA5, true);
    assert!(modifier_down.consume);
    assert_eq!(modifier_down.transition, None);

    let trigger_down = tracker.handle_key_event(0xBF, true);
    assert!(trigger_down.consume);
    assert_eq!(
        trigger_down.transition,
        Some(LowLevelHotkeyTransition::Pressed)
    );

    let trigger_up = tracker.handle_key_event(0xBF, false);
    assert!(trigger_up.consume);
    assert_eq!(
        trigger_up.transition,
        Some(LowLevelHotkeyTransition::Released)
    );

    let modifier_up = tracker.handle_key_event(0xA5, false);
    assert!(modifier_up.consume);
    assert_eq!(modifier_up.transition, None);
}

#[test]
fn low_level_hotkey_tracker_does_not_consume_trigger_key_without_required_modifier_prefix() {
    let spec = parse_hotkey("RightAlt+Space").expect("parse RightAlt+Space");
    let mut tracker = LowLevelHotkeyTracker::new(spec);

    let trigger_down = tracker.handle_key_event(0x20, true);
    assert!(!trigger_down.consume);
    assert_eq!(trigger_down.transition, None);

    let trigger_up = tracker.handle_key_event(0x20, false);
    assert!(!trigger_up.consume);
    assert_eq!(trigger_up.transition, None);
}

#[test]
fn low_level_hotkey_tracker_does_not_consume_unrelated_keys_while_right_alt_trigger_is_held() {
    let spec = parse_hotkey("RightAlt").expect("parse RightAlt");
    let mut tracker = LowLevelHotkeyTracker::new(spec);

    let right_alt_down = tracker.handle_key_event(0xA5, true);
    assert!(right_alt_down.consume);
    assert_eq!(
        right_alt_down.transition,
        Some(LowLevelHotkeyTransition::Pressed)
    );

    let ctrl_down = tracker.handle_key_event(0xA2, true);
    assert!(!ctrl_down.consume);
    assert_eq!(ctrl_down.transition, None);

    let digit_down = tracker.handle_key_event(0x31, true);
    assert!(!digit_down.consume);
    assert_eq!(digit_down.transition, None);
}

#[test]
fn windows_hotkey_binding_strategy_uses_low_level_hook_for_right_alt_shortcuts() {
    let side_specific = parse_hotkey("RightAlt+/").expect("parse RightAlt+/");
    assert_eq!(
        select_windows_hotkey_binding_strategy(&side_specific),
        WindowsHotkeyBindingStrategy::LowLevelHook
    );

    let single_right_alt = parse_hotkey("RightAlt").expect("parse RightAlt");
    assert_eq!(
        select_windows_hotkey_binding_strategy(&single_right_alt),
        WindowsHotkeyBindingStrategy::LowLevelHook
    );

    let legacy = parse_hotkey("Ctrl+Alt+F24").expect("parse Ctrl+Alt+F24");
    assert_eq!(
        select_windows_hotkey_binding_strategy(&legacy),
        WindowsHotkeyBindingStrategy::RegisterHotKey
    );
}

#[test]
fn windows_hotkey_registration_plan_adds_origin_capture_for_register_hotkey_shortcuts() {
    let registerable = parse_hotkey("Ctrl+Alt+F15").expect("parse Ctrl+Alt+F15");
    assert_eq!(
        select_windows_hotkey_binding_strategy(&registerable),
        WindowsHotkeyBindingStrategy::RegisterHotKey
    );
    assert_eq!(
        windows_hotkey_binding_registration_plan(&registerable),
        WindowsHotkeyBindingRegistrationPlan::RegisterHotKeyWithOriginCapture
    );

    let side_specific = parse_hotkey("RightAlt").expect("parse RightAlt");
    assert_eq!(
        windows_hotkey_binding_registration_plan(&side_specific),
        WindowsHotkeyBindingRegistrationPlan::LowLevelHook
    );
}

#[test]
fn desktop_action_bindings_expand_typeless_style_shortcuts_into_routes() {
    let config = TalkConfig {
        trigger: TriggerConfig {
            mode: TriggerMode::Toggle,
            toggle_shortcut: "RightAlt".to_string(),
        },
        desktop: DesktopConfig {
            shortcuts: DesktopShortcutConfig {
                translate_shortcut: Some("RightAlt+/".to_string()),
                ask_shortcut: Some("RightAlt+Space".to_string()),
                ..DesktopShortcutConfig::default()
            },
            ..DesktopConfig::default()
        },
        audio: AudioConfig {
            backend: AudioBackendMode::Silent,
            input_device: None,
            max_recording_seconds: 5,
            sample_rate_hz: 16_000,
            channels: 1,
            temp_dir: PathBuf::from(".runtime/talk/audio"),
        },
        provider: ProviderConfig {
            kind: ProviderKind::Mock,
            mock_transcript: Some("hello".to_string()),
            endpoint: None,
            audio_transcriptions_endpoint: None,
            chat_completions_endpoint: None,
            transcription_transport: OpenAiTranscriptionTransport::AudioTranscriptions,
            transcription_model: None,
            chat_model: None,
            api_key: None,
            api_key_env: None,
        },
        output: OutputConfig {
            mode: OutputMode::DryRun,
            restore_clipboard: true,
            clipboard_backend: ClipboardBackendMode::Fallback,
        },
        logging: LoggingConfig {
            dir: PathBuf::from(".runtime/talk/logs"),
        },
        speculative: Default::default(),
        voice_mode: VoiceMode::Dictate,
    };

    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let summary = bindings
        .iter()
        .map(|binding| {
            format!(
                "{}={:?}",
                binding.shortcut.display_name(),
                binding.mode_override
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        summary,
        vec![
            "RightAlt=Some(Dictate)".to_string(),
            "RightAlt+Slash=Some(Translate)".to_string(),
            "RightAlt+Space=Some(Command)".to_string()
        ]
    );
}

#[test]
fn mode_shortcut_bindings_directly_enter_each_user_facing_mode() {
    let config = TalkConfig {
        trigger: TriggerConfig {
            mode: TriggerMode::Toggle,
            toggle_shortcut: "RightAlt".to_string(),
        },
        desktop: DesktopConfig {
            shortcuts: DesktopShortcutConfig {
                transcribe_shortcut: Some("RightCtrl+1".to_string()),
                document_shortcut: Some("RightCtrl+2".to_string()),
                command_shortcut: Some("RightCtrl+3".to_string()),
                generate_shortcut: Some("RightCtrl+4".to_string()),
                smart_shortcut: Some("RightCtrl+5".to_string()),
                ..DesktopShortcutConfig::default()
            },
            ..DesktopConfig::default()
        },
        audio: AudioConfig {
            backend: AudioBackendMode::Silent,
            input_device: None,
            max_recording_seconds: 5,
            sample_rate_hz: 16_000,
            channels: 1,
            temp_dir: PathBuf::from(".runtime/talk/audio"),
        },
        provider: ProviderConfig {
            kind: ProviderKind::Mock,
            mock_transcript: Some("hello".to_string()),
            endpoint: None,
            audio_transcriptions_endpoint: None,
            chat_completions_endpoint: None,
            transcription_transport: OpenAiTranscriptionTransport::AudioTranscriptions,
            transcription_model: None,
            chat_model: None,
            api_key: None,
            api_key_env: None,
        },
        output: OutputConfig {
            mode: OutputMode::DryRun,
            restore_clipboard: true,
            clipboard_backend: ClipboardBackendMode::Fallback,
        },
        logging: LoggingConfig {
            dir: PathBuf::from(".runtime/talk/logs"),
        },
        speculative: Default::default(),
        voice_mode: VoiceMode::Smart,
    };

    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let summary = bindings
        .iter()
        .map(|binding| {
            format!(
                "{}={:?}",
                binding.shortcut.display_name(),
                binding.mode_override
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        summary,
        vec![
            "RightAlt=Some(Smart)".to_string(),
            "RightCtrl+1=Some(Transcribe)".to_string(),
            "RightCtrl+2=Some(Document)".to_string(),
            "RightCtrl+3=Some(Command)".to_string(),
            "RightCtrl+4=Some(Generate)".to_string(),
            "RightCtrl+5=Some(Smart)".to_string(),
        ]
    );
}

fn typeless_desktop_shortcuts_config(
    translate_shortcut: Option<&str>,
    ask_shortcut: Option<&str>,
) -> TalkConfig {
    TalkConfig {
        trigger: TriggerConfig {
            mode: TriggerMode::Toggle,
            toggle_shortcut: "RightAlt".to_string(),
        },
        desktop: DesktopConfig {
            shortcuts: DesktopShortcutConfig {
                translate_shortcut: translate_shortcut.map(str::to_string),
                ask_shortcut: ask_shortcut.map(str::to_string),
                ..DesktopShortcutConfig::default()
            },
            ..DesktopConfig::default()
        },
        audio: AudioConfig {
            backend: AudioBackendMode::Silent,
            input_device: None,
            max_recording_seconds: 5,
            sample_rate_hz: 16_000,
            channels: 1,
            temp_dir: PathBuf::from(".runtime/talk/audio"),
        },
        provider: ProviderConfig {
            kind: ProviderKind::Mock,
            mock_transcript: Some("hello".to_string()),
            endpoint: None,
            audio_transcriptions_endpoint: None,
            chat_completions_endpoint: None,
            transcription_transport: OpenAiTranscriptionTransport::AudioTranscriptions,
            transcription_model: None,
            chat_model: None,
            api_key: None,
            api_key_env: None,
        },
        output: OutputConfig {
            mode: OutputMode::DryRun,
            restore_clipboard: true,
            clipboard_backend: ClipboardBackendMode::Fallback,
        },
        logging: LoggingConfig {
            dir: PathBuf::from(".runtime/talk/logs"),
        },
        speculative: Default::default(),
        voice_mode: VoiceMode::Dictate,
    }
}

#[test]
fn shortcut_help_model_uses_typeless_style_entries_for_right_alt_routes() {
    let config = typeless_desktop_shortcuts_config(Some("RightAlt+/"), Some("RightAlt+Space"));
    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");

    assert_eq!(
        desktop_shortcut_help_model(&bindings),
        DesktopShortcutHelpModel {
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
                }
            ],
        }
    );
}

#[test]
fn shortcut_help_overlay_uses_non_activating_bottom_card_contract() {
    assert_eq!(
        desktop_shortcut_help_activation_policy(),
        DesktopOverlayActivationPolicy::NoActivate
    );
    assert_eq!(
        desktop_shortcut_help_metrics(),
        DesktopShortcutHelpMetrics {
            width: 420,
            height: 184,
            bottom_margin: 36,
        }
    );
    assert_eq!(
        desktop_shortcut_help_position(1920, 1080, 420, 184, 36),
        DesktopOverlayPosition { x: 750, y: 860 }
    );
}

#[test]
fn toggle_router_defers_right_alt_until_release_when_longer_chords_share_the_prefix() {
    let config = TalkConfig {
        trigger: TriggerConfig {
            mode: TriggerMode::Toggle,
            toggle_shortcut: "RightAlt".to_string(),
        },
        desktop: DesktopConfig {
            shortcuts: DesktopShortcutConfig {
                translate_shortcut: Some("RightAlt+/".to_string()),
                ask_shortcut: Some("RightAlt+Space".to_string()),
                ..DesktopShortcutConfig::default()
            },
            ..DesktopConfig::default()
        },
        audio: AudioConfig {
            backend: AudioBackendMode::Silent,
            input_device: None,
            max_recording_seconds: 5,
            sample_rate_hz: 16_000,
            channels: 1,
            temp_dir: PathBuf::from(".runtime/talk/audio"),
        },
        provider: ProviderConfig {
            kind: ProviderKind::Mock,
            mock_transcript: Some("hello".to_string()),
            endpoint: None,
            audio_transcriptions_endpoint: None,
            chat_completions_endpoint: None,
            transcription_transport: OpenAiTranscriptionTransport::AudioTranscriptions,
            transcription_model: None,
            chat_model: None,
            api_key: None,
            api_key_env: None,
        },
        output: OutputConfig {
            mode: OutputMode::DryRun,
            restore_clipboard: true,
            clipboard_backend: ClipboardBackendMode::Fallback,
        },
        logging: LoggingConfig {
            dir: PathBuf::from(".runtime/talk/logs"),
        },
        speculative: Default::default(),
        voice_mode: VoiceMode::Dictate,
    };

    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let mut router = ToggleDesktopHotkeyRouter::new(&bindings);

    let right_alt_down = router.handle_key_event(0xA5, true);
    assert!(right_alt_down.consume);
    assert_eq!(right_alt_down.action_index, None);
    assert_eq!(
        right_alt_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Start {
            trigger_virtual_key: 0xA5,
        }
    );

    let right_alt_up = router.handle_key_event(0xA5, false);
    assert!(right_alt_up.consume);
    assert_eq!(right_alt_up.action_index, Some(0));
    assert_eq!(
        right_alt_up.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Cancelled
    );
}

#[test]
fn toggle_router_suppresses_right_alt_release_after_hold_help_was_shown() {
    let config = typeless_desktop_shortcuts_config(Some("RightAlt+/"), Some("RightAlt+Space"));
    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let mut router = ToggleDesktopHotkeyRouter::new(&bindings);

    assert_eq!(
        router.handle_key_event(0xA5, true).pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Start {
            trigger_virtual_key: 0xA5,
        }
    );
    assert!(router.activate_pending_hold_help());

    let right_alt_up = router.handle_key_event(0xA5, false);
    assert!(right_alt_up.consume);
    assert_eq!(right_alt_up.action_index, None);
    assert_eq!(
        right_alt_up.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Cancelled
    );
}

#[test]
fn toggle_router_cancels_hold_help_when_right_alt_combo_executes_another_action() {
    let config = typeless_desktop_shortcuts_config(Some("RightAlt+/"), None);
    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let mut router = ToggleDesktopHotkeyRouter::new(&bindings);

    assert_eq!(
        router.handle_key_event(0xA5, true).pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Start {
            trigger_virtual_key: 0xA5,
        }
    );

    let slash_down = router.handle_key_event(0xBF, true);
    assert!(slash_down.consume);
    assert_eq!(slash_down.action_index, Some(1));
    assert_eq!(
        slash_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Cancelled
    );
}

#[test]
fn toggle_router_prefers_right_alt_space_combo_over_single_right_alt() {
    let config = TalkConfig {
        trigger: TriggerConfig {
            mode: TriggerMode::Toggle,
            toggle_shortcut: "RightAlt".to_string(),
        },
        desktop: DesktopConfig {
            shortcuts: DesktopShortcutConfig {
                translate_shortcut: None,
                ask_shortcut: Some("RightAlt+Space".to_string()),
                ..DesktopShortcutConfig::default()
            },
            ..DesktopConfig::default()
        },
        audio: AudioConfig {
            backend: AudioBackendMode::Silent,
            input_device: None,
            max_recording_seconds: 5,
            sample_rate_hz: 16_000,
            channels: 1,
            temp_dir: PathBuf::from(".runtime/talk/audio"),
        },
        provider: ProviderConfig {
            kind: ProviderKind::Mock,
            mock_transcript: Some("hello".to_string()),
            endpoint: None,
            audio_transcriptions_endpoint: None,
            chat_completions_endpoint: None,
            transcription_transport: OpenAiTranscriptionTransport::AudioTranscriptions,
            transcription_model: None,
            chat_model: None,
            api_key: None,
            api_key_env: None,
        },
        output: OutputConfig {
            mode: OutputMode::DryRun,
            restore_clipboard: true,
            clipboard_backend: ClipboardBackendMode::Fallback,
        },
        logging: LoggingConfig {
            dir: PathBuf::from(".runtime/talk/logs"),
        },
        speculative: Default::default(),
        voice_mode: VoiceMode::Dictate,
    };

    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let mut router = ToggleDesktopHotkeyRouter::new(&bindings);

    assert_eq!(router.handle_key_event(0xA5, true).action_index, None);
    assert_eq!(router.handle_key_event(0x20, true).action_index, Some(1));
    assert_eq!(router.handle_key_event(0x20, false).action_index, None);
    assert_eq!(router.handle_key_event(0xA5, false).action_index, None);
}

#[test]
fn toggle_router_does_not_consume_plain_space_or_slash_without_right_alt_prefix() {
    let config = typeless_desktop_shortcuts_config(Some("RightAlt+/"), Some("RightAlt+Space"));
    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let mut router = ToggleDesktopHotkeyRouter::new(&bindings);

    let space_down = router.handle_key_event(0x20, true);
    assert!(!space_down.consume);
    assert_eq!(space_down.action_index, None);
    assert_eq!(
        space_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::None
    );
    let space_up = router.handle_key_event(0x20, false);
    assert!(!space_up.consume);
    assert_eq!(space_up.action_index, None);

    let slash_down = router.handle_key_event(0xBF, true);
    assert!(!slash_down.consume);
    assert_eq!(slash_down.action_index, None);
    assert_eq!(
        slash_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::None
    );
    let slash_up = router.handle_key_event(0xBF, false);
    assert!(!slash_up.consume);
    assert_eq!(slash_up.action_index, None);
}

#[test]
fn toggle_router_cancels_pending_right_alt_action_when_ctrl_shortcut_commits_on_an_unrelated_key() {
    let config = typeless_desktop_shortcuts_config(Some("RightAlt+/"), Some("RightAlt+Space"));
    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let mut router = ToggleDesktopHotkeyRouter::new(&bindings);

    let right_alt_down = router.handle_key_event(0xA5, true);
    assert!(right_alt_down.consume);
    assert_eq!(
        right_alt_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Start {
            trigger_virtual_key: 0xA5,
        }
    );

    let ctrl_down = router.handle_key_event(0xA2, true);
    assert!(!ctrl_down.consume);
    assert_eq!(ctrl_down.action_index, None);
    assert_eq!(
        ctrl_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::None
    );

    let digit_down = router.handle_key_event(0x31, true);
    assert!(!digit_down.consume);
    assert_eq!(digit_down.action_index, None);
    assert_eq!(
        digit_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Cancelled
    );

    let right_alt_up = router.handle_key_event(0xA5, false);
    assert!(!right_alt_up.consume);
    assert_eq!(right_alt_up.action_index, None);
}

#[test]
fn toggle_router_does_not_consume_right_alt_release_after_pending_action_was_cancelled() {
    let config = typeless_desktop_shortcuts_config(Some("RightAlt+/"), Some("RightAlt+Space"));
    let bindings = desktop_action_bindings(&config).expect("desktop action bindings");
    let mut router = ToggleDesktopHotkeyRouter::new(&bindings);

    assert_eq!(
        router.handle_key_event(0xA5, true).pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Start {
            trigger_virtual_key: 0xA5,
        }
    );

    let ctrl_down = router.handle_key_event(0xA2, true);
    assert!(!ctrl_down.consume);
    assert_eq!(
        ctrl_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::None
    );

    let digit_down = router.handle_key_event(0x31, true);
    assert!(!digit_down.consume);
    assert_eq!(
        digit_down.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::Cancelled
    );

    let digit_up = router.handle_key_event(0x31, false);
    assert!(!digit_up.consume);
    assert_eq!(
        digit_up.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::None
    );

    let ctrl_up = router.handle_key_event(0xA2, false);
    assert!(!ctrl_up.consume);
    assert_eq!(
        ctrl_up.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::None
    );

    let right_alt_up = router.handle_key_event(0xA5, false);
    assert!(!right_alt_up.consume);
    assert_eq!(right_alt_up.action_index, None);
    assert_eq!(
        right_alt_up.pending_hold,
        ToggleDesktopHotkeyRouterPendingHold::None
    );
}

#[test]
fn tray_menu_model_allows_recovery_when_hotkey_registration_failed() {
    let state = ShellState::idle();
    let config = ConfigAvailability::ready();
    let hotkey = HotkeyBindingState::registration_failed("Ctrl+Alt+Space", "already in use");
    let menu = tray_menu_model(&state, &config, &hotkey, None);

    assert!(menu.start_enabled);
    assert!(!menu.stop_enabled);
    assert!(!menu.cancel_enabled);
    assert!(menu.reload_config_enabled);
    assert!(menu.open_config_enabled);
    assert_eq!(menu.hotkey_label, "Hotkey unavailable: Ctrl+Alt+Space");
    assert_eq!(menu.detail_label.as_deref(), Some("already in use"));
}

#[test]
fn startup_status_message_reports_invalid_shortcut_without_crashing_shell() {
    let hotkey =
        HotkeyBindingState::invalid_config("Ctrl + + Space", "shortcut contains an empty segment");

    assert_eq!(
        hotkey_status_message(&hotkey),
        Some("Talk: hotkey config invalid")
    );
}

#[test]
fn tray_menu_model_disables_start_when_config_is_unavailable() {
    let state = ShellState::idle();
    let config = ConfigAvailability::unavailable("failed to parse config");
    let hotkey = HotkeyBindingState::Unconfigured;
    let menu = tray_menu_model(&state, &config, &hotkey, None);

    assert!(!menu.start_enabled);
    assert!(!menu.stop_enabled);
    assert!(!menu.cancel_enabled);
    assert!(menu.reload_config_enabled);
    assert!(menu.open_config_enabled);
    assert_eq!(menu.hotkey_label, "Config unavailable");
    assert_eq!(
        config_status_message(&config),
        Some("Talk: config unavailable")
    );
    assert_eq!(
        idle_status_detail(&config, &hotkey, None).as_deref(),
        Some("failed to parse config")
    );
}

#[test]
fn tray_menu_model_enables_cancel_while_recording() {
    let state = ShellState::idle()
        .begin_recording()
        .expect("idle state should become recording");
    let config = ConfigAvailability::ready();
    let hotkey =
        HotkeyBindingState::active(parse_hotkey("Ctrl+Alt+F24").expect("parse Ctrl+Alt+F24"));
    let menu = tray_menu_model(&state, &config, &hotkey, None);

    assert!(!menu.start_enabled);
    assert!(menu.stop_enabled);
    assert!(menu.cancel_enabled);
}

#[test]
fn compose_hud_message_appends_detail_line_when_present() {
    assert_eq!(
        compose_hud_message("Talk: config unavailable", Some("failed to parse config")),
        "Talk: config unavailable\nfailed to parse config"
    );
    assert_eq!(compose_hud_message("Talk: done", None), "Talk: done");
}

#[test]
fn status_report_includes_current_and_last_session_details() {
    let snapshot = StatusSnapshot {
        current_summary: "Talk: hotkey unavailable".to_string(),
        current_detail: Some("Ctrl+Alt+Space is already in use".to_string()),
        config_path: "C:\\Talk\\dev-config.toml".to_string(),
        logs_dir: "C:\\Talk\\.runtime\\talk\\logs".to_string(),
        hotkey_label: "Ctrl+Alt+Space".to_string(),
        hotkey_detail: Some("already in use".to_string()),
        last_session: Some(LastSessionStatus {
            summary: "cancelled".to_string(),
            detail: Some("user cancelled during recording".to_string()),
        }),
        native_readiness: None,
    };

    let report = build_status_report(&snapshot);
    assert!(report.contains("Current: Talk: hotkey unavailable"));
    assert!(report.contains("Current detail: Ctrl+Alt+Space is already in use"));
    assert!(report.contains("Hotkey: Ctrl+Alt+Space"));
    assert!(report.contains("Hotkey detail: already in use"));
    assert!(report.contains("Last session: cancelled"));
    assert!(report.contains("Last session detail: user cancelled during recording"));
}

#[test]
fn status_report_includes_configured_native_backend_readiness() {
    let snapshot = StatusSnapshot {
        current_summary: "Talk: native unavailable".to_string(),
        current_detail: Some(
            "native_windows audio backend: no default input device is available".to_string(),
        ),
        config_path: "C:\\Talk\\dev-config.toml".to_string(),
        logs_dir: "C:\\Talk\\.runtime\\talk\\logs".to_string(),
        hotkey_label: "Ctrl+Alt+F24".to_string(),
        hotkey_detail: None,
        last_session: None,
        native_readiness: Some(NativeReadinessSnapshot {
            audio: NativeBackendSnapshot {
                configured_backend: "native_windows".to_string(),
                status: Some(NativeReadinessStatus::Unavailable),
                detail: Some(
                    "native_windows audio backend: no default input device is available"
                        .to_string(),
                ),
            },
            clipboard: NativeBackendSnapshot {
                configured_backend: "native_windows".to_string(),
                status: Some(NativeReadinessStatus::Ready),
                detail: Some("Windows clipboard path is callable".to_string()),
            },
        }),
    };

    let report = build_status_report(&snapshot);
    assert!(report.contains("Audio backend: native_windows"));
    assert!(report.contains("Audio backend readiness: unavailable"));
    assert!(report.contains(
        "Audio backend detail: native_windows audio backend: no default input device is available"
    ));
    assert!(report.contains("Clipboard backend: native_windows"));
    assert!(report.contains("Clipboard backend readiness: ready"));
    assert!(report.contains("Clipboard backend detail: Windows clipboard path is callable"));
}

#[test]
fn tray_menu_model_surfaces_native_issue_when_idle_and_otherwise_ready() {
    let state = ShellState::idle();
    let config = ConfigAvailability::ready();
    let hotkey =
        HotkeyBindingState::active(parse_hotkey("Ctrl+Alt+F24").expect("parse Ctrl+Alt+F24"));
    let native_readiness = NativeReadinessSnapshot {
        audio: NativeBackendSnapshot {
            configured_backend: "native_windows".to_string(),
            status: Some(NativeReadinessStatus::Unavailable),
            detail: Some(
                "native_windows audio backend: no default input device is available".to_string(),
            ),
        },
        clipboard: NativeBackendSnapshot {
            configured_backend: "native_windows".to_string(),
            status: Some(NativeReadinessStatus::Ready),
            detail: None,
        },
    };

    let menu = tray_menu_model(&state, &config, &hotkey, Some(&native_readiness));

    assert_eq!(
        native_status_message(Some(&native_readiness)),
        Some("Talk: native unavailable")
    );
    assert_eq!(
        idle_status_detail(&config, &hotkey, Some(&native_readiness)).as_deref(),
        Some("native_windows audio backend: no default input device is available")
    );
    assert_eq!(
        menu.detail_label.as_deref(),
        Some("native_windows audio backend: no default input device is available")
    );
}

#[test]
fn select_foreground_insert_target_ignores_shell_and_hud_windows() {
    assert_eq!(select_foreground_insert_target(0, None, 101, 202), None);
    assert_eq!(select_foreground_insert_target(101, None, 101, 202), None);
    assert_eq!(select_foreground_insert_target(202, None, 101, 202), None);
    assert_eq!(
        select_foreground_insert_target(303, None, 101, 202),
        Some(ForegroundInsertTarget {
            window_handle: 303,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        })
    );
}

#[test]
fn select_foreground_insert_target_keeps_distinct_focused_child_handle() {
    assert_eq!(
        select_foreground_insert_target(303, Some(404), 101, 202),
        Some(ForegroundInsertTarget {
            window_handle: 303,
            focus_handle: Some(404),
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        })
    );
}

#[test]
fn select_foreground_insert_target_discards_unusable_focus_handles() {
    assert_eq!(
        select_foreground_insert_target(303, Some(303), 101, 202),
        Some(ForegroundInsertTarget {
            window_handle: 303,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        })
    );
    assert_eq!(
        select_foreground_insert_target(303, Some(101), 101, 202),
        Some(ForegroundInsertTarget {
            window_handle: 303,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        })
    );
    assert_eq!(
        select_foreground_insert_target(303, Some(202), 101, 202),
        Some(ForegroundInsertTarget {
            window_handle: 303,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        })
    );
    assert_eq!(
        select_foreground_insert_target(303, Some(0), 101, 202),
        Some(ForegroundInsertTarget {
            window_handle: 303,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        })
    );
}

#[test]
fn resolve_foreground_focus_handle_prefers_primary_when_it_is_usable() {
    assert_eq!(
        resolve_foreground_focus_handle(303, Some(404), Some(505), 101, 202),
        Some(404)
    );
}

#[test]
fn resolve_foreground_focus_handle_uses_fallback_when_primary_is_missing() {
    assert_eq!(
        resolve_foreground_focus_handle(303, None, Some(505), 101, 202),
        Some(505)
    );
}

#[test]
fn resolve_foreground_focus_handle_uses_fallback_when_primary_matches_foreground() {
    assert_eq!(
        resolve_foreground_focus_handle(303, Some(303), Some(505), 101, 202),
        Some(505)
    );
}

#[test]
fn resolve_foreground_focus_handle_discards_unusable_primary_and_fallback_handles() {
    assert_eq!(
        resolve_foreground_focus_handle(303, Some(101), Some(202), 101, 202),
        None
    );
    assert_eq!(
        resolve_foreground_focus_handle(303, Some(0), Some(303), 101, 202),
        None
    );
}

#[test]
fn resolve_foreground_focus_capture_reports_primary_and_fallback_sources() {
    let primary = resolve_foreground_focus_capture(303, Some(404), Some(505), 101, 202);
    assert_eq!(primary.focus_handle, Some(404));
    assert_eq!(
        primary.source,
        Some(ForegroundFocusCaptureSource::GuiThreadInfo)
    );

    let fallback = resolve_foreground_focus_capture(303, Some(303), Some(505), 101, 202);
    assert_eq!(fallback.focus_handle, Some(505));
    assert_eq!(
        fallback.source,
        Some(ForegroundFocusCaptureSource::AttachedGetFocus)
    );
}

#[test]
fn observe_foreground_target_stability_tracks_total_and_trailing_target_polls() {
    let target_window_handle = 0x303;
    let progress = [0x303, 0x303, 0x404, 0x303, 0x303].into_iter().fold(
        ForegroundTargetStabilityProgress::default(),
        |progress, observed| {
            observe_foreground_target_stability(progress, target_window_handle, observed)
        },
    );

    assert_eq!(progress.poll_count, 5);
    assert_eq!(progress.target_foreground_poll_count, 4);
    assert_eq!(progress.trailing_target_foreground_poll_count, 2);
}

#[test]
fn foreground_target_stability_requires_consecutive_target_polls() {
    let target_window_handle = 0x303;
    let progress = [0x303, 0x404, 0x303, 0x303, 0x303, 0x303].into_iter().fold(
        ForegroundTargetStabilityProgress::default(),
        |progress, observed| {
            observe_foreground_target_stability(progress, target_window_handle, observed)
        },
    );

    assert!(foreground_target_stability_satisfied(progress, 4));
    assert!(!foreground_target_stability_satisfied(progress, 5));
}

#[test]
fn foreground_target_refresh_requested_when_post_insert_target_loses_foreground() {
    assert!(!foreground_target_refresh_requested(0, 0x404));
    assert!(!foreground_target_refresh_requested(0x303, 0x303));
    assert!(foreground_target_refresh_requested(0x303, 0x404));
    assert!(foreground_target_refresh_requested(0x303, 0));
}

#[test]
fn hydrate_foreground_insert_target_focus_backfills_missing_focus_from_refreshed_primary_capture() {
    let target = ForegroundInsertTarget {
        window_handle: 0x303,
        focus_handle: None,
        primary_focus_handle: None,
        fallback_focus_handle: None,
        focus_capture_source: None,
    };

    let hydrated =
        hydrate_foreground_insert_target_focus(target, Some(0x404), Some(0x505), 0x101, 0x202);

    assert_eq!(
        hydrated,
        ForegroundInsertTarget {
            window_handle: 0x303,
            focus_handle: Some(0x404),
            primary_focus_handle: Some(0x404),
            fallback_focus_handle: Some(0x505),
            focus_capture_source: Some(ForegroundFocusCaptureSource::GuiThreadInfo),
        }
    );
}

#[test]
fn hydrate_foreground_insert_target_focus_keeps_existing_focus_when_target_already_has_one() {
    let target = ForegroundInsertTarget {
        window_handle: 0x303,
        focus_handle: Some(0x444),
        primary_focus_handle: Some(0x444),
        fallback_focus_handle: None,
        focus_capture_source: Some(ForegroundFocusCaptureSource::GuiThreadInfo),
    };

    let hydrated =
        hydrate_foreground_insert_target_focus(target, Some(0x404), Some(0x505), 0x101, 0x202);

    assert_eq!(hydrated, target);
}

#[test]
fn parse_desktop_window_handle_accepts_hex_and_decimal_values() {
    assert_eq!(
        parse_desktop_window_handle("0x500D56").expect("parse hex handle"),
        0x500D56
    );
    assert_eq!(
        parse_desktop_window_handle("5244246").expect("parse decimal handle"),
        5_244_246
    );
}

#[test]
fn parse_desktop_window_handle_rejects_blank_and_invalid_values() {
    assert!(parse_desktop_window_handle("").is_err());
    assert!(parse_desktop_window_handle("  ").is_err());
    assert!(parse_desktop_window_handle("not-a-handle").is_err());
}

#[test]
fn desktop_audio_override_resolves_relative_file_against_config_dir() {
    let temp_dir = std::env::temp_dir().join(format!(
        "talk-desktop-audio-override-{}",
        std::process::id()
    ));
    let config_dir = temp_dir.join("config-root");
    let audio_dir = config_dir.join("fixtures");
    fs::create_dir_all(&audio_dir).expect("create audio dir");
    let config_path = config_dir.join("config.toml");
    let audio_path = audio_dir.join("spoken.wav");
    fs::write(&audio_path, b"fake wav").expect("write audio fixture");

    let resolved = resolve_desktop_audio_file_override(Some("fixtures/spoken.wav"), &config_path)
        .expect("override should resolve")
        .expect("override path should exist");

    assert_eq!(resolved, audio_path);
}

#[test]
fn desktop_audio_override_rejects_missing_file() {
    let config_path = PathBuf::from("C:/Talk/config.toml");

    let error = resolve_desktop_audio_file_override(Some("fixtures/missing.wav"), &config_path)
        .expect_err("missing override file should fail");

    assert!(
        error.contains("Talk desktop audio override file does not exist"),
        "error={error}"
    );
}

fn unique_temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}-{}-{}",
        label,
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ))
}

#[test]
fn packaged_local_asr_daemon_launch_plan_finds_release_internal_daemon() {
    let temp_dir = unique_temp_dir("talk-desktop-local-asr-release");
    let release_dir = temp_dir.join("release");
    let internal_dir = release_dir.join(".internal");
    fs::create_dir_all(&internal_dir).expect("create internal dir");
    let daemon_path = internal_dir.join("talk-local-asr-sherpa.exe");
    fs::write(&daemon_path, b"fake exe").expect("write daemon marker");
    let executable_path = release_dir.join("talk-desktop.exe");

    let plan =
        desktop_packaged_local_asr_daemon_launch_plan(&executable_path, "ws://127.0.0.1:53171/asr")
            .expect("valid launch plan")
            .expect("packaged daemon should be found");

    assert_eq!(
        plan,
        DesktopLocalAsrDaemonLaunchPlan {
            executable_path: daemon_path,
            bind: "127.0.0.1:53171".to_string(),
            args: vec!["--bind".to_string(), "127.0.0.1:53171".to_string()],
        }
    );
}

#[test]
fn packaged_local_asr_daemon_launch_plan_adds_sherpa_model_args_from_config() {
    let temp_dir = unique_temp_dir("talk-desktop-local-asr-sherpa-config");
    let release_dir = temp_dir.join("release");
    let internal_dir = release_dir.join(".internal");
    fs::create_dir_all(&internal_dir).expect("create internal dir");
    let daemon_path = internal_dir.join("talk-local-asr-sherpa.exe");
    fs::write(&daemon_path, b"fake exe").expect("write daemon marker");
    let executable_path = release_dir.join("talk-desktop.exe");
    let daemon_config = SpeculativeLocalAsrDaemonConfig {
        mode: SpeculativeLocalAsrDaemonMode::SherpaOnline,
        engine: None,
        model: Some("zipformer-bilingual-zh-en".to_string()),
        dry_run_text: None,
        dry_run_partial_text: None,
        model_family: SpeculativeSherpaOnlineModelFamily::Transducer,
        tokens: Some(PathBuf::from("C:/models/zipformer/tokens.txt")),
        encoder: Some(PathBuf::from("C:/models/zipformer/encoder.onnx")),
        decoder: Some(PathBuf::from("C:/models/zipformer/decoder.onnx")),
        joiner: Some(PathBuf::from("C:/models/zipformer/joiner.onnx")),
        provider: Some("cpu".to_string()),
        num_threads: Some(4),
        sample_rate_hz: Some(16_000),
        decoding_method: Some("modified_beam_search".to_string()),
        hotwords_file: None,
        rule_fsts: None,
        rule_fars: None,
    };

    let plan = desktop_packaged_local_asr_daemon_launch_plan_with_config(
        &executable_path,
        "ws://127.0.0.1:53171/asr",
        Some(&daemon_config),
    )
    .expect("valid launch plan")
    .expect("packaged daemon should be found");

    assert_eq!(plan.executable_path, daemon_path);
    assert_eq!(plan.bind, "127.0.0.1:53171");
    assert_eq!(
        plan.args,
        vec![
            "--bind",
            "127.0.0.1:53171",
            "--mode",
            "sherpa-online",
            "--model",
            "zipformer-bilingual-zh-en",
            "--model-family",
            "transducer",
            "--tokens",
            "C:/models/zipformer/tokens.txt",
            "--encoder",
            "C:/models/zipformer/encoder.onnx",
            "--decoder",
            "C:/models/zipformer/decoder.onnx",
            "--joiner",
            "C:/models/zipformer/joiner.onnx",
            "--provider",
            "cpu",
            "--num-threads",
            "4",
            "--sample-rate-hz",
            "16000",
            "--decoding-method",
            "modified_beam_search",
        ]
    );
}

#[test]
fn packaged_local_asr_daemon_launch_plan_auto_uses_installed_release_zipformer_model() {
    let temp_dir = unique_temp_dir("talk-desktop-local-asr-auto-model");
    let release_dir = temp_dir.join("release");
    let internal_dir = release_dir.join(".internal");
    let model_dir = release_dir
        .join(".runtime")
        .join("models")
        .join("sherpa-onnx")
        .join("zipformer-zh-en-punct-int8-480ms");
    fs::create_dir_all(&internal_dir).expect("create internal dir");
    fs::create_dir_all(&model_dir).expect("create model dir");
    let daemon_path = internal_dir.join("talk-local-asr-sherpa.exe");
    fs::write(&daemon_path, b"fake exe").expect("write daemon marker");
    fs::write(model_dir.join("tokens.txt"), b"tokens").expect("write tokens");
    fs::write(model_dir.join("encoder.int8.onnx"), b"encoder").expect("write encoder");
    fs::write(model_dir.join("decoder.onnx"), b"decoder").expect("write decoder");
    fs::write(model_dir.join("joiner.int8.onnx"), b"joiner").expect("write joiner");
    let executable_path = release_dir.join("talk-desktop.exe");

    let plan = desktop_packaged_local_asr_daemon_launch_plan_with_config(
        &executable_path,
        "ws://127.0.0.1:53171/asr",
        None,
    )
    .expect("valid launch plan")
    .expect("packaged daemon should be found");

    assert_eq!(plan.executable_path, daemon_path);
    assert!(plan.args.iter().any(|arg| arg == "--mode"));
    assert!(plan.args.iter().any(|arg| arg == "sherpa-online"));
    assert!(plan
        .args
        .iter()
        .any(|arg| arg == "zipformer-zh-en-punct-int8-480ms"));
    assert!(plan
        .args
        .iter()
        .any(|arg| arg.ends_with("encoder.int8.onnx")));
    assert!(!plan.args.iter().any(|arg| arg == "dry-run"));
}

#[test]
fn product_local_asr_launch_plan_uses_extracted_worker_and_app_data_model_root() {
    let temp_dir = unique_temp_dir("talk-desktop-product-local-asr");
    let runtime_dir = temp_dir.join("runtime").join("payload-hash");
    let model_root = temp_dir.join("models").join("sherpa-onnx");
    let model_dir = model_root.join("zipformer-zh-en-punct-int8-480ms");
    fs::create_dir_all(&runtime_dir).expect("create runtime dir");
    fs::create_dir_all(&model_dir).expect("create model dir");
    let worker_path = runtime_dir.join("talk-local-asr-sherpa.exe");
    fs::write(&worker_path, b"worker").expect("write worker");
    fs::write(model_dir.join("tokens.txt"), b"tokens").expect("write tokens");
    fs::write(model_dir.join("encoder.int8.onnx"), b"encoder").expect("write encoder");
    fs::write(model_dir.join("decoder.onnx"), b"decoder").expect("write decoder");
    fs::write(model_dir.join("joiner.int8.onnx"), b"joiner").expect("write joiner");

    let plan = desktop_product_local_asr_daemon_launch_plan_with_config(
        &worker_path,
        &model_root,
        "ws://127.0.0.1:53171/asr",
        None,
    )
    .expect("valid product launch plan")
    .expect("product worker should be available");

    assert_eq!(plan.executable_path, worker_path);
    assert!(plan
        .args
        .iter()
        .any(|arg| arg.ends_with("encoder.int8.onnx")));
    fs::remove_dir_all(temp_dir).expect("remove product launch fixture");
}

#[test]
fn local_asr_unavailable_disables_streaming_and_preserves_cloud_fallback() {
    assert!(desktop_effective_streaming_asr_enabled(
        DesktopSpeculativeLocalAsrRoute::StreamingService,
        true,
    ));
    assert!(!desktop_effective_streaming_asr_enabled(
        DesktopSpeculativeLocalAsrRoute::StreamingService,
        false,
    ));
}

#[test]
fn packaged_local_asr_daemon_launch_plan_auto_falls_back_to_installed_release_paraformer_model() {
    let temp_dir = unique_temp_dir("talk-desktop-local-asr-auto-paraformer");
    let release_dir = temp_dir.join("release");
    let internal_dir = release_dir.join(".internal");
    let model_dir = release_dir
        .join(".runtime")
        .join("models")
        .join("sherpa-onnx")
        .join("paraformer-bilingual-zh-en");
    fs::create_dir_all(&internal_dir).expect("create internal dir");
    fs::create_dir_all(&model_dir).expect("create model dir");
    let daemon_path = internal_dir.join("talk-local-asr-sherpa.exe");
    fs::write(&daemon_path, b"fake exe").expect("write daemon marker");
    fs::write(model_dir.join("tokens.txt"), b"tokens").expect("write tokens");
    fs::write(model_dir.join("encoder.int8.onnx"), b"encoder").expect("write encoder");
    fs::write(model_dir.join("decoder.int8.onnx"), b"decoder").expect("write decoder");
    let executable_path = release_dir.join("talk-desktop.exe");

    let plan = desktop_packaged_local_asr_daemon_launch_plan_with_config(
        &executable_path,
        "ws://127.0.0.1:53171/asr",
        None,
    )
    .expect("valid launch plan")
    .expect("packaged daemon should be found");

    assert_eq!(plan.executable_path, daemon_path);
    assert!(plan.args.iter().any(|arg| arg == "sherpa-online"));
    assert!(plan.args.iter().any(|arg| arg == "paraformer"));
    assert!(plan
        .args
        .iter()
        .any(|arg| arg == "paraformer-bilingual-zh-en"));
    assert!(plan
        .args
        .iter()
        .any(|arg| arg.ends_with("encoder.int8.onnx")));
    assert!(plan
        .args
        .iter()
        .any(|arg| arg.ends_with("decoder.int8.onnx")));
    assert!(!plan.args.iter().any(|arg| arg == "--joiner"));
    assert!(!plan.args.iter().any(|arg| arg == "dry-run"));
}

#[test]
fn packaged_local_asr_daemon_launch_plan_returns_none_when_daemon_is_missing() {
    let temp_dir = unique_temp_dir("talk-desktop-local-asr-missing");
    let release_dir = temp_dir.join("release");
    fs::create_dir_all(&release_dir).expect("create release dir");
    let executable_path = release_dir.join("talk-desktop.exe");

    let plan =
        desktop_packaged_local_asr_daemon_launch_plan(&executable_path, "ws://127.0.0.1:53171/asr")
            .expect("missing daemon should not be an endpoint error");

    assert_eq!(plan, None);
}

#[test]
fn packaged_local_asr_daemon_launch_plan_normalizes_localhost_to_ipv4_loopback() {
    let temp_dir = unique_temp_dir("talk-desktop-local-asr-localhost");
    let release_dir = temp_dir.join("release");
    let internal_dir = release_dir.join(".internal");
    fs::create_dir_all(&internal_dir).expect("create internal dir");
    fs::write(internal_dir.join("talk-local-asr-sherpa.exe"), b"fake exe")
        .expect("write daemon marker");
    let executable_path = release_dir.join("talk-desktop.exe");

    let plan =
        desktop_packaged_local_asr_daemon_launch_plan(&executable_path, "ws://localhost:53172/asr")
            .expect("valid launch plan")
            .expect("packaged daemon should be found");

    assert_eq!(plan.bind, "127.0.0.1:53172");
    assert_eq!(plan.args, vec!["--bind", "127.0.0.1:53172"]);
}

#[test]
fn packaged_local_asr_daemon_launch_plan_uses_bracketed_ipv6_loopback_bind() {
    let temp_dir = unique_temp_dir("talk-desktop-local-asr-ipv6");
    let release_dir = temp_dir.join("release");
    let internal_dir = release_dir.join(".internal");
    fs::create_dir_all(&internal_dir).expect("create internal dir");
    fs::write(internal_dir.join("talk-local-asr-sherpa.exe"), b"fake exe")
        .expect("write daemon marker");
    let executable_path = release_dir.join("talk-desktop.exe");

    let plan =
        desktop_packaged_local_asr_daemon_launch_plan(&executable_path, "ws://[::1]:53173/asr")
            .expect("valid launch plan")
            .expect("packaged daemon should be found");

    assert_eq!(plan.bind, "[::1]:53173");
    assert_eq!(plan.args, vec!["--bind", "[::1]:53173"]);
}

#[test]
fn default_desktop_config_path_prefers_release_sibling_template_when_present() {
    let temp_dir = std::env::temp_dir().join(format!(
        "talk-desktop-default-config-release-{}",
        std::process::id()
    ));
    let release_dir = temp_dir.join("release");
    fs::create_dir_all(&release_dir).expect("create release dir");
    let release_config_path = release_dir.join("talk.toml");
    fs::write(&release_config_path, b"voice_mode = \"command\"\n").expect("write release config");
    let executable_path = release_dir.join("Talk.exe");

    let resolved = resolve_default_desktop_config_path(None, &temp_dir, &executable_path);

    assert_eq!(resolved, release_config_path);
}

#[test]
fn default_desktop_config_path_falls_back_to_repo_example_for_dev_runs() {
    let temp_dir = std::env::temp_dir().join(format!(
        "talk-desktop-default-config-dev-{}",
        std::process::id()
    ));
    let examples_dir = temp_dir.join("examples");
    fs::create_dir_all(&examples_dir).expect("create examples dir");
    let dev_config_path = examples_dir.join("dev-config.toml");
    fs::write(&dev_config_path, b"voice_mode = \"command\"\n").expect("write dev config");
    let executable_dir = temp_dir.join("target").join("debug");
    fs::create_dir_all(&executable_dir).expect("create executable dir");
    let executable_path = executable_dir.join("talk-desktop.exe");

    let resolved = resolve_default_desktop_config_path(None, &temp_dir, &executable_path);

    assert_eq!(resolved, dev_config_path);
}

#[test]
fn desktop_insert_target_diagnostic_path_reuses_session_log_stem() {
    let session_log_path = PathBuf::from(r"C:\Talk\logs\123e4567-e89b-12d3-a456-426614174000.json");

    let diagnostic_path = desktop_insert_target_diagnostic_path(&session_log_path);

    assert_eq!(
        diagnostic_path,
        PathBuf::from(
            r"C:\Talk\logs\123e4567-e89b-12d3-a456-426614174000.desktop-insert-target.json"
        )
    );
}

#[test]
fn desktop_insert_target_diagnostic_captures_window_focus_and_restore_attempt_details() {
    let target = ForegroundInsertTarget {
        window_handle: 0x303,
        focus_handle: Some(0x404),
        primary_focus_handle: Some(0x404),
        fallback_focus_handle: Some(0x505),
        focus_capture_source: Some(ForegroundFocusCaptureSource::GuiThreadInfo),
    };
    let restore = DesktopInsertTargetRestoreDiagnostic {
        attempted: true,
        target_window_exists: Some(true),
        target_focus_exists: Some(false),
        focus_restore_requested: true,
        post_insert_release_reason: Some(ForegroundTargetReleaseReason::TargetStable),
        post_insert_wait_duration_ms: Some(210),
        post_insert_poll_count: Some(7),
        post_insert_target_foreground_poll_count: Some(5),
        post_insert_trailing_target_foreground_poll_count: Some(4),
        post_insert_required_stable_foreground_polls: Some(4),
    };
    let context = DesktopInsertTargetContext {
        target: Some(target),
        focus_class_name: Some("Chrome_RenderWidgetHostHWND".to_string()),
        caret_window_handle: None,
        automation_control_type: Some("document".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![7, 8, 9]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: false,
    };

    let diagnostic = build_desktop_insert_target_diagnostic(
        target,
        Some(&context),
        Some(DesktopOutputStrategy::HonorConfiguredOutput),
        Some(restore),
    );

    assert_eq!(diagnostic.captured_window_handle, "0x303");
    assert_eq!(diagnostic.captured_focus_handle.as_deref(), Some("0x404"));
    assert_eq!(
        diagnostic.captured_primary_focus_handle.as_deref(),
        Some("0x404")
    );
    assert_eq!(
        diagnostic.captured_fallback_focus_handle.as_deref(),
        Some("0x505")
    );
    assert_eq!(
        diagnostic.captured_focus_source.as_deref(),
        Some("gui_thread_info")
    );
    assert!(diagnostic.restore_attempted);
    assert_eq!(diagnostic.restore_target_window_exists, Some(true));
    assert_eq!(diagnostic.restore_target_focus_exists, Some(false));
    assert!(diagnostic.restore_focus_requested);
    assert_eq!(
        diagnostic.post_insert_release_reason.as_deref(),
        Some("target_stable")
    );
    assert_eq!(diagnostic.post_insert_wait_duration_ms, Some(210));
    assert_eq!(diagnostic.post_insert_poll_count, Some(7));
    assert_eq!(diagnostic.post_insert_target_foreground_poll_count, Some(5));
    assert_eq!(
        diagnostic.post_insert_trailing_target_foreground_poll_count,
        Some(4)
    );
    assert_eq!(
        diagnostic.post_insert_required_stable_foreground_polls,
        Some(4)
    );
    assert_eq!(
        diagnostic.output_strategy.as_deref(),
        Some("honor_configured_output")
    );
    assert_eq!(
        diagnostic.focus_class_name.as_deref(),
        Some("Chrome_RenderWidgetHostHWND")
    );
    assert_eq!(diagnostic.caret_window_handle, None);
    assert_eq!(
        diagnostic.automation_control_type.as_deref(),
        Some("document")
    );
    assert_eq!(
        diagnostic.automation_framework_id.as_deref(),
        Some("Chrome")
    );
    assert_eq!(diagnostic.automation_runtime_id, Some(vec![7, 8, 9]));
    assert_eq!(diagnostic.automation_is_keyboard_focusable, Some(true));
    assert!(diagnostic.automation_supports_text_pattern);
    assert!(!diagnostic.automation_supports_value_pattern);
    assert_eq!(diagnostic.focus_looks_editable, Some(true));
}

#[test]
fn desktop_insert_target_diagnostic_trace_captures_origin_and_current_browser_identity() {
    let origin_context = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0xAC1168,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("edit".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 7999618, 4, 993, 8, 66]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: true,
        automation_supports_value_pattern: true,
    };
    let current_context = origin_context.clone();
    let release_time_context = DesktopInsertTargetContext {
        target: Some(ForegroundInsertTarget {
            window_handle: 0xAC1168,
            focus_handle: None,
            primary_focus_handle: None,
            fallback_focus_handle: None,
            focus_capture_source: None,
        }),
        focus_class_name: None,
        caret_window_handle: None,
        automation_control_type: Some("menu_bar".to_string()),
        automation_framework_id: Some("Chrome".to_string()),
        automation_runtime_id: Some(vec![42, 7999618, 4, 1000]),
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };
    let trace = build_desktop_insert_target_trace_diagnostic(
        Some("hotkey_pending_pretrigger"),
        Some(&origin_context),
        Some(&current_context),
        Some(&origin_context),
        Some(&release_time_context),
    )
    .expect("trace should be present");

    let diagnostic = build_desktop_insert_target_diagnostic_with_trace(
        current_context.target.expect("current target"),
        Some(&current_context),
        Some(DesktopOutputStrategy::HonorConfiguredOutput),
        None,
        Some(trace),
    );

    let trace = diagnostic.trace.expect("trace");
    assert_eq!(
        trace.selected_origin_source.as_deref(),
        Some("hotkey_pending_pretrigger")
    );
    assert_eq!(
        trace
            .origin_target
            .as_ref()
            .and_then(|target| target.automation_runtime_id.clone()),
        Some(vec![42, 7999618, 4, 993, 8, 66])
    );
    assert_eq!(
        trace
            .release_time_origin_target
            .as_ref()
            .and_then(|target| target.automation_control_type.as_deref()),
        Some("menu_bar")
    );
    assert_eq!(trace.same_window_as_origin, Some(true));
    assert_eq!(trace.same_control_by_handle, Some(false));
    assert_eq!(trace.same_control_by_runtime_id, Some(true));
}

#[test]
fn desktop_insert_target_diagnostic_marks_copy_popup_fallback_context_as_not_editable() {
    let target = ForegroundInsertTarget {
        window_handle: 0x606,
        focus_handle: Some(0x707),
        primary_focus_handle: Some(0x707),
        fallback_focus_handle: Some(0x707),
        focus_capture_source: Some(ForegroundFocusCaptureSource::AttachedGetFocus),
    };
    let context = DesktopInsertTargetContext {
        target: Some(target),
        focus_class_name: Some("Button".to_string()),
        caret_window_handle: None,
        automation_control_type: Some("button".to_string()),
        automation_framework_id: Some("Win32".to_string()),
        automation_runtime_id: None,
        automation_is_keyboard_focusable: Some(true),
        automation_supports_text_pattern: false,
        automation_supports_value_pattern: false,
    };

    let diagnostic = build_desktop_insert_target_diagnostic(
        target,
        Some(&context),
        Some(DesktopOutputStrategy::ShowCopyPopupOnly),
        None,
    );

    assert_eq!(
        diagnostic.output_strategy.as_deref(),
        Some("show_copy_popup_only")
    );
    assert_eq!(diagnostic.focus_looks_editable, Some(false));
    assert_eq!(
        diagnostic.captured_focus_source.as_deref(),
        Some("attached_get_focus")
    );
    assert_eq!(
        diagnostic.automation_control_type.as_deref(),
        Some("button")
    );
    assert_eq!(diagnostic.automation_framework_id.as_deref(), Some("Win32"));
    assert_eq!(diagnostic.automation_is_keyboard_focusable, Some(true));
    assert!(!diagnostic.automation_supports_text_pattern);
    assert!(!diagnostic.automation_supports_value_pattern);
}
