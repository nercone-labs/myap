//! The C interface of the Myap core (the library `myap_ffi`), for every platform: a core is
//! created, given JSON inputs (`myap::Input`) one at a time, and answers each with a JSON response
//! (`myap::Response`). Graph builds run apart, on any thread. The bytes are UTF-8 without a
//! terminating NUL.
//!
//! ```c
//! typedef struct MyapCore MyapCore;
//! MyapCore *myap_core_new(void);
//! uint8_t *myap_core_handle(MyapCore *core, const uint8_t *input, size_t length, size_t *response_length);
//! uint8_t *myap_build_graph(const uint8_t *input, size_t length, size_t *response_length);
//! void myap_buffer_free(uint8_t *buffer, size_t length);
//! void myap_core_free(MyapCore *core);
//! ```
//!
//! A core is not thread-safe: the caller serialises its calls. `myap_build_graph` holds no state
//! and may run on any thread, alongside the core. A panic is caught and answered with an error,
//! so a bug never takes the application down in the middle of a navigation.
//!
//! `jni` holds the same interface for the Java Native Interface (Android, and the JVM on
//! desktops).

mod jni;

use myap::{Core, GraphBuild};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// An opaque core for C.
pub struct MyapCore {
    core: Core,
}

/// Running the core and the builds, with every panic caught.
pub struct Calls;

impl Calls {
    /// `text` as a JSON string literal.
    fn json_string(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 2);
        out.push('"');
        for c in text.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
        panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_default()
    }

    /// Handles one JSON input. Never panics: a panic of the core becomes an error response.
    pub fn handle(core: &mut MyapCore, input: &[u8]) -> Vec<u8> {
        let Ok(text) = std::str::from_utf8(input) else { return br#"{"effects":[],"error":"the input is not UTF-8"}"#.to_vec() };
        let core = &mut core.core;
        match catch_unwind(AssertUnwindSafe(|| core.handle_json(text))) {
            Ok(response) => response.into_bytes(),
            Err(panic) => {
                format!(r#"{{"effects":[],"error":{}}}"#, Self::json_string(&format!("the core failed: {}", Self::panic_message(panic)))).into_bytes()
            }
        }
    }

    /// Runs one graph build given as JSON. Never panics.
    pub fn build_graph(input: &[u8]) -> Vec<u8> {
        let Ok(text) = std::str::from_utf8(input) else { return br#"{"Err":"the input is not UTF-8"}"#.to_vec() };
        match catch_unwind(|| GraphBuild::run_json(text)) {
            Ok(response) => response.into_bytes(),
            Err(panic) => format!(r#"{{"Err":{}}}"#, Self::json_string(&format!("the build failed: {}", Self::panic_message(panic)))).into_bytes(),
        }
    }

    /// The bytes of `input` (null with a length of 0 is empty).
    ///
    /// # Safety
    /// `input` points to `length` readable bytes, or is null with `length` 0.
    unsafe fn bytes<'a>(input: *const u8, length: usize) -> &'a [u8] {
        if input.is_null() || length == 0 { &[] } else { unsafe { std::slice::from_raw_parts(input, length) } }
    }

    /// Hands `response` to C, writing its length.
    ///
    /// # Safety
    /// `response_length` points to a writable `size_t`.
    unsafe fn give(response: Vec<u8>, response_length: *mut usize) -> *mut u8 {
        let response = response.into_boxed_slice();
        unsafe { *response_length = response.len() };
        Box::into_raw(response) as *mut u8
    }
}

/// Creates a core. Free it with `myap_core_free`.
#[unsafe(no_mangle)]
pub extern "C" fn myap_core_new() -> *mut MyapCore {
    Box::into_raw(Box::new(MyapCore { core: Core::new() }))
}

/// Frees a core. A null pointer is ignored.
///
/// # Safety
/// `core` is null or a pointer from `myap_core_new` not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn myap_core_free(core: *mut MyapCore) {
    if !core.is_null() {
        // SAFETY: the caller passes a pointer from `myap_core_new`, once.
        drop(unsafe { Box::from_raw(core) });
    }
}

/// Handles `length` bytes of JSON at `input` and returns the response, whose length is written to
/// `response_length`. Free the response with `myap_buffer_free`. Returns null when `core` or
/// `response_length` is null.
///
/// # Safety
/// `core` is a live core; `input` points to `length` readable bytes (or is null with `length` 0);
/// `response_length` points to a writable `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn myap_core_handle(core: *mut MyapCore, input: *const u8, length: usize, response_length: *mut usize) -> *mut u8 {
    if core.is_null() || response_length.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: checked for null; the caller guarantees the rest.
    unsafe {
        let response = Calls::handle(&mut *core, Calls::bytes(input, length));
        Calls::give(response, response_length)
    }
}

/// Runs a graph build (`myap::GraphBuild` as JSON) and returns its result as JSON. May be called
/// from any thread. Returns null when `response_length` is null.
///
/// # Safety
/// `input` points to `length` readable bytes (or is null with `length` 0); `response_length`
/// points to a writable `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn myap_build_graph(input: *const u8, length: usize, response_length: *mut usize) -> *mut u8 {
    if response_length.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: as documented.
    unsafe {
        let response = Calls::build_graph(Calls::bytes(input, length));
        Calls::give(response, response_length)
    }
}

/// Frees a response, with the length it gave.
///
/// # Safety
/// `buffer` is null or a response of this library not yet freed, of `length` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn myap_buffer_free(buffer: *mut u8, length: usize) {
    if !buffer.is_null() {
        // SAFETY: the buffer was a boxed slice of this length.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buffer, length)) });
    }
}
