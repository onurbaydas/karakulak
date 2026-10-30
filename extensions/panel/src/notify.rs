//! Notifications for system events (from karakulakd's "watch" port) and
//! finished downloads. Runs in the background page (background.js).

use std::cell::RefCell;

use js_sys::Function;
use karakulak_browser::{call, get, object, t, turkish};
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue, prelude::*};

thread_local! {
    static PORT: RefCell<Option<JsValue>> = const { RefCell::new(None) };
}

fn notify(title: &str, body: &str) {
    let options = object(&[
        ("type", "basic".into()),
        ("title", title.into()),
        ("message", body.into()),
        ("iconUrl", "logo.svg".into()),
    ]);
    spawn_local(async move {
        let _ = call("notifications.create", &[options]).await;
    });
}

fn on_event(msg: JsValue) {
    match get(&msg, "event").as_string().as_deref() {
        Some("usb-added") => {
            let name = get(&msg, "name").as_string().filter(|n| !n.is_empty()).unwrap_or_else(|| "USB".into());
            notify(t("USB bellek takıldı", "USB stick connected"), &format!("{name} — {}", t("Dosyalar'da açabilirsin.", "Open it in Files.")));
        }
        Some("update-finished") => {
            if get(&msg, "ok").as_bool() == Some(true) {
                notify(t("Güncelleme bitti", "Update finished"), t("Sistem güncel.", "The system is up to date."));
            } else {
                notify(t("Güncelleme başarısız", "Update failed"), t("Daha sonra tekrar dene.", "Try again later."));
            }
        }
        Some("battery-low") => {
            let percent = get(&msg, "percent").as_f64().unwrap_or(0.0);
            let body = if turkish() {
                format!("Pil %{percent:.0}. Şarja tak.")
            } else {
                format!("Battery at {percent:.0}%. Plug in the charger.")
            };
            notify(t("Pil azaldı", "Battery low"), &body);
        }
        _ => {}
    }
}

/// Opens the watch port unless it is still open. Called at browser start
/// and every minute, so a suspended background page reconnects.
#[wasm_bindgen]
pub async fn start_watch() -> Result<(), JsValue> {
    if PORT.with_borrow(|p| p.is_some()) {
        return Ok(());
    }
    let port = call("runtime.connectNative", &["karakulakd".into()]).await?;
    let on_message = Closure::<dyn FnMut(JsValue)>::new(on_event);
    let on_disconnect = Closure::<dyn FnMut(JsValue)>::new(|_| PORT.with_borrow_mut(|p| *p = None));
    for (event, listener) in [("onMessage", &on_message), ("onDisconnect", &on_disconnect)] {
        let target = get(&port, event);
        if let Ok(add) = get(&target, "addListener").dyn_into::<Function>() {
            add.call1(&target, listener.as_ref())?;
        }
    }
    on_message.forget();
    on_disconnect.forget();
    if let Ok(post) = get(&port, "postMessage").dyn_into::<Function>() {
        post.call1(&port, &object(&[("cmd", "watch".into())]))?;
    }
    PORT.with_borrow_mut(|p| *p = Some(port));
    Ok(())
}

/// downloads.onChanged: tell when a download has finished.
#[wasm_bindgen]
pub async fn download_changed(delta: JsValue) -> Result<(), JsValue> {
    if get(&get(&delta, "state"), "current").as_string().as_deref() != Some("complete") {
        return Ok(());
    }
    let found = call("downloads.search", &[object(&[("id", get(&delta, "id"))])]).await?;
    let first = js_sys::Array::from(&found).get(0);
    let file = get(&first, "filename")
        .as_string()
        .and_then(|path| path.rsplit('/').next().map(String::from))
        .unwrap_or_default();
    notify(t("İndirme tamamlandı", "Download finished"), &file);
    Ok(())
}
