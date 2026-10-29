//! Call Firefox extension APIs (`browser.*`) from Rust.

use js_sys::{Array, Function, Object, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// Calls `browser.<path>(...args)` and awaits the result,
/// e.g. `call("storage.local.get", &["level".into()])`.
pub async fn call(path: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let mut parent = JsValue::UNDEFINED;
    let mut target = Reflect::get(&js_sys::global(), &"browser".into()).map_err(message)?;
    for key in path.split('.') {
        parent = target;
        target = Reflect::get(&parent, &key.into()).map_err(message)?;
    }
    let function: Function = target
        .dyn_into()
        .map_err(|_| format!("browser.{path} is not a function"))?;
    let result = function
        .apply(&parent, &args.iter().collect::<Array>())
        .map_err(message)?;
    JsFuture::from(Promise::resolve(&result))
        .await
        .map_err(message)
}

/// Builds a plain JS object: `object(&[("cmd", "ping".into())])`.
pub fn object(entries: &[(&str, JsValue)]) -> JsValue {
    let object = Object::new();
    for (key, value) in entries {
        Reflect::set(&object, &(*key).into(), value).expect("setting a key on a fresh object");
    }
    object.into()
}

/// Reads a property, `undefined` if it is missing.
pub fn get(value: &JsValue, key: &str) -> JsValue {
    Reflect::get(value, &key.into()).unwrap_or(JsValue::UNDEFINED)
}

fn message(error: JsValue) -> String {
    get(&error, "message")
        .as_string()
        .or_else(|| error.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}
