//! First-run welcome page. The user picks a privacy level, which is applied
//! through `browser.privacy`. Telemetry is off at every level via policy.

use karakulak_browser::{call, get, object, t};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

/// A setting left as `None` goes back to the Firefox default.
#[derive(Default)]
struct Level {
    tracking_protection: Option<&'static str>,
    cookies: Option<&'static str>,
    resist_fingerprinting: Option<bool>,
    webrtc: Option<bool>,
    password_saving: Option<bool>,
}

fn level(name: &str) -> Level {
    let strict = Level {
        tracking_protection: Some("always"),
        cookies: Some("reject_trackers_and_partition_foreign"),
        ..Level::default()
    };
    match name {
        "strict" => strict,
        "paranoid" => Level {
            resist_fingerprinting: Some(true),
            webrtc: Some(false),
            password_saving: Some(false),
            ..strict
        },
        _ => Level::default(),
    }
}

async fn set(setting: &str, value: Option<JsValue>) -> Result<(), String> {
    match value {
        Some(value) => call(&format!("privacy.{setting}.set"), &[object(&[("value", value)])]).await?,
        None => call(&format!("privacy.{setting}.clear"), &[object(&[])]).await?,
    };
    Ok(())
}

async fn apply(name: &'static str) -> Result<(), String> {
    let l = level(name);
    set("websites.trackingProtectionMode", l.tracking_protection.map(Into::into)).await?;
    set(
        "websites.cookieConfig",
        l.cookies.map(|b| object(&[("behavior", b.into())])),
    )
    .await?;
    set("websites.resistFingerprinting", l.resist_fingerprinting.map(Into::into)).await?;
    set("network.peerConnectionEnabled", l.webrtc.map(Into::into)).await?;
    set("services.passwordSavingEnabled", l.password_saving.map(Into::into)).await?;
    call("storage.local.set", &[object(&[("level", name.into())])]).await?;
    Ok(())
}

async fn chosen_level() -> Option<String> {
    let stored = call("storage.local.get", &["level".into()]).await.ok()?;
    get(&stored, "level").as_string()
}

/// Settings (panel extension) changes the level through background.js.
#[wasm_bindgen]
pub async fn set_level(name: String) -> Result<(), JsValue> {
    let name: &'static str = match name.as_str() {
        "standard" => "standard",
        "strict" => "strict",
        "paranoid" => "paranoid",
        _ => return Err("unknown level".into()),
    };
    apply(name).await.map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub async fn get_level() -> JsValue {
    chosen_level().await.map(JsValue::from).unwrap_or(JsValue::NULL)
}

/// Called by background.js on install and on every browser start.
#[wasm_bindgen]
pub async fn open_welcome_if_needed() -> Result<(), JsValue> {
    if chosen_level().await.is_none() {
        call("tabs.create", &[object(&[("url", "welcome.html".into())])]).await?;
    }
    Ok(())
}

/// (code, Turkish title, English title, Turkish text, English text)
const LEVELS: [(&str, &str, &str, &str, &str); 3] = [
    (
        "standard",
        "Standart",
        "Standard",
        "Firefox'un kendi ayarları. Her site sorunsuz çalışır.",
        "Firefox's own settings. Every site works.",
    ),
    (
        "strict",
        "Sıkı",
        "Strict",
        "İzleyiciler her yerde engellenir, çerezler sitelere göre ayrılır.",
        "Trackers are blocked everywhere, cookies are kept apart per site.",
    ),
    (
        "paranoid",
        "Paranoyak",
        "Paranoid",
        "Parmak izine karşı koruma açılır, WebRTC ve şifre kaydetme kapanır. Bazı siteler bozulabilir.",
        "Fingerprinting protection on, WebRTC and password saving off. Some sites may break.",
    ),
];

#[component]
fn Welcome() -> impl IntoView {
    let (chosen, set_chosen) = signal(None::<String>);
    let (status, set_status) = signal(String::new());
    spawn_local(async move { set_chosen.set(chosen_level().await) });

    let buttons = LEVELS.map(|(name, title_tr, title_en, text_tr, text_en)| {
        let (title, text) = (t(title_tr, title_en), t(text_tr, text_en));
        let click = move |_| {
            spawn_local(async move {
                match apply(name).await {
                    Ok(()) => {
                        set_chosen.set(Some(name.to_string()));
                        set_status.set("Kaydedildi. Bu sekmeyi kapatabilirsin.".into());
                    }
                    Err(e) => set_status.set(format!("{}: {e}", t("Ayar uygulanamadı", "Could not apply the setting"))),
                }
            })
        };
        view! {
            <button
                aria-pressed=move || (chosen.get().as_deref() == Some(name)).to_string()
                on:click=click
            >
                <strong>{title}</strong>
                <span>{text}</span>
            </button>
        }
    });

    view! {
        <main>
            <img class="logo" src="logo.svg" alt="" />
            <h1>{t("Karakulak'a hoş geldin", "Welcome to Karakulak")}</h1>
            <p>{t("Gizlilik seviyeni seç. Sonra istediğin zaman değiştirebilirsin.", "Choose your privacy level. You can change it any time in Settings.")}</p>
            {buttons}
            <p id="status" role="status">{status}</p>
        </main>
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    // The same module also loads in the background page; only mount on the page.
    let path = window().location().pathname().unwrap_or_default();
    if path.ends_with("/welcome.html") {
        mount_to_body(Welcome);
    }
}
