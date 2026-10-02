mod aliases;

use super::{
    apply_worker_transition, choose_worker_transition, OwnedWorkerState, WorkerTransition,
};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use talk_core::{
    OutputMode, SpeculativeLocalAsrDaemonConfig, SpeculativeLocalAsrDaemonMode, TalkConfig,
};
use talk_desktop::{
    desktop_product_local_asr_daemon_launch_plan_with_config, tray_menu_model, ConfigAvailability,
    DesktopLocalAsrDaemonLaunchPlan, HotkeyBindingState, ShellState,
};

struct Fixture {
    root: PathBuf,
    worker: PathBuf,
    config: TalkConfig,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("talk-asr-reload-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let worker = root.join("talk-local-asr-sherpa.exe");
        fs::write(&worker, b"launch-plan fixture, never executed").unwrap();
        let mut config = TalkConfig::from_toml_str(include_str!(
            "../../../../examples/desktop-streaming-service-speculative-config.toml"
        ))
        .unwrap();
        config
            .speculative
            .streaming_service
            .as_mut()
            .unwrap()
            .local_daemon = Some(SpeculativeLocalAsrDaemonConfig {
            mode: SpeculativeLocalAsrDaemonMode::SherpaOnline,
            model: Some("zipformer-zh-en-punct-int8-480ms".into()),
            tokens: Some(root.join("tokens.txt")),
            encoder: Some(root.join("encoder.int8.onnx")),
            decoder: Some(root.join("decoder.onnx")),
            joiner: Some(root.join("joiner.int8.onnx")),
            provider: Some("cpu".into()),
            num_threads: Some(2),
            sample_rate_hz: Some(16_000),
            decoding_method: Some("greedy_search".into()),
            ..SpeculativeLocalAsrDaemonConfig::default()
        });
        Self {
            root,
            worker,
            config,
        }
    }

    fn plan(&self, config: &TalkConfig) -> Option<DesktopLocalAsrDaemonLaunchPlan> {
        config.validate().unwrap();
        let service = config.speculative.streaming_service.as_ref().unwrap();
        desktop_product_local_asr_daemon_launch_plan_with_config(
            &self.worker,
            &self.root,
            &service.endpoint,
            service.local_daemon.as_ref(),
        )
        .unwrap()
    }

    fn matches(&self, current: &DesktopLocalAsrDaemonLaunchPlan, config: &TalkConfig) -> bool {
        let owned = OwnedWorkerState {
            plan: current,
            running: true,
        };
        let requested_endpoint = &config
            .speculative
            .streaming_service
            .as_ref()
            .unwrap()
            .endpoint;
        choose_worker_transition(
            Some(&owned),
            requested_endpoint,
            || false,
            || Ok::<_, &str>(self.plan(config)),
        )
        .unwrap()
            == WorkerTransition::ReuseOwned
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn managed_worker_reload_applies_engine_options_at_the_same_endpoint() {
    let fixture = Fixture::new();
    let current = fixture.plan(&fixture.config).unwrap();
    for option in ["threads", "decoder", "model", "encoder", "hotwords"] {
        let mut changed = fixture.config.clone();
        let daemon = changed
            .speculative
            .streaming_service
            .as_mut()
            .unwrap()
            .local_daemon
            .as_mut()
            .unwrap();
        match option {
            "threads" => daemon.num_threads = Some(1),
            "decoder" => daemon.decoding_method = Some("modified_beam_search".into()),
            "model" => daemon.model = Some("another-model".into()),
            "encoder" => daemon.encoder = Some(fixture.root.join("another-encoder.onnx")),
            "hotwords" => daemon.hotwords_file = Some(fixture.root.join("hotwords.txt")),
            _ => unreachable!(),
        }
        assert!(
            !fixture.matches(&current, &changed),
            "stale worker reused after {option} changed"
        );
    }
}

#[test]
fn managed_worker_reload_keeps_warm_for_unrelated_settings() {
    let fixture = Fixture::new();
    let current = fixture.plan(&fixture.config).unwrap();
    let mut changed = fixture.config.clone();
    changed.output.mode = OutputMode::DryRun;
    changed.output.restore_clipboard = false;
    changed.logging.dir = fixture.root.join("different-logs");
    let service = changed.speculative.streaming_service.as_mut().unwrap();
    service.connect_timeout_ms += 1_000;
    service.pump_timeout_ms += 100;
    service.final_timeout_ms += 1_000;
    assert!(fixture.matches(&current, &changed));
    assert!(fixture.matches(&current, &fixture.config));
}

#[test]
fn managed_worker_reload_rejects_changed_endpoint_or_executable() {
    let mut fixture = Fixture::new();
    let current = fixture.plan(&fixture.config).unwrap();
    let mut changed = fixture.config.clone();
    changed
        .speculative
        .streaming_service
        .as_mut()
        .unwrap()
        .endpoint = "ws://127.0.0.1:53172/asr".into();
    assert!(!fixture.matches(&current, &changed));

    fixture.worker = fixture.root.join("replacement-worker.exe");
    fs::write(&fixture.worker, b"replacement launch-plan fixture").unwrap();
    assert!(!fixture.matches(&current, &fixture.config));
}

#[test]
fn managed_worker_reload_does_not_reuse_an_unavailable_plan() {
    let fixture = Fixture::new();
    let current = fixture.plan(&fixture.config).unwrap();
    fs::remove_file(&fixture.worker).unwrap();
    assert!(fixture.plan(&fixture.config).is_none());
    assert!(!fixture.matches(&current, &fixture.config));
}

#[test]
fn reload_config_waits_until_recording_and_processing_finish() {
    let recording = ShellState::idle().begin_recording().unwrap();
    for active in [recording, recording.set_busy()] {
        let menu = tray_menu_model(
            &active,
            &ConfigAvailability::ready(),
            &HotkeyBindingState::Unconfigured,
            None,
        );
        assert!(!menu.reload_config_enabled);
        assert!(menu.open_config_enabled);
        let completed = tray_menu_model(
            &active.complete(),
            &ConfigAvailability::ready(),
            &HotkeyBindingState::Unconfigured,
            None,
        );
        assert!(completed.reload_config_enabled);
    }
    let recovery = tray_menu_model(
        &ShellState::idle(),
        &ConfigAvailability::unavailable("invalid TOML"),
        &HotkeyBindingState::Unconfigured,
        None,
    );
    assert!(recovery.reload_config_enabled);
}

#[test]
fn external_takeovers_never_resolve_packaged_files_or_launch_a_competitor() {
    let fixture = Fixture::new();
    let target = "ws://127.0.0.1:53171/asr";
    for (name, endpoint, running) in [
        ("unowned", None, false),
        ("dead owned at same endpoint", Some(target), false),
        (
            "live owned at other endpoint",
            Some("ws://127.0.0.1:53172/asr"),
            true,
        ),
    ] {
        let events = RefCell::new(Vec::new());
        let mut owned_config = fixture.config.clone();
        if let Some(endpoint) = endpoint {
            owned_config
                .speculative
                .streaming_service
                .as_mut()
                .unwrap()
                .endpoint = endpoint.into();
        }
        let owned_plan = fixture.plan(&owned_config).unwrap();
        let state = endpoint.map(|_| OwnedWorkerState {
            plan: &owned_plan,
            running,
        });
        let mut owned = endpoint.map(|_| "owned process");
        let transition = choose_worker_transition(
            state.as_ref(),
            target,
            || {
                events.borrow_mut().push("probe");
                true
            },
            || -> Result<_, &str> {
                panic!("{name}: packaged config is unavailable but irrelevant")
            },
        )
        .unwrap();
        assert_eq!(transition, WorkerTransition::ConnectExternal);
        assert!(apply_worker_transition(
            transition,
            &mut owned,
            |worker| {
                assert_eq!(*worker, "owned process");
                events.borrow_mut().push("stop owned");
                Ok::<_, &str>(())
            },
            |_| panic!("{name}: must not launch against the external listener"),
        )
        .unwrap());
        assert!(
            owned.is_none(),
            "the external process must never become owned"
        );
        assert_eq!(
            *events.borrow(),
            if endpoint.is_some() {
                vec!["probe", "stop owned"]
            } else {
                vec!["probe"]
            }
        );
    }
}

#[test]
fn live_worker_replacement_waits_for_stop_before_launch() {
    let fixture = Fixture::new();
    let plan = fixture.plan(&fixture.config).unwrap();
    let mut changed = fixture.config.clone();
    changed
        .speculative
        .streaming_service
        .as_mut()
        .unwrap()
        .local_daemon
        .as_mut()
        .unwrap()
        .num_threads = Some(1);
    let replacement = fixture.plan(&changed).unwrap();
    let target = "ws://127.0.0.1:53171/asr";
    let state = OwnedWorkerState {
        plan: &plan,
        running: true,
    };
    let events = RefCell::new(Vec::new());
    let transition = choose_worker_transition(
        Some(&state),
        target,
        || panic!("the live owned listener is not an external worker"),
        || {
            events.borrow_mut().push("resolve");
            Ok::<_, &str>(Some(replacement.clone()))
        },
    )
    .unwrap();
    let mut owned = Some("old");
    assert!(apply_worker_transition(
        transition,
        &mut owned,
        |worker| {
            assert_eq!(*worker, "old");
            events.borrow_mut().extend(["kill", "wait"]);
            Ok::<_, &str>(())
        },
        |actual| {
            assert_eq!(actual, replacement);
            events.borrow_mut().push("launch");
            Ok("new")
        },
    )
    .unwrap());
    assert_eq!(owned, Some("new"));
    assert_eq!(*events.borrow(), vec!["resolve", "kill", "wait", "launch"]);
}

#[test]
fn unchanged_live_worker_stays_warm_without_probing_or_stopping() {
    let fixture = Fixture::new();
    let plan = fixture.plan(&fixture.config).unwrap();
    let target = "ws://127.0.0.1:53171/asr";
    let state = OwnedWorkerState {
        plan: &plan,
        running: true,
    };
    let transition = choose_worker_transition(
        Some(&state),
        target,
        || panic!("do not probe our live listener"),
        || Ok::<_, &str>(Some(plan.clone())),
    )
    .unwrap();
    let mut owned = Some(123);
    assert!(apply_worker_transition(
        transition,
        &mut owned,
        |_| panic!("do not stop unchanged worker"),
        |_| panic!("do not restart unchanged worker"),
    )
    .unwrap_or_else(|_: &str| unreachable!()));
    assert_eq!(owned, Some(123));
}

#[test]
fn failed_stop_retains_ownership_and_prevents_launch_or_external_use() {
    let fixture = Fixture::new();
    let plan = fixture.plan(&fixture.config).unwrap();
    for transition in [
        WorkerTransition::ConnectExternal,
        WorkerTransition::ReplaceOwned(Some(plan)),
    ] {
        for failure in ["kill failed", "wait failed"] {
            let mut owned = Some(123);
            let result = apply_worker_transition(
                transition.clone(),
                &mut owned,
                |_| Err(failure),
                |_| panic!("must not launch until owned process has stopped"),
            );
            assert_eq!(result, Err(failure));
            assert_eq!(owned, Some(123));
        }
    }
}

#[test]
fn dead_owned_worker_restarts_when_no_external_listener_exists() {
    let fixture = Fixture::new();
    let plan = fixture.plan(&fixture.config).unwrap();
    let target = "ws://127.0.0.1:53171/asr";
    let state = OwnedWorkerState {
        plan: &plan,
        running: false,
    };
    let events = RefCell::new(Vec::new());
    let transition = choose_worker_transition(
        Some(&state),
        target,
        || {
            events.borrow_mut().push("probe");
            false
        },
        || {
            events.borrow_mut().push("resolve");
            Ok::<_, &str>(Some(plan.clone()))
        },
    )
    .unwrap();
    let mut owned = Some("dead");
    assert!(apply_worker_transition(
        transition,
        &mut owned,
        |_| {
            events.borrow_mut().push("reap");
            Ok::<_, &str>(())
        },
        |_| {
            events.borrow_mut().push("launch");
            Ok("new")
        },
    )
    .unwrap());
    assert_eq!(*events.borrow(), vec!["probe", "resolve", "reap", "launch"]);
    assert_eq!(owned, Some("new"));
}

#[test]
fn resolution_failure_does_not_return_a_destructive_transition() {
    let fixture = Fixture::new();
    let plan = fixture.plan(&fixture.config).unwrap();
    let target = "ws://127.0.0.1:53171/asr";
    let state = OwnedWorkerState {
        plan: &plan,
        running: true,
    };
    assert_eq!(
        choose_worker_transition(
            Some(&state),
            target,
            || panic!("do not mistake the owned listener for an external process"),
            || Err("invalid launch configuration"),
        ),
        Err("invalid launch configuration")
    );
}

#[test]
fn unavailable_plan_and_launch_failure_do_not_promote_a_worker() {
    let fixture = Fixture::new();
    let plan = fixture.plan(&fixture.config).unwrap();
    let mut owned = Some(123);
    assert!(!apply_worker_transition(
        WorkerTransition::ReplaceOwned(None),
        &mut owned,
        |_| Ok::<_, &str>(()),
        |_| panic!("no worker plan"),
    )
    .unwrap());
    assert!(owned.is_none());
    let result = apply_worker_transition(
        WorkerTransition::ReplaceOwned(Some(plan)),
        &mut owned,
        |_| panic!("there is no owned worker"),
        |_| Err("launch failed"),
    );
    assert_eq!(result, Err("launch failed"));
    assert!(owned.is_none());
}
