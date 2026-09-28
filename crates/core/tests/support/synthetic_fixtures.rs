// Test-only construction of explicitly synthetic provider-shaped credentials.
use serde_json::Value;

pub fn expand(mut row: Value) -> Value {
    let Some(recipes) = row.as_object_mut().unwrap().remove("synthetic_credentials") else {
        return row;
    };
    for (name, recipe) in recipes.as_object().unwrap() {
        let prefix = recipe["prefix"].as_str().unwrap();
        let unit = recipe["body_unit"].as_str().unwrap();
        let repeat = usize::try_from(recipe["repeat"].as_u64().unwrap()).unwrap();
        assert!(["sk_test_", "sk_live_", "rk_test_", "rk_live_"].contains(&prefix));
        assert!(!unit.is_empty() && unit.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert!((1..=1000).contains(&repeat));
        let credential = format!("{prefix}{}", unit.repeat(repeat));
        replace(&mut row, &format!("{{{{synthetic:{name}}}}}"), &credential);
    }
    row
}

fn replace(value: &mut Value, marker: &str, credential: &str) {
    match value {
        Value::String(text) => *text = text.replace(marker, credential),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| replace(value, marker, credential)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| replace(value, marker, credential)),
        _ => {}
    }
}
