//! Toolbar popup: clock, battery and power buttons. System actions go
//! through `karakulakd` (see daemon/).

use std::time::Duration;

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
struct Network {
    ssid: String,
    signal: u8,
    secure: bool,
    active: bool,
}

#[derive(Clone, Deserialize)]
struct Bluetooth {
    powered: bool,
    devices: Vec<BluetoothDevice>,
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
    let brightness = RwSignal::new(None::<Level>);
    run::<Level>("brightness", vec![], on_error, move |b| brightness.set(b));

    move || {
        brightness.get().map(|b| {
            view! {
                <section class="row">
                    <span class="icon" title="Parlaklık">"☀"</span>
                    <input
                        type="range"
                        min="5"
                        max="100"
                        aria-label="Parlaklık"
                        prop:value=percent(b.level)
                        on:change=move |ev| {
                            let level = slider_level(&ev);
                            run::<Level>("set-brightness", vec![("level", level)], on_error, move |b| brightness.set(b))
                        }
                    />
                </section>
            }
        })
    }
}

#[component]
fn Wifi(on_error: WriteSignal<String>) -> impl IntoView {
    let networks = RwSignal::new(Vec::<Network>::new());
    let selected = RwSignal::new(None::<String>);
    let password = RwSignal::new(String::new());
    let refresh = move || run::<Vec<Network>>("wifi-list", vec![], on_error, move |n| networks.set(n.unwrap_or_default()));
    refresh();

    let connect = move |ssid: String| {
        let args = vec![("ssid", ssid.into()), ("password", password.get().into())];
        password.set(String::new());
        selected.set(None);
        run::<()>("wifi-connect", args, on_error, move |_| refresh());
    };

    view! {
        <details>
            <summary>"Wi-Fi"</summary>
            <ul class="list">
                <For each=move || networks.get() key=|n| n.ssid.clone() let:network>
                    {
                        let ssid = network.ssid.clone();
                        let is_selected = {
                            let ssid = ssid.clone();
                            move || selected.get().as_deref() == Some(ssid.as_str())
                        };
                        let pick = {
                            let ssid = ssid.clone();
                            move |_| {
                                if network.secure {
                                    selected.set(Some(ssid.clone()));
                                } else {
                                    connect(ssid.clone());
                                }
                            }
                        };
                        view! {
                            <li>
                                <button class:active=network.active on:click=pick>
                                    {network.ssid.clone()}
                                    <small>{format!(" %{}{}", network.signal, if network.secure { " 🔒" } else { "" })}</small>
                                </button>
                                <Show when=is_selected>
                                    <form on:submit={
                                        let ssid = ssid.clone();
                                        move |ev| {
                                            ev.prevent_default();
                                            connect(ssid.clone());
                                        }
                                    }>
                                        <input type="password" placeholder="Şifre" bind:value=password />
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
    let update = move |b| bluetooth.set(b);
    run::<Bluetooth>("bluetooth", vec![], on_error, update);

    move || {
        bluetooth.get().map(|b| {
            let devices = b.devices.into_iter().map(|device| {
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
                            {device.name}
                            <small>{format!(" {label}")}</small>
                        </button>
                    </li>
                }
            }).collect_view();
            let powered = b.powered;
            view! {
                <details>
                    <summary>"Bluetooth"</summary>
                    <label class="row">
                        <input
                            type="checkbox"
                            prop:checked=powered
                            on:change={move |_| run::<Bluetooth>("bluetooth-power", vec![("on", (!powered).into())], on_error, update)}
                        />
                        "Açık"
                    </label>
                    <ul class="list">{devices}</ul>
                </details>
            }
        })
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
            <PowerButton label="Uyku" cmd="suspend" on_error=set_error />
            <PowerButton label="Yeniden başlat" cmd="reboot" on_error=set_error />
            <PowerButton label="Kapat" cmd="poweroff" on_error=set_error />
        </div>
        <p id="error" role="alert">{error}</p>
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    mount_to_body(Panel);
}
