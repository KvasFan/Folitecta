//! libxdo, loaded when it is first used.
//!
//! The menu library of the interface toolkit uses libxdo for the predefined
//! Copy, Cut, Paste and Select All items of native menus on Linux. Folitecta
//! shows no native menu, but the bindings from crates.io link the library, so
//! the program could not start where `libxdo.so.3` is missing (systems with
//! libxdo 4 only, desktops without the package). This crate takes their place
//! (see `[patch.crates-io]` in the workspace manifest) with the same interface:
//! `libxdo.so.3` is opened the first time an [`XDo`] is made, and without it
//! [`XDo::new`] fails with [`CreationError::Ffi`], as when no display can be
//! opened. Other versions of the library are not opened: their functions are
//! not known to take the same arguments.

use std::error::Error;
use std::ffi::{CString, NulError, c_char, c_int, c_uint, c_ulong, c_void};
use std::fmt;
use std::sync::OnceLock;

/// An X window (`Window`, an XID).
type Window = c_ulong;

/// The window that has the focus.
const CURRENTWINDOW: Window = 0;

type NewFn = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type FreeFn = unsafe extern "C" fn(*mut c_void);
type MoveFn = unsafe extern "C" fn(*const c_void, c_int, c_int, c_int) -> c_int;
type MoveRelativeFn = unsafe extern "C" fn(*const c_void, c_int, c_int) -> c_int;
type ButtonFn = unsafe extern "C" fn(*const c_void, Window, c_int) -> c_int;
type TextFn = unsafe extern "C" fn(*const c_void, Window, *const c_char, c_uint) -> c_int;

/// The functions of the library used here.
struct Api {
    new: NewFn,
    free: FreeFn,
    move_mouse: MoveFn,
    move_mouse_relative: MoveRelativeFn,
    click_window: ButtonFn,
    mouse_down: ButtonFn,
    mouse_up: ButtonFn,
    enter_text_window: TextFn,
    send_keysequence_window: TextFn,
    send_keysequence_window_up: TextFn,
    send_keysequence_window_down: TextFn,
}

/// The library, opened at the first use (None: it is missing, or lacks one
/// of the functions).
fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(open_library).as_ref()
}

fn open_library() -> Option<Api> {
    // SAFETY: dlopen with a constant, nul-terminated name; the handle is
    // never closed, so the functions taken from it stay valid.
    let lib = unsafe { libc::dlopen(c"libxdo.so.3".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if lib.is_null() {
        return None;
    }
    macro_rules! function {
        ($name:literal as $ty:ty) => {{
            // SAFETY: a constant, nul-terminated name in a handle that stays open.
            let f = unsafe { libc::dlsym(lib, $name.as_ptr()) };
            if f.is_null() {
                return None;
            }
            // SAFETY: the function of libxdo 3 with this name has this type
            // (xdo.h); a function pointer has the size of a data pointer on
            // the systems that have dlsym.
            unsafe { std::mem::transmute::<*mut c_void, $ty>(f) }
        }};
    }
    Some(Api {
        new: function!(c"xdo_new" as NewFn),
        free: function!(c"xdo_free" as FreeFn),
        move_mouse: function!(c"xdo_move_mouse" as MoveFn),
        move_mouse_relative: function!(c"xdo_move_mouse_relative" as MoveRelativeFn),
        click_window: function!(c"xdo_click_window" as ButtonFn),
        mouse_down: function!(c"xdo_mouse_down" as ButtonFn),
        mouse_up: function!(c"xdo_mouse_up" as ButtonFn),
        enter_text_window: function!(c"xdo_enter_text_window" as TextFn),
        send_keysequence_window: function!(c"xdo_send_keysequence_window" as TextFn),
        send_keysequence_window_up: function!(c"xdo_send_keysequence_window_up" as TextFn),
        send_keysequence_window_down: function!(c"xdo_send_keysequence_window_down" as TextFn),
    })
}

/// The main handle type which provides access to the various operations.
pub struct XDo {
    handle: *mut c_void,
    api: &'static Api,
}

/// An error that can happen when trying to create an `XDo` instance.
#[derive(Debug)]
pub enum CreationError {
    /// The provided string parameter had an interior null byte in it.
    Nul(NulError),
    /// The library is missing, or failed to create an instance (no display).
    Ffi,
}

impl fmt::Display for CreationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CreationError::Nul(err) => {
                write!(f, "Failed to create XDo instance: Nul byte in argument: {err}")
            }
            CreationError::Ffi => write!(f, "Libxdo failed to create an instance."),
        }
    }
}

impl Error for CreationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CreationError::Nul(err) => Some(err),
            CreationError::Ffi => None,
        }
    }
}

impl From<NulError> for CreationError {
    fn from(err: NulError) -> CreationError {
        CreationError::Nul(err)
    }
}

/// An error that can happen while executing an operation.
#[derive(Debug)]
pub enum OpError {
    /// The provided string parameter had an interior null byte in it.
    Nul(NulError),
    /// Libxdo failed, returning an error code.
    Ffi(i32),
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OpError::Nul(err) => write!(f, "Xdo operation failed: Nul byte in argument: {err}"),
            OpError::Ffi(code) => write!(f, "Xdo operation failed. Error code {code}."),
        }
    }
}

impl Error for OpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            OpError::Nul(err) => Some(err),
            OpError::Ffi(_) => None,
        }
    }
}

impl From<NulError> for OpError {
    fn from(err: NulError) -> Self {
        OpError::Nul(err)
    }
}

/// Result of an `XDo` operation.
pub type OpResult = Result<(), OpError>;

fn result(code: c_int) -> OpResult {
    match code {
        0 => Ok(()),
        code => Err(OpError::Ffi(code)),
    }
}

impl XDo {
    /// Creates a new `XDo` instance.
    ///
    /// `display` is an optional display name, such as `":0"`; with `None`,
    /// `$DISPLAY` is used. Fails with [`CreationError::Ffi`] when
    /// `libxdo.so.3` is missing or no display can be opened.
    pub fn new(display: Option<&str>) -> Result<XDo, CreationError> {
        let name = display.map(CString::new).transpose()?;
        let api = api().ok_or(CreationError::Ffi)?;
        let display = name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr());
        // SAFETY: a valid C string or null, as xdo_new takes.
        let handle = unsafe { (api.new)(display) };
        if handle.is_null() {
            return Err(CreationError::Ffi);
        }
        Ok(XDo { handle, api })
    }

    /// Moves the mouse to the specified position.
    pub fn move_mouse(&self, x: i32, y: i32, screen: i32) -> OpResult {
        // SAFETY: a live handle made by xdo_new.
        result(unsafe { (self.api.move_mouse)(self.handle, x, y, screen) })
    }

    /// Moves the mouse relative to the current position.
    pub fn move_mouse_relative(&self, x: i32, y: i32) -> OpResult {
        // SAFETY: a live handle made by xdo_new.
        result(unsafe { (self.api.move_mouse_relative)(self.handle, x, y) })
    }

    /// Does a mouse click.
    pub fn click(&self, button: i32) -> OpResult {
        // SAFETY: a live handle made by xdo_new.
        result(unsafe { (self.api.click_window)(self.handle, CURRENTWINDOW, button) })
    }

    /// Holds a mouse button down.
    pub fn mouse_down(&self, button: i32) -> OpResult {
        // SAFETY: a live handle made by xdo_new.
        result(unsafe { (self.api.mouse_down)(self.handle, CURRENTWINDOW, button) })
    }

    /// Releases a mouse button.
    pub fn mouse_up(&self, button: i32) -> OpResult {
        // SAFETY: a live handle made by xdo_new.
        result(unsafe { (self.api.mouse_up)(self.handle, CURRENTWINDOW, button) })
    }

    /// Types the specified text.
    pub fn enter_text(&self, text: &str, delay_microsecs: u32) -> OpResult {
        let text = CString::new(text)?;
        // SAFETY: a live handle and a C string that outlives the call.
        result(unsafe {
            (self.api.enter_text_window)(self.handle, CURRENTWINDOW, text.as_ptr(), delay_microsecs)
        })
    }

    /// Does the specified key sequence.
    pub fn send_keysequence(&self, sequence: &str, delay_microsecs: u32) -> OpResult {
        let sequence = CString::new(sequence)?;
        // SAFETY: a live handle and a C string that outlives the call.
        result(unsafe {
            (self.api.send_keysequence_window)(self.handle, CURRENTWINDOW, sequence.as_ptr(), delay_microsecs)
        })
    }

    /// Releases the specified key sequence.
    pub fn send_keysequence_up(&self, sequence: &str, delay_microsecs: u32) -> OpResult {
        let sequence = CString::new(sequence)?;
        // SAFETY: a live handle and a C string that outlives the call.
        result(unsafe {
            (self.api.send_keysequence_window_up)(
                self.handle,
                CURRENTWINDOW,
                sequence.as_ptr(),
                delay_microsecs,
            )
        })
    }

    /// Presses the specified key sequence down.
    pub fn send_keysequence_down(&self, sequence: &str, delay_microsecs: u32) -> OpResult {
        let sequence = CString::new(sequence)?;
        // SAFETY: a live handle and a C string that outlives the call.
        result(unsafe {
            (self.api.send_keysequence_window_down)(
                self.handle,
                CURRENTWINDOW,
                sequence.as_ptr(),
                delay_microsecs,
            )
        })
    }
}

impl Drop for XDo {
    fn drop(&mut self) {
        // SAFETY: the handle was made by xdo_new and is freed once.
        unsafe { (self.api.free)(self.handle) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_with_a_nul_byte_are_refused_before_the_library_is_opened() {
        assert!(matches!(XDo::new(Some("bad\0name")), Err(CreationError::Nul(_))));
    }

    #[test]
    fn a_display_that_cannot_be_opened_is_an_error_with_or_without_the_library() {
        // Never the user's display: a name no X server has.
        assert!(matches!(
            XDo::new(Some(":folitecta-no-such-display")),
            Err(CreationError::Ffi)
        ));
    }

    #[test]
    fn errors_say_what_failed() {
        assert_eq!(OpError::Ffi(3).to_string(), "Xdo operation failed. Error code 3.");
        assert!(CreationError::Ffi.source().is_none());
        let nul = CString::new("a\0b").unwrap_err();
        assert!(OpError::from(nul).source().is_some());
    }
}
