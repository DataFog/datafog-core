use napi::bindgen_prelude::{Buffer, FromNapiValue};
use napi::{Env, Error, Status, sys};
use std::ptr;

/// Use JavaScript-owned storage so worker teardown cannot strand an external
/// Buffer finalizer on a closing Node environment (nodejs/node#67417).
pub(crate) fn copied_output_buffer(env: &Env, bytes: &[u8]) -> napi::Result<Buffer> {
    let mut value = ptr::null_mut();
    // The input remains borrowed and live until the synchronous copy completes.
    let status = unsafe {
        sys::napi_create_buffer_copy(
            env.raw(),
            bytes.len(),
            bytes.as_ptr().cast(),
            ptr::null_mut(),
            &mut value,
        )
    };
    if status != sys::Status::napi_ok {
        return Err(Error::new(
            Status::from(status),
            "Failed to copy output into a JavaScript-owned Buffer",
        ));
    }
    // This value is a Buffer in the callback's live environment. Retaining its
    // reference makes Buffer's output conversion reuse it without an external
    // backing-store callback.
    unsafe { Buffer::from_napi_value(env.raw(), value) }
}
