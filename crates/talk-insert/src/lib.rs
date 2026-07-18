use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
pub use talk_core::NativeReadinessStatus;
use talk_core::TalkError;

mod patch;
pub use patch::{compute_patch_edit_ratio, should_auto_apply_corrected_text};

// Real desktop targets can return from Ctrl+V before they have actually consumed the
// clipboard payload. Keep the inserted text available long enough to avoid restoring
// the user's original clipboard contents back into a slow paste consumer.
const CLIPBOARD_RESTORE_SETTLE_DELAY: Duration = Duration::from_millis(500);
pub const TALK_WINDOWS_PASTE_SHORTCUT_ENV: &str = "TALK_WINDOWS_PASTE_SHORTCUT";

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

#[derive(Debug, Clone)]
pub struct ClipboardPasteInserter<C, P> {
    clipboard: C,
    paste_shortcut: P,
    restore_policy: ClipboardRestorePolicy,
}

impl<C, P> ClipboardPasteInserter<C, P> {
    pub fn new(clipboard: C, paste_shortcut: P, restore_policy: ClipboardRestorePolicy) -> Self {
        Self {
            clipboard,
            paste_shortcut,
            restore_policy,
        }
    }
}

impl<C, P> TextInserter for ClipboardPasteInserter<C, P>
where
    C: ClipboardBackend,
    P: PasteShortcut,
{
    fn insert_text(&self, text: &str) -> Result<InsertOutcome, TalkError> {
        reject_empty_text(text)?;

        match self.restore_policy {
            ClipboardRestorePolicy::RestoreOriginal => {
                let snapshot = self.clipboard.capture()?;
                self.clipboard.write_text(text)?;
                let paste_result = self.paste_shortcut.send_paste();
                if paste_result.is_ok() {
                    std::thread::sleep(CLIPBOARD_RESTORE_SETTLE_DELAY);
                }
                self.clipboard.restore(snapshot)?;
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

impl PasteShortcut for WindowsPasteShortcut {
    fn send_paste(&self) -> Result<(), TalkError> {
        if std::env::var_os("TALK_DISABLE_NATIVE_CLIPBOARD").is_some() {
            return Err(TalkError::Insert(
                "native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD"
                    .to_string(),
            ));
        }

        let mode = resolve_windows_paste_shortcut_mode_from_env_value(
            std::env::var(TALK_WINDOWS_PASTE_SHORTCUT_ENV)
                .ok()
                .as_deref(),
        );
        send_native_windows_paste_shortcut(mode)
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

                let locked = GlobalLock(handle as HGLOBAL) as *const u16;
                if locked.is_null() {
                    return Err(last_error("GlobalLock(clipboard text)"));
                }

                let mut len = 0usize;
                while *locked.add(len) != 0 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(locked, len);
                let text = String::from_utf16(slice).map_err(|error| {
                    TalkError::Insert(format!("clipboard text is not valid UTF-16: {error}"))
                })?;
                let _ = GlobalUnlock(handle as HGLOBAL);
                Some(text)
            }
        };

        Ok(WindowsClipboardSnapshot { text })
    }

    pub(super) fn write_clipboard_text(text: Option<&str>) -> Result<(), TalkError> {
        let _guard = ClipboardOpenGuard::open()?;
        unsafe {
            if EmptyClipboard() == 0 {
                return Err(last_error("EmptyClipboard"));
            }

            let Some(text) = text else {
                return Ok(());
            };

            let handle = wide_text_to_global_handle(text)?;
            if SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
                let _ = GlobalFree(handle);
                return Err(last_error("SetClipboardData(CF_UNICODETEXT)"));
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
        let mut inputs = inputs.to_vec();
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
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
            let opened = unsafe { OpenClipboard(std::ptr::null_mut::<std::ffi::c_void>() as HWND) };
            if opened == 0 {
                return Err(last_error("OpenClipboard"));
            }
            Ok(Self)
        }
    }

    impl Drop for ClipboardOpenGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    fn last_error(operation: &str) -> TalkError {
        let code = unsafe { GetLastError() };
        TalkError::Insert(format!("{operation} failed with Windows error {code}"))
    }
}
