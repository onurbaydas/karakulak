//! Call Firefox extension APIs (`browser.*`) from Rust.

use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};

/// Calls `browser.<path>(...args)` and awaits the result,
/// e.g. `call("storage.local.get", &["level".into()])`.
pub async fn call(path: &str, args: &[JsValue]) -> Result<JsValue, String> {
    #[cfg(feature = "demo")]
    return demo_call(path, args);
    #[cfg(not(feature = "demo"))]
    return browser_call(path, args).await;
}

#[cfg(not(feature = "demo"))]
async fn browser_call(path: &str, args: &[JsValue]) -> Result<JsValue, String> {
    use js_sys::Promise;
    use wasm_bindgen_futures::JsFuture;
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

/// The demo website has no `browser.*`: pages open in the demo's screen, the
/// rest does nothing.
#[cfg(feature = "demo")]
fn demo_call(path: &str, args: &[JsValue]) -> Result<JsValue, String> {
    match path {
        "tabs.create" => {
            let url = args.first().map(|a| get(a, "url")).and_then(|u| u.as_string()).unwrap_or_default();
            // In the frame named "screen" of the demo page, like a new tab.
            let open: Function = get(&js_sys::global(), "open").dyn_into().map_err(|_| "no window.open")?;
            open.call2(&JsValue::NULL, &url.into(), &"screen".into()).map_err(message)?;
            Ok(JsValue::UNDEFINED)
        }
        "topSites.get" => Ok(Array::new().into()),
        "runtime.connectNative" => Err("demo".into()),
        _ => Ok(JsValue::UNDEFINED),
    }
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

/// The Turkish or the English text, by the browser's language.
pub fn t(tr: &'static str, en: &'static str) -> &'static str {
    if turkish() { tr } else { en }
}

pub fn turkish() -> bool {
    thread_local! {
        static TURKISH: bool = get(&get(&js_sys::global(), "navigator"), "language")
            .as_string()
            .is_some_and(|l| l.starts_with("tr"));
    }
    TURKISH.with(|t| *t)
}

/// Locale for dates and times, matching `t`.
pub fn locale() -> &'static str {
    t("tr-TR", "en-GB")
}
