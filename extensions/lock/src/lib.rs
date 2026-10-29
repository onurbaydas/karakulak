//! Lock screen page, shown by the compositor in a kiosk Firefox while the
//! session is locked. The password goes to karakulakd, which hands it to the
//! compositor; the compositor checks it with PAM and unlocks.

use std::time::Duration;

use karakulak_browser::{call, get, object};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
struct Reply {
    ok: bool,
    error: Option<String>,
}

async fn unlock(password: String) -> Result<(), String> {
    let reply = call(
        "runtime.sendNativeMessage",
        &[
            "karakulakd".into(),
            object(&[("cmd", "unlock".into()), ("password", password.into())]),
        ],
    )
    .await?;
    let reply: Reply = serde_wasm_bindgen::from_value(reply).map_err(|e| e.to_string())?;
    if reply.ok {
        Ok(())
    } else {
        Err(reply.error.unwrap_or_default())
    }
}

/// Nothing but this page may be open in the lock browser.
async fn close_other_tabs() {
    let Ok(me) = call("tabs.getCurrent", &[]).await else {
        return;
    };
    let my_id = get(&me, "id");
    let Ok(tabs) = call("tabs.query", &[object(&[])]).await else {
        return;
    };
    for tab in js_sys::Array::from(&tabs).iter() {
        let id = get(&tab, "id");
        if id != my_id {
            let _ = call("tabs.remove", &[id]).await;
        }
    }
}

fn clock() -> (String, String) {
    let now = js_sys::Date::new_0();
    let time = now.to_locale_time_string_with_options(
        "tr-TR",
        &object(&[("hour", "2-digit".into()), ("minute", "2-digit".into())]),
    );
    let date = now.to_locale_date_string(
        "tr-TR",
        &object(&[("weekday", "long".into()), ("day", "numeric".into()), ("month", "long".into())]),
    );
    (time.into(), date.into())
}

#[component]
fn Lock() -> impl IntoView {
    let (time, set_time) = signal(clock());
    let password = RwSignal::new(String::new());
    let (status, set_status) = signal(String::new());
    let (checking, set_checking) = signal(false);

    set_interval(
        move || {
            set_time.set(clock());
            spawn_local(close_other_tabs());
        },
        Duration::from_secs(1),
    );

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let entered = password.get();
        password.set(String::new());
        set_checking.set(true);
        set_status.set(String::new());
        spawn_local(async move {
            if let Err(e) = unlock(entered).await {
                set_status.set(if e == "wrong password" {
                    "Şifre yanlış.".into()
                } else {
                    format!("Hata: {e}")
                });
            }
            set_checking.set(false);
        });
    };

    view! {
        <main>
            <time>{move || time.get().0}</time>
            <p class="date">{move || time.get().1}</p>
            <form on:submit=submit>
                <input
                    type="password"
                    placeholder="Şifre"
                    autofocus
                    required
                    prop:disabled=checking
                    bind:value=password
                />
            </form>
            <p class="status" role="alert">{status}</p>
        </main>
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    mount_to_body(Lock);
}
