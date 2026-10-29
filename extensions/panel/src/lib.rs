//! Toolbar popup: clock, battery and power buttons. System actions go
//! through `karakulakd` (see daemon/).

use std::time::Duration;

mod files;
mod newtab;
mod settings;
mod terminal;

use karakulak_browser::{call, object};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
struct Reply<T> {
    ok: bool,
    result: Option<T>,
    error: Option<String>,
}

#[derive(Clone, Deserialize)]
struct Level {
    level: f64,
    #[serde(default)]
    muted: bool,
}

#[derive(Clone, Deserialize)]
struct Screen {
    id: String,
    name: String,
    level: f64,
}

#[derive(Clone, Deserialize)]
struct Network {
    ssid: String,
    signal: u8,
    secure: bool,
    active: bool,
    known: bool,
}

#[derive(Clone, Deserialize)]
struct Bluetooth {
    powered: bool,
    devices: Vec<BluetoothDevice>,
}

#[derive(Clone, Deserialize)]
struct FoundDevice {
    path: String,
    name: String,
}

#[derive(Clone, Deserialize)]
struct BluetoothDevice {
    path: String,
    name: String,
    connected: bool,
}

#[derive(Deserialize)]
struct Battery {
    capacity: Option<u8>,
    status: Option<String>,
}

/// Sends a command to karakulakd and returns its result.
async fn send<T: DeserializeOwned>(cmd: &str, args: &[(&str, JsValue)]) -> Result<Option<T>, String> {
    let mut message = vec![("cmd", JsValue::from(cmd))];
    message.extend(args.iter().cloned());
    let reply = call("runtime.sendNativeMessage", &["karakulakd".into(), object(&message)]).await?;
    let reply: Reply<T> = serde_wasm_bindgen::from_value(reply).map_err(|e| e.to_string())?;
    if reply.ok {
        Ok(reply.result)
    } else {
        Err(reply.error.unwrap_or_default())
    }
}

/// Runs a command in the background; the result goes to `done`, errors to `on_error`.
fn run<T: DeserializeOwned + 'static>(
    cmd: &'static str,
    args: Vec<(&'static str, JsValue)>,
    on_error: WriteSignal<String>,
    done: impl FnOnce(Option<T>) + 'static,
) {
    spawn_local(async move {
        match send::<T>(cmd, &args).await {
            Ok(result) => done(result),
            Err(error) => on_error.set(format!("Hata: {error}")),
        }
    });
}

fn battery_text(battery: &Battery) -> String {
    let status = match battery.status.as_deref() {
        Some("Charging") => "şarj oluyor",
        Some("Discharging") => "pilde",
        Some("Full") => "dolu",
        Some("Not charging") => "şarj olmuyor",
        Some(other) => other,
        None => "",
    };
    match battery.capacity {
        Some(capacity) => format!("Pil %{capacity}, {status}"),
        None => format!("Pil {status}"),
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
        &object(&[
            ("weekday", "long".into()),
            ("day", "numeric".into()),
            ("month", "long".into()),
        ]),
    );
    (time.into(), date.into())
}

/// Power buttons need a second click to confirm.
#[component]
fn PowerButton(label: &'static str, cmd: &'static str, on_error: WriteSignal<String>) -> impl IntoView {
    let (armed, set_armed) = signal(false);
    let click = move |_| {
        if !armed.get() {
            set_armed.set(true);
            return;
        }
        set_armed.set(false);
        run::<()>(cmd, vec![], on_error, |_| {});
    };
    view! {
        <button class:confirm=armed on:click=click>
            {move || if armed.get() { format!("{label}: emin misin?") } else { label.to_string() }}
        </button>
    }
}

/// A 0-100 slider for a 0.0-1.0 level.
fn percent(level: f64) -> String {
    ((level * 100.0).round() as u32).to_string()
}

fn slider_level(ev: &leptos::ev::Event) -> JsValue {
    (event_target_value(ev).parse::<f64>().unwrap_or(0.0) / 100.0).into()
}

#[component]
fn Volume(on_error: WriteSignal<String>) -> impl IntoView {
    let volume = RwSignal::new(None::<Level>);
    run::<Level>("volume", vec![], on_error, move |v| volume.set(v));

    move || {
        volume.get().map(|v| {
            view! {
                <section class="row">
                    <button
                        class="icon"
                        title="Sessize al"
                        on:click={move |_| run::<Level>("toggle-mute", vec![], on_error, move |v| volume.set(v))}
                    >
                        {if v.muted { "🔇" } else { "🔊" }}
                    </button>
                    <input
                        type="range"
                        min="0"
                        max="100"
                        aria-label="Ses"
                        prop:value=percent(v.level)
                        on:change=move |ev| {
                            let level = slider_level(&ev);
                            run::<Level>("set-volume", vec![("level", level)], on_error, move |v| volume.set(v))
                        }
                    />
                </section>
            }
        })
    }
}

#[component]
fn Brightness(on_error: WriteSignal<String>) -> impl IntoView {
    let screens = RwSignal::new(Vec::<Screen>::new());
    let update = move |s: Option<Vec<Screen>>| screens.set(s.unwrap_or_default());
    run::<Vec<Screen>>("brightness", vec![], on_error, update);

    move || {
        screens
            .get()
            .into_iter()
            .map(|screen| {
                let id = screen.id.clone();
                view! {
                    <section class="row">
                        <span class="icon" title=screen.name.clone()>"☀"</span>
                        <label class="screen">
                            <small>{screen.name.clone()}</small>
                            <input
                                type="range"
                                min="0"
                                max="100"
                                aria-label=format!("Parlaklık: {}", screen.name)
                                prop:value=percent(screen.level)
                                on:change=move |ev| {
                                    let args = vec![("id", id.clone().into()), ("level", slider_level(&ev))];
                                    run::<Vec<Screen>>("set-brightness", args, on_error, update)
                                }
                            />
                        </label>
                    </section>
                }
            })
            .collect_view()
    }
}

#[component]
fn Wifi(on_error: WriteSignal<String>) -> impl IntoView {
    let networks = RwSignal::new(Vec::<Network>::new());
    let selected = RwSignal::new(None::<String>);
    let password = RwSignal::new(String::new());
    let connecting = RwSignal::new(None::<String>);
    let failed = RwSignal::new(false);
    let refresh = move || run::<Vec<Network>>("wifi-list", vec![], on_error, move |n| networks.set(n.unwrap_or_default()));
    refresh();

    // Didn't connect: ask for the password, which replaces the saved one.
    let ask_password = move |ssid: String| {
        connecting.set(None);
        failed.set(true);
        selected.set(Some(ssid));
    };

    let connect = move |ssid: String| {
        let args = vec![("ssid", ssid.clone().into()), ("password", password.get().into())];
        password.set(String::new());
        selected.set(None);
        failed.set(false);
        connecting.set(Some(ssid.clone()));
        spawn_local(async move {
            if send::<()>("wifi-connect", &args).await.is_err() {
                ask_password(ssid);
                return;
            }
            // NetworkManager connects in the background; check after a moment.
            set_timeout(
                move || {
                    run::<Vec<Network>>("wifi-list", vec![], on_error, move |n| {
                        let n = n.unwrap_or_default();
                        let connected = n.iter().any(|net| net.ssid == ssid && net.active);
                        networks.set(n);
                        if connected {
                            connecting.set(None);
                        } else {
                            ask_password(ssid);
                        }
                    })
                },
                Duration::from_secs(6),
            );
        });
    };

    view! {
        <details>
            <summary>"Wi-Fi"</summary>
            <ul class="list">
                // Key on the state too, so a row redraws when it connects.
                <For each=move || networks.get() key=|n| (n.ssid.clone(), n.active, n.known) let:network>
                    {
                        let ssid = network.ssid.clone();
                        let is_selected = {
                            let ssid = ssid.clone();
                            move || selected.get().as_deref() == Some(ssid.as_str())
                        };
                        let status = {
                            let ssid = ssid.clone();
                            move || {
                                if network.active {
                                    "Bağlı ✓".to_string()
                                } else if connecting.get().as_deref() == Some(ssid.as_str()) {
                                    "Bağlanıyor…".to_string()
                                } else {
                                    format!("%{}{}", network.signal, if network.secure { " 🔒" } else { "" })
                                }
                            }
                        };
                        let pick = {
                            let ssid = ssid.clone();
                            move |_| {
                                if network.active {
                                    return;
                                }
                                if network.secure && !network.known {
                                    selected.set(Some(ssid.clone()));
                                } else {
                                    connect(ssid.clone());
                                }
                            }
                        };
                        view! {
                            <li>
                                <button class:active=network.active on:click=pick>
                                    <span>{network.ssid.clone()}</span>
                                    <small>{status}</small>
                                </button>
                                <Show when=is_selected>
                                    <Show when=move || failed.get()>
                                        <p class="hint">"Bağlanamadı, şifreyi gir."</p>
                                    </Show>
                                    <form on:submit={
                                        let ssid = ssid.clone();
                                        move |ev| {
                                            ev.prevent_default();
                                            connect(ssid.clone());
                                        }
                                    }>
                                        <input type="password" placeholder="Şifre" required bind:value=password />
                                        <button type="submit">"Bağlan"</button>
                                    </form>
                                </Show>
                            </li>
                        }
                    }
                </For>
            </ul>
        </details>
    }
}

#[component]
fn BluetoothSection(on_error: WriteSignal<String>) -> impl IntoView {
    let bluetooth = RwSignal::new(None::<Bluetooth>);
    let found = RwSignal::new(Vec::<FoundDevice>::new());
    let scanning = RwSignal::new(false);
    let pairing = RwSignal::new(None::<String>);
    let update = move |b| bluetooth.set(b);
    run::<Bluetooth>("bluetooth", vec![], on_error, update);

    let scan = move |_| {
        scanning.set(true);
        found.set(Vec::new());
        run::<Vec<FoundDevice>>("bluetooth-scan", vec![], on_error, move |f| {
            found.set(f.unwrap_or_default());
            scanning.set(false);
        });
    };

    let paired_devices = move || {
        bluetooth.get().map(|b| {
            b.devices
                .into_iter()
                .map(|device| {
                    let (cmd, label) = if device.connected {
                        ("bluetooth-disconnect", "Bağlantıyı kes")
                    } else {
                        ("bluetooth-connect", "Bağlan")
                    };
                    let path = device.path.clone();
                    view! {
                        <li>
                            <button
                                class:active=device.connected
                                on:click={move |_| run::<Bluetooth>(cmd, vec![("path", path.clone().into())], on_error, update)}
                            >
                                <span>{device.name}</span>
                                <small>{label}</small>
                            </button>
                        </li>
                    }
                })
                .collect_view()
        })
    };

    let new_devices = move || {
        found
            .get()
            .into_iter()
            .map(|device| {
                let path = device.path.clone();
                let is_pairing = {
                    let path = path.clone();
                    move || pairing.get().as_deref() == Some(path.as_str())
                };
                let pair = move |_| {
                    pairing.set(Some(path.clone()));
                    let done_path = path.clone();
                    run::<Bluetooth>("bluetooth-pair", vec![("path", path.clone().into())], on_error, move |b| {
                        update(b);
                        found.update(|f| f.retain(|d| d.path != done_path));
                        pairing.set(None);
                    });
                };
                view! {
                    <li>
                        <button on:click=pair>
                            <span>{device.name}</span>
                            <small>{move || if is_pairing() { "Eşleştiriliyor…" } else { "Eşleştir" }}</small>
                        </button>
                    </li>
                }
            })
            .collect_view()
    };

    view! {
        <Show when=move || bluetooth.get().is_some()>
            <details>
                <summary>"Bluetooth"</summary>
                <label class="row">
                    <input
                        type="checkbox"
                        prop:checked=move || bluetooth.get().is_some_and(|b| b.powered)
                        on:change={move |_| {
                            let on = !bluetooth.get().is_some_and(|b| b.powered);
                            run::<Bluetooth>("bluetooth-power", vec![("on", on.into())], on_error, update)
                        }}
                    />
                    "Açık"
                </label>
                <ul class="list">{paired_devices}</ul>
                <button class="scan" prop:disabled=move || scanning.get() on:click=scan>
                    {move || if scanning.get() { "Aranıyor…" } else { "Cihaz ara" }}
                </button>
                <ul class="list">{new_devices}</ul>
            </details>
        </Show>
    }
}

#[component]
fn Panel() -> impl IntoView {
    let (time, set_time) = signal(clock());
    let (battery, set_battery) = signal(String::new());
    let (error, set_error) = signal(String::new());

    set_interval(move || set_time.set(clock()), Duration::from_secs(1));
    spawn_local(async move {
        match send::<Battery>("battery", &[]).await {
            Ok(Some(b)) => set_battery.set(battery_text(&b)),
            Ok(None) => {}
            Err(e) => set_error.set(format!("Hata: {e}")),
        }
    });

    view! {
        <time id="clock">{move || time.get().0}</time>
        <p id="date">{move || time.get().1}</p>
        <p id="battery">{battery}</p>
        <Volume on_error=set_error />
        <Brightness on_error=set_error />
        <Wifi on_error=set_error />
        <BluetoothSection on_error=set_error />
        <div class="power">
            <button on:click={move |_| {
                spawn_local(async move {
                    let _ = call("tabs.create", &[object(&[("url", "files.html".into())])]).await;
                });
            }}>"Dosyalar"</button>
            <button on:click={move |_| {
                spawn_local(async move {
                    let _ = call("tabs.create", &[object(&[("url", "terminal.html".into())])]).await;
                });
            }}>"Terminal"</button>
            <button on:click={move |_| {
                spawn_local(async move {
                    let _ = call("tabs.create", &[object(&[("url", "settings.html".into())])]).await;
                });
            }}>"Ayarlar"</button>
            <button on:click={move |_| run::<()>("lock", vec![], set_error, |_| {})}>"Kilitle"</button>
            <PowerButton label="Uyku" cmd="suspend" on_error=set_error />
            <PowerButton label="Yeniden başlat" cmd="reboot" on_error=set_error />
            <PowerButton label="Kapat" cmd="poweroff" on_error=set_error />
        </div>
        <p id="error" role="alert">{error}</p>
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    // One module for the popup and the pages it opens.
    let path = window().location().pathname().unwrap_or_default();
    if path.ends_with("/files.html") {
        mount_to_body(files::Files);
    } else if path.ends_with("/newtab.html") {
        mount_to_body(newtab::NewTab);
    } else if path.ends_with("/settings.html") {
        mount_to_body(settings::Settings);
    } else if path.ends_with("/terminal.html") {
        mount_to_body(terminal::Terminal);
    } else {
        mount_to_body(Panel);
    }
}
