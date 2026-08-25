use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct Entity {
    label: String,
    text: String,
    start: usize,
    end: usize,
}

#[wasm_bindgen]
pub fn scan(text: &str) -> Result<JsValue, JsValue> {
    let entities: Vec<Entity> = datafog_core::scan(text)
        .into_iter()
        .map(|entity| Entity {
            label: entity.label,
            text: entity.text,
            start: entity.start,
            end: entity.end,
        })
        .collect();

    serde_wasm_bindgen::to_value(&entities).map_err(|error| JsValue::from_str(&error.to_string()))
}
