//! Node binding for datafog-core.

use napi::{Error, Status};
use napi_derive::napi;

#[napi(object, object_from_js = false)]
pub struct Entity {
    #[napi(readonly, ts_type = "Label")]
    pub label: String,

    #[napi(readonly)]
    pub text: String,

    #[napi(readonly)]
    pub start: u32,

    #[napi(readonly)]
    pub end: u32,
}

fn js_offset(offset: usize) -> napi::Result<u32> {
    u32::try_from(offset).map_err(|_| {
        Error::new(
            Status::GenericFailure,
            "entity offset exceeds the JavaScript binding limit",
        )
    })
}

/// Scan text for supported PII entities.
#[napi(strict, catch_unwind)]
pub fn scan(text: String) -> napi::Result<Vec<Entity>> {
    datafog_core::scan(&text)
        .into_iter()
        .map(|entity| {
            Ok(Entity {
                label: entity.label,
                text: entity.text,
                start: js_offset(entity.start)?,
                end: js_offset(entity.end)?,
            })
        })
        .collect()
}
