use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
pub use talk_core::NativeReadinessStatus;
use talk_core::TalkError;

mod patch;
pub use patch::{compute_patch_edit_ratio, should_auto_apply_corrected_text};

// Real desktop targets can return from Ctrl+V before they have actually consumed the
// clipboard payload. Keep the inserted text available long enough to avoid restoring
// the user's original clipboard contents back into a slow paste consumer.
pub const DEFAULT_CLIPBOARD_RESTORE_SETTLE_DELAY: Duration = Duration::from_millis(500);
pub const TALK_WINDOWS_PASTE_SHORTCUT_ENV: &str = "TALK_WINDOWS_PASTE_SHORTCUT";
pub const TALK_WINDOWS_PASTE_TARGET_HWND_ENV: &str = "TALK_WINDOWS_PASTE_TARGET_HWND";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertMethod {
    DryRun,
    ClipboardPaste,
    ClipboardFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertOutcome {
    Inserted { method: InsertMethod },
    FallbackClipboard { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeWindowsClipboardReadiness {
    pub status: NativeReadinessStatus,
    pub reason: Option<String>,
}

pub trait TextInserter {
    fn insert_text(&self, text: &str) -> Result<InsertOutcome, TalkError>;
}

pub trait ClipboardBackend {
    type Snapshot;

    fn capture(&self) -> Result<Self::Snapshot, TalkError>;
    fn write_text(&self, text: &str) -> Result<(), TalkError>;
    fn restore(&self, snapshot: Self::Snapshot) -> Result<(), TalkError>;
}

pub trait PasteShortcut {
    fn send_paste(&self) -> Result<(), TalkError>;
}

pub struct BeforePasteShortcut<P, H> {
    inner: P,
    before_paste: H,
}

impl<P, H> BeforePasteShortcut<P, H> {
    pub fn new(inner: P, before_paste: H) -> Self {
        Self {
            inner,
            before_paste,
        }
    }
}

impl<P, H> PasteShortcut for BeforePasteShortcut<P, H>
where
    P: PasteShortcut,
    H: Fn(),
{
    fn send_paste(&self) -> Result<(), TalkError> {
        (self.before_paste)();
        self.inner.send_paste()
    }
}

pub struct AroundPasteShortcut<P, B, A> {
    inner: P,
    before_paste: B,
    after_paste: A,
}

impl<P, B, A> AroundPasteShortcut<P, B, A> {
    pub fn new(inner: P, before_paste: B, after_paste: A) -> Self {
        Self {
            inner,
            before_paste,
            after_paste,
        }
    }
}

impl<P, B, A> PasteShortcut for AroundPasteShortcut<P, B, A>
where
    P: PasteShortcut,
    B: Fn(),
    A: Fn(),
{
    fn send_paste(&self) -> Result<(), TalkError> {
        (self.before_paste)();
        let result = self.inner.send_paste();
        (self.after_paste)();
        result
    }
}

pub struct AroundTextInserter<I, B, A> {
    inner: I,
    before_insert: B,
    after_insert: A,
}

impl<I, B, A> AroundTextInserter<I, B, A> {
    pub fn new(inner: I, before_insert: B, after_insert: A) -> Self {
        Self {
            inner,
            before_insert,
            after_insert,
        }
    }
}

impl<I, B, A> TextInserter for AroundTextInserter<I, B, A>
where
    I: TextInserter,
    B: Fn(),
    A: Fn(),
{
    fn insert_text(&self, text: &str) -> Result<InsertOutcome, TalkError> {
        (self.before_insert)();
        let result = self.inner.insert_text(text);
        (self.after_insert)();
        result
    }
}

pub fn probe_native_windows_clipboard_readiness() -> NativeWindowsClipboardReadiness {
    if std::env::var_os("TALK_DISABLE_NATIVE_CLIPBOARD").is_some() {
        return NativeWindowsClipboardReadiness::unavailable(
            "native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD",
        );
    }

    probe_native_windows_clipboard_readiness_impl()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardRestorePolicy {
    RestoreOriginal,
    LeaveInsertedText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClipboardRestoreTiming {
    #[default]
    Immediate,
    Deferred,
}

fn pending_clipboard_restore() -> &'static Mutex<Option<JoinHandle<()>>> {
    static PENDING: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

fn clipboard_insert_transaction() -> &'static Mutex<()> {
    static TRANSACTION: OnceLock<Mutex<()>> = OnceLock::new();
    TRANSACTION.get_or_init(|| Mutex::new(()))
}

fn flush_pending_clipboard_restore_locked() {
    let handle = pending_clipboard_restore()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    if let Some(handle) = handle {
        if handle.join().is_err() {
            eprintln!("Talk deferred clipboard restore worker panicked");
        }
    }
}

/// Waits for a deferred clipboard restore started by a previous insert.
/// Product shells should call this during shutdown so a short-lived process
/// does not exit before the original clipboard is restored.
pub fn flush_pending_clipboard_restore() {
    let _transaction = clipboard_insert_transaction()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    flush_pending_clipboard_restore_locked();
}

fn schedule_clipboard_restore(job: impl FnOnce() + Send + 'static) {
    match pending_clipboard_restore().lock() {
        Ok(mut slot) => {
            *slot = Some(thread::spawn(job));
        }
        Err(poisoned) => {
            *poisoned.into_inner() = Some(thread::spawn(job));
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClipboardPasteInserter<C, P> {
    clipboard: C,
    paste_shortcut: P,
    restore_policy: ClipboardRestorePolicy,
    settle_delay: Duration,
    restore_timing: ClipboardRestoreTiming,
}

impl<C, P> ClipboardPasteInserter<C, P> {
    pub fn new(clipboard: C, paste_shortcut: P, restore_policy: ClipboardRestorePolicy) -> Self {
        Self::with_settle_delay(
            clipboard,
            paste_shortcut,
            restore_policy,
            DEFAULT_CLIPBOARD_RESTORE_SETTLE_DELAY,
        )
    }

    pub fn with_settle_delay(
        clipboard: C,
        paste_shortcut: P,
        restore_policy: ClipboardRestorePolicy,
        settle_delay: Duration,
    ) -> Self {
        Self {
            clipboard,
            paste_shortcut,
            restore_policy,
            settle_delay,
            restore_timing: ClipboardRestoreTiming::Immediate,
        }
    }

    pub fn with_deferred_restore(mut self) -> Self {
        self.restore_timing = ClipboardRestoreTiming::Deferred;
        self
    }
}

impl<C, P> TextInserter for ClipboardPasteInserter<C, P>
where
    C: ClipboardBackend + Clone + Send + 'static,
    C::Snapshot: Send + 'static,
    P: PasteShortcut,
{
    fn insert_text(&self, text: &str) -> Result<InsertOutcome, TalkError> {
        reject_empty_text(text)?;
        let _transaction = clipboard_insert_transaction()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // A prior deferred restore must finish before another clipboard
        // transaction captures or writes the shared system clipboard.
        flush_pending_clipboard_restore_locked();

        match self.restore_policy {
            ClipboardRestorePolicy::RestoreOriginal => {
                let snapshot = self.clipboard.capture()?;
                self.clipboard.write_text(text)?;
                let paste_result = self.paste_shortcut.send_paste();
                if paste_result.is_ok() && self.restore_timing == ClipboardRestoreTiming::Deferred {
                    let clipboard = self.clipboard.clone();
                    let settle_delay = self.settle_delay;
                    schedule_clipboard_restore(move || {
                        if !settle_delay.is_zero() {
                            thread::sleep(settle_delay);
                        }
                        if let Err(error) = clipboard.restore(snapshot) {
                            eprintln!("Talk clipboard restore after paste failed: {error}");
                        }
                    });
                } else {
                    if paste_result.is_ok() && !self.settle_delay.is_zero() {
                        thread::sleep(self.settle_delay);
                    }
                    if let Err(error) = self.clipboard.restore(snapshot) {
                        // The paste may already have reached the target. Do not turn a
                        // post-paste clipboard cleanup failure into a retryable insert.
                        eprintln!("Talk clipboard restore after paste failed: {error}");
                    }
                }
                paste_result?;
            }
            ClipboardRestorePolicy::LeaveInsertedText => {
                self.clipboard.write_text(text)?;
                self.paste_shortcut.send_paste()?;
            }
        }

        Ok(InsertOutcome::Inserted {
            method: InsertMethod::ClipboardPaste,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct DryRunInserter {
    last_text: Arc<Mutex<Option<String>>>,
}

impl DryRunInserter {
    pub fn last_text(&self) -> Option<String> {
        self.last_text
            .lock()
            .expect("dry-run inserter mutex poisoned")
            .clone()
    }
}

impl TextInserter for DryRunInserter {
    fn insert_text(&self, text: &str) -> Result<InsertOutcome, TalkError> {
        reject_empty_text(text)?;

        *self
            .last_text
            .lock()
            .map_err(|error| TalkError::Insert(error.to_string()))? = Some(text.to_string());
        Ok(InsertOutcome::Inserted {
            method: InsertMethod::DryRun,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClipboardFallbackInserter;

impl TextInserter for ClipboardFallbackInserter {
    fn insert_text(&self, text: &str) -> Result<InsertOutcome, TalkError> {
        reject_empty_text(text)?;
        Ok(InsertOutcome::FallbackClipboard {
            reason:
                "native clipboard paste is not enabled for the configured Talk fallback backend"
                    .to_string(),
        })
    }
}

fn reject_empty_text(text: &str) -> Result<(), TalkError> {
    if text.trim().is_empty() {
        return Err(TalkError::Insert(
            "refusing to insert empty text".to_string(),
        ));
    }
    Ok(())
}

impl NativeWindowsClipboardReadiness {
    fn ready() -> Self {
        Self {
            status: NativeReadinessStatus::Ready,
            reason: None,
        }
    }

    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            status: NativeReadinessStatus::Unavailable,
            reason: Some(reason.into()),
        }
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Default)]
pub struct WindowsClipboardBackend;

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsClipboardSnapshot {
    text: Option<String>,
}

#[cfg(windows)]
impl ClipboardBackend for WindowsClipboardBackend {
    type Snapshot = WindowsClipboardSnapshot;

    fn capture(&self) -> Result<Self::Snapshot, TalkError> {
        windows_native::capture_clipboard_text()
    }

    fn write_text(&self, text: &str) -> Result<(), TalkError> {
        windows_native::write_clipboard_text(Some(text))
    }

    fn restore(&self, snapshot: Self::Snapshot) -> Result<(), TalkError> {
        windows_native::write_clipboard_text(snapshot.text.as_deref())
    }
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Default)]
pub struct WindowsClipboardBackend;

#[cfg(not(windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsClipboardSnapshot;

#[cfg(not(windows))]
impl ClipboardBackend for WindowsClipboardBackend {
    type Snapshot = WindowsClipboardSnapshot;

    fn capture(&self) -> Result<Self::Snapshot, TalkError> {
        Err(native_windows_unavailable())
    }

    fn write_text(&self, _text: &str) -> Result<(), TalkError> {
        Err(native_windows_unavailable())
    }

    fn restore(&self, _snapshot: Self::Snapshot) -> Result<(), TalkError> {
        Err(native_windows_unavailable())
    }
}

#[derive(Debug, Clone, Default)]
pub struct WindowsPasteShortcut;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsPasteShortcutMode {
    ControlV,
    ControlShiftV,
    ShiftInsert,
}

/// Explicit paste parameters. Unset fields fall back to the per-thread
/// overrides installed via [`set_windows_paste_thread_overrides`], and finally
/// to the `TALK_WINDOWS_PASTE_SHORTCUT` / `TALK_WINDOWS_PASTE_TARGET_HWND`
/// environment variables (kept as an external debugging override entry point).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowsPasteOverrides {
    pub shortcut_mode: Option<WindowsPasteShortcutMode>,
    pub target_hwnd: Option<isize>,
}

impl WindowsPasteOverrides {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shortcut_mode(mut self, shortcut_mode: WindowsPasteShortcutMode) -> Self {
        self.shortcut_mode = Some(shortcut_mode);
        self
    }

    pub fn with_target_hwnd(mut self, target_hwnd: isize) -> Self {
        self.target_hwnd = Some(target_hwnd);
        self
    }
}

thread_local! {
    static WINDOWS_PASTE_THREAD_OVERRIDES: std::cell::Cell<WindowsPasteOverrides> =
        const { std::cell::Cell::new(WindowsPasteOverrides {
            shortcut_mode: None,
            target_hwnd: None,
        }) };
}

/// Installs paste overrides for the current thread and returns the previously
/// installed overrides so callers can restore them afterwards. This is the
/// race-free replacement for mutating the process-wide paste environment
/// variables around an insert executed on the same thread (for example the
/// runtime insert path that constructs its own [`WindowsPasteShortcut`]).
pub fn set_windows_paste_thread_overrides(
    overrides: WindowsPasteOverrides,
) -> WindowsPasteOverrides {
    WINDOWS_PASTE_THREAD_OVERRIDES.with(|cell| cell.replace(overrides))
}

/// Returns the paste overrides currently installed for this thread.
pub fn windows_paste_thread_overrides() -> WindowsPasteOverrides {
    WINDOWS_PASTE_THREAD_OVERRIDES.with(std::cell::Cell::get)
}

/// A [`PasteShortcut`] carrying explicit paste parameters. Explicit values win
/// over the per-thread overrides and the environment variables; unset values
/// keep the [`WindowsPasteShortcut`] fallback behavior.
#[derive(Debug, Clone, Default)]
pub struct ConfiguredWindowsPasteShortcut {
    overrides: WindowsPasteOverrides,
}

impl ConfiguredWindowsPasteShortcut {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shortcut_mode(mut self, shortcut_mode: WindowsPasteShortcutMode) -> Self {
        self.overrides = self.overrides.with_shortcut_mode(shortcut_mode);
        self
    }

    pub fn with_target_hwnd(mut self, target_hwnd: isize) -> Self {
        self.overrides = self.overrides.with_target_hwnd(target_hwnd);
        self
    }

    pub fn shortcut_mode(&self) -> Option<WindowsPasteShortcutMode> {
        self.overrides.shortcut_mode
    }

    pub fn target_hwnd(&self) -> Option<isize> {
        self.overrides.target_hwnd
    }
}

impl PasteShortcut for ConfiguredWindowsPasteShortcut {
    fn send_paste(&self) -> Result<(), TalkError> {
        send_windows_paste_with_explicit_overrides(self.overrides)
    }
}

/// The resolved paste parameters actually used by a native Windows paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsPastePlan {
    pub shortcut_mode: WindowsPasteShortcutMode,
    pub target_hwnd: Option<isize>,
}

/// Resolves the effective paste parameters. Priority per field: explicit
/// value, then per-thread override, then environment variable value.
pub fn resolve_windows_paste_plan(
    explicit: WindowsPasteOverrides,
    thread_overrides: WindowsPasteOverrides,
    shortcut_env_value: Option<&str>,
    target_hwnd_env_value: Option<&str>,
) -> WindowsPastePlan {
    WindowsPastePlan {
        shortcut_mode: explicit
            .shortcut_mode
            .or(thread_overrides.shortcut_mode)
            .unwrap_or_else(|| {
                resolve_windows_paste_shortcut_mode_from_env_value(shortcut_env_value)
            }),
        target_hwnd: explicit
            .target_hwnd
            .or(thread_overrides.target_hwnd)
            .or_else(|| resolve_windows_paste_target_hwnd_from_env_value(target_hwnd_env_value)),
    }
}

pub fn resolve_windows_paste_shortcut_mode_from_env_value(
    value: Option<&str>,
) -> WindowsPasteShortcutMode {
    match value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("ctrl_v") => WindowsPasteShortcutMode::ControlV,
        Some("ctrl_shift_v") => WindowsPasteShortcutMode::ControlShiftV,
        Some("shift_insert") => WindowsPasteShortcutMode::ShiftInsert,
        _ => WindowsPasteShortcutMode::ControlV,
    }
}

pub fn resolve_windows_paste_target_hwnd_from_env_value(value: Option<&str>) -> Option<isize> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        usize::from_str_radix(hex, 16)
            .ok()
            .map(|value| value as isize)
    } else {
        value.parse::<usize>().ok().map(|value| value as isize)
    }
}

fn send_windows_paste_with_explicit_overrides(
    explicit: WindowsPasteOverrides,
) -> Result<(), TalkError> {
    if std::env::var_os("TALK_DISABLE_NATIVE_CLIPBOARD").is_some() {
        return Err(TalkError::Insert(
            "native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD"
                .to_string(),
        ));
    }

    let plan = resolve_windows_paste_plan(
        explicit,
        windows_paste_thread_overrides(),
        std::env::var(TALK_WINDOWS_PASTE_SHORTCUT_ENV)
            .ok()
            .as_deref(),
        std::env::var(TALK_WINDOWS_PASTE_TARGET_HWND_ENV)
            .ok()
            .as_deref(),
    );

    if let Some(hwnd) = plan.target_hwnd {
        #[cfg(windows)]
        if windows_native::send_wm_paste(hwnd).is_ok() {
            return Ok(());
        }
    }

    send_native_windows_paste_shortcut(plan.shortcut_mode)
}

impl PasteShortcut for WindowsPasteShortcut {
    fn send_paste(&self) -> Result<(), TalkError> {
        send_windows_paste_with_explicit_overrides(WindowsPasteOverrides::default())
    }
}

#[cfg(windows)]
fn send_native_windows_paste_shortcut(mode: WindowsPasteShortcutMode) -> Result<(), TalkError> {
    match mode {
        WindowsPasteShortcutMode::ControlV => windows_native::send_ctrl_v(),
        WindowsPasteShortcutMode::ControlShiftV => windows_native::send_ctrl_shift_v(),
        WindowsPasteShortcutMode::ShiftInsert => windows_native::send_shift_insert(),
    }
}

#[cfg(not(windows))]
fn send_native_windows_paste_shortcut(_mode: WindowsPasteShortcutMode) -> Result<(), TalkError> {
    Err(native_windows_unavailable())
}

#[cfg(not(windows))]
fn native_windows_unavailable() -> TalkError {
    TalkError::Insert("native_windows clipboard backend is only available on Windows".to_string())
}

#[cfg(windows)]
fn probe_native_windows_clipboard_readiness_impl() -> NativeWindowsClipboardReadiness {
    match windows_native::capture_clipboard_text() {
        Ok(_) => NativeWindowsClipboardReadiness::ready(),
        Err(error) => NativeWindowsClipboardReadiness::unavailable(error.to_string()),
    }
}

#[cfg(not(windows))]
fn probe_native_windows_clipboard_readiness_impl() -> NativeWindowsClipboardReadiness {
    NativeWindowsClipboardReadiness::unavailable(
        "native_windows clipboard backend is only available on Windows",
    )
}

#[cfg(windows)]
mod windows_native {
    use super::WindowsClipboardSnapshot;
    use std::mem;
    use std::ptr;
    use talk_core::TalkError;
    use windows_sys::Win32::Foundation::{GetLastError, GlobalFree, HGLOBAL, HWND};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL,
        VK_INSERT, VK_SHIFT, VK_V,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsWindow, SendMessageTimeoutW, SMTO_ABORTIFHUNG, WM_PASTE,
    };

    pub(super) fn capture_clipboard_text() -> Result<WindowsClipboardSnapshot, TalkError> {
        let _guard = ClipboardOpenGuard::open()?;
        let text = unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) == 0 {
                None
            } else {
                let handle = GetClipboardData(CF_UNICODETEXT as u32);
                if handle.is_null() {
                    return Err(last_error("GetClipboardData(CF_UNICODETEXT)"));
                }

                let lock = GlobalLockGuard::lock(handle as HGLOBAL, "GlobalLock(clipboard text)")?;
                let locked = lock.pointer();
                let mut len = 0usize;
                while *locked.add(len) != 0 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(locked, len);
                let text = String::from_utf16(slice).map_err(|error| {
                    TalkError::Insert(format!("clipboard text is not valid UTF-16: {error}"))
                })?;
                Some(text)
            }
        };

        Ok(WindowsClipboardSnapshot { text })
    }

    pub(super) fn write_clipboard_text(text: Option<&str>) -> Result<(), TalkError> {
        let _guard = ClipboardOpenGuard::open()?;
        unsafe {
            // Allocate the global handle before EmptyClipboard so an allocation
            // failure cannot leave the user's clipboard already cleared.
            let handle = text
                .map(|text| wide_text_to_global_handle(text))
                .transpose()?;

            if EmptyClipboard() == 0 {
                let error = last_error("EmptyClipboard");
                if let Some(handle) = handle {
                    let _ = GlobalFree(handle);
                }
                return Err(error);
            }

            let Some(handle) = handle else {
                return Ok(());
            };
            if SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
                let error = last_error("SetClipboardData(CF_UNICODETEXT)");
                let _ = GlobalFree(handle);
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn send_ctrl_v() -> Result<(), TalkError> {
        let inputs = [
            keyboard_input(VK_CONTROL, 0),
            keyboard_input(VK_V, 0),
            keyboard_input(VK_V, KEYEVENTF_KEYUP),
            keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        send_inputs("SendInput(Ctrl+V)", &inputs)
    }

    pub(super) fn send_ctrl_shift_v() -> Result<(), TalkError> {
        let inputs = [
            keyboard_input(VK_CONTROL, 0),
            keyboard_input(VK_SHIFT, 0),
            keyboard_input(VK_V, 0),
            keyboard_input(VK_V, KEYEVENTF_KEYUP),
            keyboard_input(VK_SHIFT, KEYEVENTF_KEYUP),
            keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        send_inputs("SendInput(Ctrl+Shift+V)", &inputs)
    }

    pub(super) fn send_shift_insert() -> Result<(), TalkError> {
        let inputs = [
            keyboard_input(VK_SHIFT, 0),
            keyboard_input(VK_INSERT, 0),
            keyboard_input(VK_INSERT, KEYEVENTF_KEYUP),
            keyboard_input(VK_SHIFT, KEYEVENTF_KEYUP),
        ];
        send_inputs("SendInput(Shift+Insert)", &inputs)
    }

    pub(super) fn send_wm_paste(hwnd_value: isize) -> Result<(), TalkError> {
        let hwnd = hwnd_value as HWND;
        if hwnd.is_null() || unsafe { IsWindow(hwnd) } == 0 {
            return Err(TalkError::Insert(format!(
                "WM_PASTE target hwnd is invalid: {hwnd_value}"
            )));
        }

        let mut message_result = 0usize;
        let sent = unsafe {
            SendMessageTimeoutW(
                hwnd,
                WM_PASTE,
                0,
                0,
                SMTO_ABORTIFHUNG,
                400,
                &mut message_result,
            )
        };
        if sent == 0 {
            return Err(last_error("SendMessageTimeoutW(WM_PASTE)"));
        }
        Ok(())
    }

    unsafe fn wide_text_to_global_handle(text: &str) -> Result<HGLOBAL, TalkError> {
        let mut wide = text.encode_utf16().collect::<Vec<_>>();
        wide.push(0);
        let byte_len = wide.len() * mem::size_of::<u16>();
        let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, byte_len);
        if handle.is_null() {
            return Err(last_error("GlobalAlloc(clipboard text)"));
        }

        let locked = GlobalLock(handle) as *mut u16;
        if locked.is_null() {
            let _ = GlobalFree(handle);
            return Err(last_error("GlobalLock(allocated clipboard text)"));
        }

        ptr::copy_nonoverlapping(wide.as_ptr(), locked, wide.len());
        let _ = GlobalUnlock(handle);
        Ok(handle)
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

    fn send_inputs(operation: &str, inputs: &[INPUT]) -> Result<(), TalkError> {
        const MAX_INPUTS: usize = 8;
        if inputs.len() > MAX_INPUTS {
            return Err(TalkError::Insert(format!(
                "{operation} exceeds the stack SendInput buffer ({MAX_INPUTS})"
            )));
        }

        let mut storage = [unsafe { mem::zeroed::<INPUT>() }; MAX_INPUTS];
        storage[..inputs.len()].copy_from_slice(inputs);
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                storage.as_mut_ptr(),
                mem::size_of::<INPUT>() as i32,
            )
        };
        if sent != inputs.len() as u32 {
            return Err(last_error(operation));
        }
        Ok(())
    }

    struct ClipboardOpenGuard;

    impl ClipboardOpenGuard {
        fn open() -> Result<Self, TalkError> {
            // The clipboard is a system-wide resource that other processes
            // (including the paste target itself) briefly hold open, so a
            // single failed OpenClipboard is not conclusive. Retry a few times
            // before reporting the failure.
            const OPEN_ATTEMPTS: u32 = 5;
            const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(10);

            let mut last_open_error = None;
            for attempt in 0..OPEN_ATTEMPTS {
                let opened =
                    unsafe { OpenClipboard(std::ptr::null_mut::<std::ffi::c_void>() as HWND) };
                if opened != 0 {
                    return Ok(Self);
                }
                last_open_error = Some(last_error("OpenClipboard"));
                if attempt + 1 < OPEN_ATTEMPTS {
                    std::thread::sleep(RETRY_DELAY);
                }
            }
            Err(last_open_error.expect("at least one OpenClipboard attempt must run"))
        }
    }

    impl Drop for ClipboardOpenGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    struct GlobalLockGuard {
        handle: HGLOBAL,
        pointer: *const u16,
    }

    impl GlobalLockGuard {
        /// # Safety
        /// `handle` must be a valid HGLOBAL owning a NUL-terminated UTF-16
        /// buffer for as long as the guard is alive.
        unsafe fn lock(handle: HGLOBAL, operation: &str) -> Result<Self, TalkError> {
            let pointer = GlobalLock(handle) as *const u16;
            if pointer.is_null() {
                return Err(last_error(operation));
            }
            Ok(Self { handle, pointer })
        }

        fn pointer(&self) -> *const u16 {
            self.pointer
        }
    }

    impl Drop for GlobalLockGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = GlobalUnlock(self.handle);
            }
        }
    }

    fn last_error(operation: &str) -> TalkError {
        let code = unsafe { GetLastError() };
        TalkError::Insert(format!("{operation} failed with Windows error {code}"))
    }
}
