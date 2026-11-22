//! The Java Native Interface of the core, for the class `dev.nercone.myap.core.NativeCore`:
//!
//! ```kotlin
//! object NativeCore {
//!     @JvmStatic external fun create(): Long
//!     @JvmStatic external fun handle(core: Long, input: ByteArray): ByteArray?
//!     @JvmStatic external fun buildGraph(input: ByteArray): ByteArray?
//!     @JvmStatic external fun destroy(core: Long)
//! }
//! ```
//!
//! Byte arrays carry UTF-8, not Java strings, whose modified UTF-8 would need converting.

use crate::{Calls, MyapCore};
use jni_sys::{JNIEnv, jbyte, jbyteArray, jclass, jlong, jsize};

/// Reading and making the byte arrays of Java.
struct Arrays;

impl Arrays {
    /// The bytes of `array`, or `None` when the JVM could not give them (an exception is pending).
    ///
    /// # Safety
    /// `env` is the JNI environment of this thread; `array` is a live byte array.
    unsafe fn read(env: *mut JNIEnv, array: jbyteArray) -> Option<Vec<u8>> {
        // SAFETY: the functions of versions 1.1 and 1.2 are present in every JVM Android and the
        // desktops run.
        unsafe {
            let functions = (**env).v1_1;
            let exception_check = (**env).v1_2.ExceptionCheck;
            let length = (functions.GetArrayLength)(env, array);
            if length < 0 {
                return None;
            }
            let mut bytes = vec![0u8; length as usize];
            (functions.GetByteArrayRegion)(env, array, 0, length, bytes.as_mut_ptr() as *mut jbyte);
            if exception_check(env) {
                return None;
            }
            Some(bytes)
        }
    }

    /// A new byte array of `bytes`, or null when it could not be made.
    ///
    /// # Safety
    /// `env` is the JNI environment of this thread.
    unsafe fn make(env: *mut JNIEnv, bytes: &[u8]) -> jbyteArray {
        let Ok(size) = jsize::try_from(bytes.len()) else { return std::ptr::null_mut() };
        // SAFETY: as above.
        unsafe {
            let functions = (**env).v1_1;
            let array = (functions.NewByteArray)(env, size);
            if array.is_null() {
                return std::ptr::null_mut();
            }
            (functions.SetByteArrayRegion)(env, array, 0, size, bytes.as_ptr() as *const jbyte);
            array
        }
    }
}

/// # Safety
/// Called by the JVM.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_nercone_myap_core_NativeCore_create(_env: *mut JNIEnv, _class: jclass) -> jlong {
    crate::myap_core_new() as jlong
}

/// # Safety
/// Called by the JVM with a handle from `create` not yet destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_nercone_myap_core_NativeCore_destroy(_env: *mut JNIEnv, _class: jclass, core: jlong) {
    // SAFETY: the handle came from `create`.
    unsafe { crate::myap_core_free(core as *mut MyapCore) };
}

/// Returns the response, or null when the input could not be read or the array could not be
/// made (an exception is then pending in the JVM).
///
/// # Safety
/// Called by the JVM with a handle from `create`, from one thread at a time.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_nercone_myap_core_NativeCore_handle(env: *mut JNIEnv, _class: jclass, core: jlong, input: jbyteArray) -> jbyteArray {
    if core == 0 || input.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `env` is this thread's; the handle came from `create`.
    unsafe {
        let Some(bytes) = Arrays::read(env, input) else { return std::ptr::null_mut() };
        let response = Calls::handle(&mut *(core as *mut MyapCore), &bytes);
        Arrays::make(env, &response)
    }
}

/// Runs a graph build; any thread.
///
/// # Safety
/// Called by the JVM.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_nercone_myap_core_NativeCore_buildGraph(env: *mut JNIEnv, _class: jclass, input: jbyteArray) -> jbyteArray {
    if input.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `env` is this thread's.
    unsafe {
        let Some(bytes) = Arrays::read(env, input) else { return std::ptr::null_mut() };
        Arrays::make(env, &Calls::build_graph(&bytes))
    }
}
