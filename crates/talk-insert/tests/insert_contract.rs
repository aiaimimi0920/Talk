use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use talk_core::TalkError;
use talk_insert::{
    probe_native_windows_clipboard_readiness, AroundPasteShortcut, AroundTextInserter,
    BeforePasteShortcut, ClipboardBackend, ClipboardFallbackInserter, ClipboardPasteInserter,
    ClipboardRestorePolicy, DryRunInserter, InsertMethod, InsertOutcome, NativeReadinessStatus,
    PasteShortcut, TextInserter, WindowsPasteShortcut, WindowsPasteShortcutMode,
};

static NATIVE_CLIPBOARD_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn dry_run_inserter_reports_success_without_touching_clipboard() {
    let inserter = DryRunInserter::default();
    let outcome = inserter.insert_text("hello neuro").expect("dry run insert");

    assert_eq!(
        outcome,
        InsertOutcome::Inserted {
            method: InsertMethod::DryRun
        }
    );
    assert_eq!(inserter.last_text().as_deref(), Some("hello neuro"));
}

#[test]
fn dry_run_inserter_refuses_empty_text_like_real_insert_backends() {
    let inserter = DryRunInserter::default();

    let error = inserter
        .insert_text("")
        .expect_err("empty text must be rejected");

    assert_eq!(
        error,
        TalkError::Insert("refusing to insert empty text".to_string())
    );
    assert_eq!(inserter.last_text(), None);
}

#[test]
fn dry_run_inserter_refuses_blank_text_like_real_insert_backends() {
    let inserter = DryRunInserter::default();

    let error = inserter
        .insert_text(" \t\r\n")
        .expect_err("blank text must be rejected");

    assert_eq!(
        error,
        TalkError::Insert("refusing to insert empty text".to_string())
    );
    assert_eq!(inserter.last_text(), None);
}

#[test]
fn windows_paste_shortcut_mode_defaults_to_ctrl_v_when_env_is_unset() {
    assert_eq!(
        talk_insert::resolve_windows_paste_shortcut_mode_from_env_value(None),
        WindowsPasteShortcutMode::ControlV
    );
}

#[test]
fn windows_paste_shortcut_mode_accepts_ctrl_shift_v_env_override() {
    assert_eq!(
        talk_insert::resolve_windows_paste_shortcut_mode_from_env_value(Some("ctrl_shift_v")),
        WindowsPasteShortcutMode::ControlShiftV
    );
}

#[test]
fn windows_paste_shortcut_mode_accepts_explicit_ctrl_v_env_override() {
    assert_eq!(
        talk_insert::resolve_windows_paste_shortcut_mode_from_env_value(Some("ctrl_v")),
        WindowsPasteShortcutMode::ControlV
    );
}

#[test]
fn windows_paste_shortcut_mode_accepts_shift_insert_env_override() {
    assert_eq!(
        talk_insert::resolve_windows_paste_shortcut_mode_from_env_value(Some("shift_insert")),
        WindowsPasteShortcutMode::ShiftInsert
    );
}

#[test]
fn windows_paste_shortcut_mode_falls_back_to_ctrl_v_for_unknown_env_values() {
    assert_eq!(
        talk_insert::resolve_windows_paste_shortcut_mode_from_env_value(Some("custom")),
        WindowsPasteShortcutMode::ControlV
    );
}

#[test]
fn clipboard_paste_inserter_writes_text_sends_paste_and_restores_original_clipboard() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = RecordingPasteShortcut::new(calls.clone());
    let inserter = ClipboardPasteInserter::new(
        clipboard.clone(),
        paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );

    let outcome = inserter
        .insert_text("hello clipboard")
        .expect("clipboard paste insert");

    assert_eq!(
        outcome,
        InsertOutcome::Inserted {
            method: InsertMethod::ClipboardPaste
        }
    );
    assert_eq!(
        clipboard.current_text(),
        Some("before clipboard".to_string())
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "capture",
            "write:hello clipboard",
            "paste_shortcut",
            "restore:before clipboard"
        ]
    );
}

#[test]
fn clipboard_paste_inserter_can_leave_inserted_text_when_restore_is_disabled() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = RecordingPasteShortcut::new(calls.clone());
    let inserter = ClipboardPasteInserter::new(
        clipboard.clone(),
        paste,
        ClipboardRestorePolicy::LeaveInsertedText,
    );

    let outcome = inserter
        .insert_text("hello clipboard")
        .expect("clipboard paste insert");

    assert_eq!(
        outcome,
        InsertOutcome::Inserted {
            method: InsertMethod::ClipboardPaste
        }
    );
    assert_eq!(
        clipboard.current_text(),
        Some("hello clipboard".to_string())
    );
    assert_eq!(
        recorded_calls(&calls),
        vec!["write:hello clipboard", "paste_shortcut"]
    );
}

#[test]
fn clipboard_paste_inserter_refuses_empty_text_before_clipboard_or_keyboard_side_effects() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = RecordingPasteShortcut::new(calls.clone());
    let inserter =
        ClipboardPasteInserter::new(clipboard, paste, ClipboardRestorePolicy::RestoreOriginal);

    let error = inserter
        .insert_text("")
        .expect_err("empty text must be rejected");

    assert_eq!(
        error,
        TalkError::Insert("refusing to insert empty text".to_string())
    );
    assert!(recorded_calls(&calls).is_empty());
}

#[test]
fn clipboard_paste_inserter_refuses_blank_text_before_clipboard_or_keyboard_side_effects() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = RecordingPasteShortcut::new(calls.clone());
    let inserter =
        ClipboardPasteInserter::new(clipboard, paste, ClipboardRestorePolicy::RestoreOriginal);

    let error = inserter
        .insert_text(" \t\r\n")
        .expect_err("blank text must be rejected");

    assert_eq!(
        error,
        TalkError::Insert("refusing to insert empty text".to_string())
    );
    assert!(recorded_calls(&calls).is_empty());
}

#[test]
fn clipboard_fallback_inserter_refuses_blank_text() {
    let inserter = ClipboardFallbackInserter;

    let error = inserter
        .insert_text(" \t\r\n")
        .expect_err("blank text must be rejected");

    assert_eq!(
        error,
        TalkError::Insert("refusing to insert empty text".to_string())
    );
}

#[test]
fn clipboard_fallback_inserter_reports_current_talk_reason_without_legacy_mvp_wording() {
    let inserter = ClipboardFallbackInserter;

    let outcome = inserter
        .insert_text("hello fallback")
        .expect("fallback insert");
    let InsertOutcome::FallbackClipboard { reason } = outcome else {
        panic!("expected fallback clipboard outcome");
    };

    assert!(
        reason.contains("native clipboard paste is not enabled"),
        "reason={reason}"
    );
    assert!(
        !reason.contains("MVP") && !reason.contains("Hook") && !reason.contains("HookLess"),
        "fallback reason must use current Talk product wording, reason={reason}"
    );
}

#[test]
fn clipboard_paste_inserter_restores_original_clipboard_when_paste_shortcut_fails() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = FailingPasteShortcut::new(calls.clone());
    let inserter = ClipboardPasteInserter::new(
        clipboard.clone(),
        paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );

    let error = inserter
        .insert_text("hello clipboard")
        .expect_err("paste shortcut failure must be surfaced");

    assert_eq!(
        error,
        TalkError::Insert("paste shortcut failed".to_string())
    );
    assert_eq!(
        clipboard.current_text(),
        Some("before clipboard".to_string())
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "capture",
            "write:hello clipboard",
            "paste_shortcut_failed",
            "restore:before clipboard"
        ]
    );
}

#[test]
fn clipboard_paste_inserter_keeps_inserted_text_available_until_async_paste_consumer_reads_it() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let (paste_called_tx, paste_called_rx) = mpsc::channel();
    let (release_paste_tx, release_paste_rx) = mpsc::channel();
    let (observed_text_tx, observed_text_rx) = mpsc::channel();
    let paste = AsyncClipboardReaderPasteShortcut::new(
        calls.clone(),
        clipboard.clone(),
        paste_called_tx,
        release_paste_rx,
        observed_text_tx,
    );
    let inserter = ClipboardPasteInserter::new(
        clipboard.clone(),
        paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );

    let worker = thread::spawn(move || inserter.insert_text("hello clipboard"));

    paste_called_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("paste shortcut should be triggered");
    release_paste_tx
        .send(())
        .expect("release async clipboard reader");

    let outcome = worker
        .join()
        .expect("clipboard paste worker thread must join")
        .expect("clipboard paste insert");
    let observed_text = observed_text_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("async paste observer should capture clipboard text");

    assert_eq!(
        outcome,
        InsertOutcome::Inserted {
            method: InsertMethod::ClipboardPaste
        }
    );
    assert_eq!(observed_text.as_deref(), Some("hello clipboard"));
    assert_eq!(
        clipboard.current_text(),
        Some("before clipboard".to_string())
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "capture",
            "write:hello clipboard",
            "paste_shortcut",
            "async_paste_observed:hello clipboard",
            "restore:before clipboard"
        ]
    );
}

#[test]
fn clipboard_paste_inserter_keeps_inserted_text_available_for_delayed_async_paste_consumers() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let (observed_text_tx, observed_text_rx) = mpsc::channel();
    let paste = DelayedAsyncClipboardReaderPasteShortcut::new(
        calls.clone(),
        clipboard.clone(),
        Duration::from_millis(250),
        observed_text_tx,
    );
    let inserter = ClipboardPasteInserter::new(
        clipboard.clone(),
        paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );

    let outcome = inserter
        .insert_text("hello clipboard")
        .expect("clipboard paste insert");
    let observed_text = observed_text_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("delayed async paste observer should capture clipboard text");

    assert_eq!(
        outcome,
        InsertOutcome::Inserted {
            method: InsertMethod::ClipboardPaste
        }
    );
    assert_eq!(observed_text.as_deref(), Some("hello clipboard"));
    assert_eq!(
        clipboard.current_text(),
        Some("before clipboard".to_string())
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "capture",
            "write:hello clipboard",
            "paste_shortcut",
            "delayed_async_paste_observed:hello clipboard",
            "restore:before clipboard"
        ]
    );
}

#[test]
fn before_paste_shortcut_runs_hook_after_clipboard_write_and_before_paste_shortcut() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = RecordingPasteShortcut::new(calls.clone());
    let before_paste_calls = Arc::clone(&calls);
    let wrapped_paste = BeforePasteShortcut::new(paste, move || {
        before_paste_calls
            .lock()
            .expect("calls mutex poisoned")
            .push("before_paste".to_string());
    });
    let inserter = ClipboardPasteInserter::new(
        clipboard.clone(),
        wrapped_paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );

    let outcome = inserter
        .insert_text("hello clipboard")
        .expect("clipboard paste insert");

    assert_eq!(
        outcome,
        InsertOutcome::Inserted {
            method: InsertMethod::ClipboardPaste
        }
    );
    assert_eq!(
        clipboard.current_text(),
        Some("before clipboard".to_string())
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "capture",
            "write:hello clipboard",
            "before_paste",
            "paste_shortcut",
            "restore:before clipboard"
        ]
    );
}

#[test]
fn around_paste_shortcut_runs_before_and_after_hooks_around_paste_shortcut() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = RecordingPasteShortcut::new(calls.clone());
    let before_calls = Arc::clone(&calls);
    let after_calls = Arc::clone(&calls);
    let wrapped_paste = AroundPasteShortcut::new(
        paste,
        move || {
            before_calls
                .lock()
                .expect("calls mutex poisoned")
                .push("before_paste".to_string());
        },
        move || {
            after_calls
                .lock()
                .expect("calls mutex poisoned")
                .push("after_paste".to_string());
        },
    );
    let inserter = ClipboardPasteInserter::new(
        clipboard.clone(),
        wrapped_paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );

    let outcome = inserter
        .insert_text("hello clipboard")
        .expect("clipboard paste insert");

    assert_eq!(
        outcome,
        InsertOutcome::Inserted {
            method: InsertMethod::ClipboardPaste
        }
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "capture",
            "write:hello clipboard",
            "before_paste",
            "paste_shortcut",
            "after_paste",
            "restore:before clipboard"
        ]
    );
}

#[test]
fn around_paste_shortcut_runs_after_hook_even_when_paste_shortcut_fails() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = FailingPasteShortcut::new(calls.clone());
    let before_calls = Arc::clone(&calls);
    let after_calls = Arc::clone(&calls);
    let wrapped_paste = AroundPasteShortcut::new(
        paste,
        move || {
            before_calls
                .lock()
                .expect("calls mutex poisoned")
                .push("before_paste".to_string());
        },
        move || {
            after_calls
                .lock()
                .expect("calls mutex poisoned")
                .push("after_paste".to_string());
        },
    );
    let inserter = ClipboardPasteInserter::new(
        clipboard.clone(),
        wrapped_paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );

    let error = inserter
        .insert_text("hello clipboard")
        .expect_err("paste shortcut failure must be surfaced");

    assert_eq!(
        error,
        TalkError::Insert("paste shortcut failed".to_string())
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "capture",
            "write:hello clipboard",
            "before_paste",
            "paste_shortcut_failed",
            "after_paste",
            "restore:before clipboard"
        ]
    );
}

#[test]
fn around_text_inserter_runs_after_hook_after_clipboard_restore() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = RecordingPasteShortcut::new(calls.clone());
    let inner = ClipboardPasteInserter::new(
        clipboard.clone(),
        paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );
    let before_calls = Arc::clone(&calls);
    let after_calls = Arc::clone(&calls);
    let inserter = AroundTextInserter::new(
        inner,
        move || {
            before_calls
                .lock()
                .expect("calls mutex poisoned")
                .push("before_insert".to_string());
        },
        move || {
            after_calls
                .lock()
                .expect("calls mutex poisoned")
                .push("after_insert".to_string());
        },
    );

    let outcome = inserter
        .insert_text("hello clipboard")
        .expect("clipboard paste insert");

    assert_eq!(
        outcome,
        InsertOutcome::Inserted {
            method: InsertMethod::ClipboardPaste
        }
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "before_insert",
            "capture",
            "write:hello clipboard",
            "paste_shortcut",
            "restore:before clipboard",
            "after_insert"
        ]
    );
}

#[test]
fn around_text_inserter_runs_after_hook_even_when_clipboard_insert_fails() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let clipboard = RecordingClipboard::new(Some("before clipboard".to_string()), calls.clone());
    let paste = FailingPasteShortcut::new(calls.clone());
    let inner = ClipboardPasteInserter::new(
        clipboard.clone(),
        paste,
        ClipboardRestorePolicy::RestoreOriginal,
    );
    let before_calls = Arc::clone(&calls);
    let after_calls = Arc::clone(&calls);
    let inserter = AroundTextInserter::new(
        inner,
        move || {
            before_calls
                .lock()
                .expect("calls mutex poisoned")
                .push("before_insert".to_string());
        },
        move || {
            after_calls
                .lock()
                .expect("calls mutex poisoned")
                .push("after_insert".to_string());
        },
    );

    let error = inserter
        .insert_text("hello clipboard")
        .expect_err("paste shortcut failure must be surfaced");

    assert_eq!(
        error,
        TalkError::Insert("paste shortcut failed".to_string())
    );
    assert_eq!(
        recorded_calls(&calls),
        vec![
            "before_insert",
            "capture",
            "write:hello clipboard",
            "paste_shortcut_failed",
            "restore:before clipboard",
            "after_insert"
        ]
    );
}

#[test]
fn windows_paste_shortcut_can_be_disabled_before_native_keyboard_side_effects() {
    let _guard = NATIVE_CLIPBOARD_ENV_LOCK
        .lock()
        .expect("native clipboard env mutex");
    let previous = std::env::var_os("TALK_DISABLE_NATIVE_CLIPBOARD");
    std::env::set_var("TALK_DISABLE_NATIVE_CLIPBOARD", "1");

    let result = WindowsPasteShortcut.send_paste();

    match previous {
        Some(value) => std::env::set_var("TALK_DISABLE_NATIVE_CLIPBOARD", value),
        None => std::env::remove_var("TALK_DISABLE_NATIVE_CLIPBOARD"),
    }

    let error = result.expect_err("disabled native shortcut must fail");
    assert!(
        error.to_string().contains("TALK_DISABLE_NATIVE_CLIPBOARD"),
        "error={error}"
    );
}

#[test]
fn native_windows_clipboard_readiness_reports_disabled_env_before_side_effects() {
    let _guard = NATIVE_CLIPBOARD_ENV_LOCK
        .lock()
        .expect("native clipboard env mutex");
    let previous = std::env::var_os("TALK_DISABLE_NATIVE_CLIPBOARD");
    std::env::set_var("TALK_DISABLE_NATIVE_CLIPBOARD", "1");

    let readiness = probe_native_windows_clipboard_readiness();

    match previous {
        Some(value) => std::env::set_var("TALK_DISABLE_NATIVE_CLIPBOARD", value),
        None => std::env::remove_var("TALK_DISABLE_NATIVE_CLIPBOARD"),
    }

    assert_eq!(readiness.status, NativeReadinessStatus::Unavailable);
    assert_eq!(
        readiness.reason.as_deref(),
        Some("native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD")
    );
}

#[derive(Debug, Clone)]
struct RecordingClipboard {
    text: Arc<Mutex<Option<String>>>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl RecordingClipboard {
    fn new(text: Option<String>, calls: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            text: Arc::new(Mutex::new(text)),
            calls,
        }
    }

    fn current_text(&self) -> Option<String> {
        self.text
            .lock()
            .expect("recording clipboard mutex poisoned")
            .clone()
    }
}

impl ClipboardBackend for RecordingClipboard {
    type Snapshot = Option<String>;

    fn capture(&self) -> Result<Self::Snapshot, TalkError> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push("capture".to_string());
        Ok(self.current_text())
    }

    fn write_text(&self, text: &str) -> Result<(), TalkError> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push(format!("write:{text}"));
        *self.text.lock().expect("clipboard mutex poisoned") = Some(text.to_string());
        Ok(())
    }

    fn restore(&self, snapshot: Self::Snapshot) -> Result<(), TalkError> {
        let label = snapshot.as_deref().unwrap_or("<empty>");
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push(format!("restore:{label}"));
        *self.text.lock().expect("clipboard mutex poisoned") = snapshot;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct RecordingPasteShortcut {
    calls: Arc<Mutex<Vec<String>>>,
}

impl RecordingPasteShortcut {
    fn new(calls: Arc<Mutex<Vec<String>>>) -> Self {
        Self { calls }
    }
}

impl PasteShortcut for RecordingPasteShortcut {
    fn send_paste(&self) -> Result<(), TalkError> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push("paste_shortcut".to_string());
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct FailingPasteShortcut {
    calls: Arc<Mutex<Vec<String>>>,
}

impl FailingPasteShortcut {
    fn new(calls: Arc<Mutex<Vec<String>>>) -> Self {
        Self { calls }
    }
}

impl PasteShortcut for FailingPasteShortcut {
    fn send_paste(&self) -> Result<(), TalkError> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push("paste_shortcut_failed".to_string());
        Err(TalkError::Insert("paste shortcut failed".to_string()))
    }
}

#[derive(Debug)]
struct AsyncClipboardReaderPasteShortcut {
    calls: Arc<Mutex<Vec<String>>>,
    clipboard: RecordingClipboard,
    paste_called_tx: mpsc::Sender<()>,
    release_paste_rx: Mutex<mpsc::Receiver<()>>,
    observed_text_tx: Mutex<Option<mpsc::Sender<Option<String>>>>,
}

impl AsyncClipboardReaderPasteShortcut {
    fn new(
        calls: Arc<Mutex<Vec<String>>>,
        clipboard: RecordingClipboard,
        paste_called_tx: mpsc::Sender<()>,
        release_paste_rx: mpsc::Receiver<()>,
        observed_text_tx: mpsc::Sender<Option<String>>,
    ) -> Self {
        Self {
            calls,
            clipboard,
            paste_called_tx,
            release_paste_rx: Mutex::new(release_paste_rx),
            observed_text_tx: Mutex::new(Some(observed_text_tx)),
        }
    }
}

impl PasteShortcut for AsyncClipboardReaderPasteShortcut {
    fn send_paste(&self) -> Result<(), TalkError> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push("paste_shortcut".to_string());
        self.paste_called_tx
            .send(())
            .expect("signal paste shortcut invocation");
        self.release_paste_rx
            .lock()
            .expect("release receiver mutex poisoned")
            .recv_timeout(Duration::from_secs(1))
            .expect("release async paste observer");

        let calls = Arc::clone(&self.calls);
        let clipboard = self.clipboard.clone();
        let observed_text_tx = self
            .observed_text_tx
            .lock()
            .expect("observed sender mutex poisoned")
            .take()
            .expect("observed text sender should only be used once");
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(5));
            let observed_text = clipboard.current_text();
            calls.lock().expect("calls mutex poisoned").push(format!(
                "async_paste_observed:{}",
                observed_text.as_deref().unwrap_or("<empty>")
            ));
            observed_text_tx
                .send(observed_text)
                .expect("send observed async clipboard text");
        });

        Ok(())
    }
}

#[derive(Debug)]
struct DelayedAsyncClipboardReaderPasteShortcut {
    calls: Arc<Mutex<Vec<String>>>,
    clipboard: RecordingClipboard,
    observe_after: Duration,
    observed_text_tx: Mutex<Option<mpsc::Sender<Option<String>>>>,
}

impl DelayedAsyncClipboardReaderPasteShortcut {
    fn new(
        calls: Arc<Mutex<Vec<String>>>,
        clipboard: RecordingClipboard,
        observe_after: Duration,
        observed_text_tx: mpsc::Sender<Option<String>>,
    ) -> Self {
        Self {
            calls,
            clipboard,
            observe_after,
            observed_text_tx: Mutex::new(Some(observed_text_tx)),
        }
    }
}

impl PasteShortcut for DelayedAsyncClipboardReaderPasteShortcut {
    fn send_paste(&self) -> Result<(), TalkError> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push("paste_shortcut".to_string());

        let calls = Arc::clone(&self.calls);
        let clipboard = self.clipboard.clone();
        let observe_after = self.observe_after;
        let observed_text_tx = self
            .observed_text_tx
            .lock()
            .expect("observed sender mutex poisoned")
            .take()
            .expect("observed text sender should only be used once");
        thread::spawn(move || {
            thread::sleep(observe_after);
            let observed_text = clipboard.current_text();
            calls.lock().expect("calls mutex poisoned").push(format!(
                "delayed_async_paste_observed:{}",
                observed_text.as_deref().unwrap_or("<empty>")
            ));
            observed_text_tx
                .send(observed_text)
                .expect("send observed delayed async clipboard text");
        });

        Ok(())
    }
}

fn recorded_calls(calls: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    calls.lock().expect("calls mutex poisoned").clone()
}
