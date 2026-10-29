//! Settings page (settings.html): screens, region and language, privacy,
//! updates, firewall, printers.

use leptos::prelude::*;
use serde::Deserialize;

use karakulak_browser::{call, object};
use leptos::task::spawn_local;

use crate::run;

#[derive(Clone, Deserialize)]
struct Screen {
    id: String,
    name: String,
    current: Option<(i32, i32, i32)>,
    scale: i32,
    modes: Vec<(i32, i32, i32)>,
}

fn mode_text((w, h, mhz): (i32, i32, i32)) -> String {
    format!("{w} × {h}, {} Hz", (mhz as f64 / 1000.0).round())
}

#[component]
fn ScreenSettings(screen: Screen, screens: RwSignal<Vec<Screen>>, on_error: WriteSignal<String>) -> impl IntoView {
    let id = screen.id.clone();
    let apply = move |mode: (i32, i32, i32), scale: i32| {
        let args = vec![
            ("id", id.clone().into()),
            ("width", mode.0.into()),
            ("height", mode.1.into()),
            ("refresh", mode.2.into()),
            ("scale", scale.into()),
        ];
        run::<Vec<Screen>>("set-screen", args, on_error, move |s| screens.set(s.unwrap_or_default()));
    };
    let current = screen.current.unwrap_or_default();
    let scale = screen.scale;
    let modes = screen.modes.clone();
    let apply_mode = apply.clone();

    let options = screen
        .modes
        .iter()
        .enumerate()
        .map(|(i, mode)| {
            view! { <option value=i.to_string() selected=*mode == current>{mode_text(*mode)}</option> }
        })
        .collect_view();

    view! {
        <section class="card">
            <h2>{screen.name}</h2>
            <label>
                "Çözünürlük"
                <select on:change=move |ev| {
                    if let Some(mode) = event_target_value(&ev).parse::<usize>().ok().and_then(|i| modes.get(i)) {
                        apply_mode(*mode, scale);
                    }
                }>{options}</select>
            </label>
            <label>
                "Ölçek"
                <select on:change=move |ev| {
                    if let Ok(scale) = event_target_value(&ev).parse::<i32>() {
                        apply(current, scale);
                    }
                }>
                    <option value="1" selected=scale == 1>"%100"</option>
                    <option value="2" selected=scale == 2>"%200 (yüksek çözünürlüklü ekranlar)"</option>
                </select>
            </label>
        </section>
    }
}

#[derive(Clone, Deserialize)]
struct Regional {
    language: String,
    layout: String,
    variant: String,
    timezone: String,
    timezones: Vec<String>,
}

#[derive(Clone, Deserialize)]
struct Updates {
    pending: u32,
    state: String,
}

#[derive(Clone, Deserialize)]
struct Firewall {
    on: bool,
}

const LANGUAGES: [(&str, &str); 2] = [("tr_TR.UTF-8", "Türkçe"), ("en_US.UTF-8", "English")];
const KEYBOARDS: [(&str, &str, &str); 6] = [
    ("tr", "", "Türkçe Q"),
    ("tr", "f", "Türkçe F"),
    ("us", "", "English (US)"),
    ("gb", "", "English (UK)"),
    ("de", "", "Deutsch"),
    ("fr", "", "Français"),
];
#[derive(Clone, Deserialize)]
pub struct Vpn {
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// Reads the chosen file as text: `(file name, contents)`.
async fn read_chosen_file(ev: &leptos::ev::Event) -> Option<(String, String)> {
    use wasm_bindgen::JsCast;
    let input = ev.target()?;
    let file = js_sys::Reflect::get(&karakulak_browser::get(&input, "files"), &0.into()).ok()?;
    let name = karakulak_browser::get(&file, "name").as_string()?;
    let text: js_sys::Function = karakulak_browser::get(&file, "text").dyn_into().ok()?;
    let promise: js_sys::Promise = text.call0(&file).ok()?.dyn_into().ok()?;
    let contents = wasm_bindgen_futures::JsFuture::from(promise).await.ok()?.as_string()?;
    Some((name, contents))
}

#[component]
fn VpnSettings(on_error: WriteSignal<String>) -> impl IntoView {
    let vpns = RwSignal::new(Vec::<Vpn>::new());
    let update = move |v: Option<Vec<Vpn>>| vpns.set(v.unwrap_or_default());
    run::<Vec<Vpn>>("vpn-list", vec![], on_error, update);
    let add = move |ev: leptos::ev::Event| {
        spawn_local(async move {
            if let Some((name, config)) = read_chosen_file(&ev).await {
                run::<Vec<Vpn>>("vpn-import", vec![("name", name.into()), ("config", config.into())], on_error, update);
            }
        });
    };
    view! {
        <section class="card">
            {move || {
                vpns.get()
                    .into_iter()
                    .map(|vpn| {
                        let id = vpn.id.clone();
                        view! {
                            <label>
                                <span>{vpn.name} <small class="hint">{if vpn.active { "Bağlı" } else { "Kapalı" }}</small></span>
                                <button class="danger" on:click={move |_| run::<Vec<Vpn>>("vpn-remove", vec![("id", id.clone().into())], on_error, update)}>
                                    "Sil"
                                </button>
                            </label>
                        }
                    })
                    .collect_view()
            }}
            <label>
                "WireGuard dosyası ekle"
                <input type="file" accept=".conf" on:change=add />
            </label>
            <p class="hint">"VPN sağlayıcının (Mullvad, Proton...) verdiği .conf dosyasını seç. Panelden açıp kapatabilirsin."</p>
        </section>
    }
}

#[derive(Clone, Deserialize)]
struct ScreenBrightness {
    id: String,
    name: String,
    method: String,
    software: bool,
}

#[component]
fn BrightnessSettings(on_error: WriteSignal<String>) -> impl IntoView {
    let screens = RwSignal::new(Vec::<ScreenBrightness>::new());
    let update = move |s: Option<Vec<ScreenBrightness>>| screens.set(s.unwrap_or_default());
    run::<Vec<ScreenBrightness>>("brightness", vec![], on_error, update);
    move || {
        screens
            .get()
            .into_iter()
            .map(|screen| {
                let how = match screen.method.as_str() {
                    "backlight" => "ekran ışığıyla",
                    "ddc" => "monitörün kendisiyle (DDC/CI)",
                    _ => "yazılımla",
                };
                let id = screen.id.clone();
                let software = screen.software;
                view! {
                    <section class="card">
                        <label>
                            <span>{format!("{}: yazılımla karart", screen.name)} <small class="hint">{format!("Şu an {how} ayarlanıyor.")}</small></span>
                            <input
                                type="checkbox"
                                prop:checked=software
                                on:change={move |_| {
                                    let args = vec![("id", id.clone().into()), ("on", (!software).into())];
                                    run::<Vec<ScreenBrightness>>("set-software-brightness", args, on_error, update)
                                }}
                            />
                        </label>
                    </section>
                }
            })
            .collect_view()
    }
}

#[derive(Clone, Deserialize)]
struct Night {
    mode: String,
}

const NIGHT: [(&str, &str); 3] = [("off", "Kapalı"), ("evening", "Akşamları (20:00 - 07:00)"), ("on", "Her zaman")];

/// Recommended extensions: privacy-respecting, from addons.mozilla.org.
const EXTENSIONS: [(&str, &str, &str); 5] = [
    ("uBlock Origin", "Reklam ve izleyici engelleyici", "https://addons.mozilla.org/firefox/addon/ublock-origin/"),
    ("Bitwarden", "Açık kaynak şifre yöneticisi", "https://addons.mozilla.org/firefox/addon/bitwarden-password-manager/"),
    ("Firefox Multi-Account Containers", "Siteleri birbirinden ayrı kutularda aç", "https://addons.mozilla.org/firefox/addon/multi-account-containers/"),
    ("Dark Reader", "Her siteyi koyu temada göster", "https://addons.mozilla.org/firefox/addon/darkreader/"),
    ("SponsorBlock", "YouTube videolarındaki sponsor kısımlarını atla", "https://addons.mozilla.org/firefox/addon/sponsorblock/"),
];

const PRIVACY: [(&str, &str); 3] = [("standard", "Standart"), ("strict", "Sıkı"), ("paranoid", "Paranoyak")];

#[component]
fn RegionSettings(on_error: WriteSignal<String>) -> impl IntoView {
    let regional = RwSignal::new(None::<Regional>);
    let update = move |r: Option<Regional>| regional.set(r);
    run::<Regional>("regional", vec![], on_error, update);

    move || {
        regional.get().map(|r| {
            let keyboard = KEYBOARDS.iter().position(|(l, v, _)| *l == r.layout && *v == r.variant);
            view! {
                <section class="card">
                    <label>
                        "Dil"
                        <select on:change=move |ev| {
                            let args = vec![("language", event_target_value(&ev).into())];
                            run::<Regional>("set-language", args, on_error, update);
                        }>
                            {LANGUAGES.map(|(code, name)| view! { <option value=code selected=r.language == code>{name}</option> })}
                        </select>
                    </label>
                    <p class="hint">"Dil değişikliği bir sonraki girişte uygulanır."</p>
                    <label>
                        "Klavye"
                        <select on:change=move |ev| {
                            if let Some((layout, variant, _)) = event_target_value(&ev).parse::<usize>().ok().and_then(|i| KEYBOARDS.get(i)) {
                                let args = vec![("layout", (*layout).into()), ("variant", (*variant).into())];
                                run::<Regional>("set-keyboard", args, on_error, update);
                            }
                        }>
                            {KEYBOARDS
                                .iter()
                                .enumerate()
                                .map(|(i, (_, _, name))| view! { <option value=i.to_string() selected=keyboard == Some(i)>{*name}</option> })
                                .collect_view()}
                        </select>
                    </label>
                    <label>
                        "Saat dilimi"
                        <select on:change=move |ev| {
                            let args = vec![("timezone", event_target_value(&ev).into())];
                            run::<Regional>("set-timezone", args, on_error, update);
                        }>
                            {r.timezones
                                .iter()
                                .map(|zone| view! { <option value=zone.clone() selected=*zone == r.timezone>{zone.clone()}</option> })
                                .collect_view()}
                        </select>
                    </label>
                </section>
            }
        })
    }
}

/// The level lives in the welcome extension, which applies it.
async fn privacy_message(message: wasm_bindgen::JsValue) -> Result<wasm_bindgen::JsValue, String> {
    call("runtime.sendMessage", &["welcome@karakulak".into(), message]).await
}

#[component]
fn PrivacySettings(on_error: WriteSignal<String>) -> impl IntoView {
    let level = RwSignal::new(None::<String>);
    spawn_local(async move {
        if let Ok(current) = privacy_message(object(&[("get", true.into())])).await {
            level.set(current.as_string());
        }
    });
    let change = move |ev: leptos::ev::Event| {
        let chosen = event_target_value(&ev);
        spawn_local(async move {
            match privacy_message(object(&[("level", chosen.clone().into())])).await {
                Ok(_) => level.set(Some(chosen)),
                Err(e) => on_error.set(format!("Hata: {e}")),
            }
        });
    };
    view! {
        <section class="card">
            <label>
                "Gizlilik seviyesi"
                <select on:change=change>
                    {PRIVACY.map(|(code, name)| {
                        view! { <option value=code selected=move || level.get().as_deref() == Some(code)>{name}</option> }
                    })}
                </select>
            </label>
            <p class="hint">"Telemetri her seviyede kapalıdır."</p>
        </section>
    }
}

#[component]
fn SystemSettings(on_error: WriteSignal<String>) -> impl IntoView {
    let updates = RwSignal::new(None::<Updates>);
    let firewall = RwSignal::new(None::<Firewall>);
    let refresh = move || run::<Updates>("updates", vec![], on_error, move |u| updates.set(u));
    refresh();
    run::<Firewall>("firewall", vec![], on_error, move |f| firewall.set(f));

    // While an update runs, check every few seconds.
    set_interval(
        move || {
            if updates.get_untracked().is_some_and(|u| u.state == "activating") {
                refresh();
            }
        },
        std::time::Duration::from_secs(3),
    );

    let update_text = move || match updates.get() {
        Some(u) if u.state == "activating" => "Güncelleniyor…".to_string(),
        Some(u) if u.state == "failed" => "Son güncelleme başarısız oldu.".to_string(),
        Some(u) if u.pending == 0 => "Sistem güncel.".to_string(),
        Some(u) => format!("{} güncelleme bekliyor.", u.pending),
        None => String::new(),
    };

    view! {
        <section class="card">
            <label>
                <span>"Güncellemeler" <small class="hint">{update_text}</small></span>
                <button
                    prop:disabled=move || updates.get().is_some_and(|u| u.state == "activating")
                    on:click={move |_| run::<Updates>("update-now", vec![], on_error, move |u| updates.set(u))}
                >
                    "Şimdi güncelle"
                </button>
            </label>
            <p class="hint">"Güvenlik güncellemeleri her gün kendiliğinden kurulur."</p>
        </section>
        <section class="card">
            <label>
                "Güvenlik duvarı"
                <input
                    type="checkbox"
                    prop:checked=move || firewall.get().is_some_and(|f| f.on)
                    on:change=move |_| {
                        let on = !firewall.get_untracked().is_some_and(|f| f.on);
                        run::<Firewall>("set-firewall", vec![("on", on.into())], on_error, move |f| firewall.set(f));
                    }
                />
            </label>
            <p class="hint">"Açıkken dışarıdan bu bilgisayara bağlantı kurulamaz. Kapatmak yeniden başlatana kadar geçerlidir."</p>
        </section>
        <section class="card">
            <label>
                "Yazıcılar"
                <button on:click=move |_| {
                    spawn_local(async move {
                        let _ = call("tabs.create", &[object(&[("url", "http://localhost:631/".into())])]).await;
                    });
                }>"Yazıcı ayarlarını aç"</button>
            </label>
            <p class="hint">"Ağdaki ve USB'deki çoğu yazıcı sürücü kurmadan çalışır."</p>
        </section>
    }
}

#[component]
fn NightLight(on_error: WriteSignal<String>) -> impl IntoView {
    let mode = RwSignal::new(None::<String>);
    run::<Night>("night-light", vec![], on_error, move |n| mode.set(n.map(|n| n.mode)));
    view! {
        <section class="card">
            <label>
                "Gece ışığı"
                <select on:change=move |ev| {
                    let args = vec![("mode", event_target_value(&ev).into())];
                    run::<Night>("night-light", args, on_error, move |n| mode.set(n.map(|n| n.mode)));
                }>
                    {NIGHT.map(|(code, name)| {
                        view! { <option value=code selected=move || mode.get().as_deref() == Some(code)>{name}</option> }
                    })}
                </select>
            </label>
            <p class="hint">"Ekranı sarıya kaydırır, akşamları gözü daha az yorar."</p>
        </section>
    }
}

#[component]
fn Extensions() -> impl IntoView {
    view! {
        <section class="card">
            {EXTENSIONS.map(|(name, about, url)| {
                view! {
                    <label>
                        <span>{name} <small class="hint">{about}</small></span>
                        <button on:click=move |_| {
                            spawn_local(async move {
                                let _ = call("tabs.create", &[object(&[("url", url.into())])]).await;
                            });
                        }>"Firefox'a ekle"</button>
                    </label>
                }
            })}
            <p class="hint">"Mozilla'nın eklenti mağazasında açılır, kurmak için orada \"Firefox'a ekle\"ye bas."</p>
        </section>
    }
}

#[component]
pub fn Settings() -> impl IntoView {
    let (error, set_error) = signal(String::new());
    let screens = RwSignal::new(Vec::<Screen>::new());
    run::<Vec<Screen>>("screens", vec![], set_error, move |s| screens.set(s.unwrap_or_default()));

    view! {
        <main class="settings">
            <h1>"Ayarlar"</h1>
            <p class="error" role="alert">{error}</p>
            <h2 class="group">"Ekranlar"</h2>
            {move || {
                screens
                    .get()
                    .into_iter()
                    .map(|screen| view! { <ScreenSettings screen screens on_error=set_error /> })
                    .collect_view()
            }}
            <BrightnessSettings on_error=set_error />
            <p class="hint">"Parlaklık tuşları ekranı değiştirmiyorsa \"yazılımla karart\"ı aç."</p>
            <NightLight on_error=set_error />
            <h2 class="group">"Bölge ve dil"</h2>
            <RegionSettings on_error=set_error />
            <h2 class="group">"Gizlilik"</h2>
            <PrivacySettings on_error=set_error />
            <h2 class="group">"VPN"</h2>
            <VpnSettings on_error=set_error />
            <h2 class="group">"Sistem"</h2>
            <SystemSettings on_error=set_error />
            <h2 class="group">"Önerilen eklentiler"</h2>
            <Extensions />
        </main>
    }
}
