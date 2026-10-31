//! Demo mode (`--features demo`): the pages run on a plain website, with
//! made-up answers in place of karakulakd. Settings changed here only live
//! until the page is reloaded.

use std::cell::RefCell;

use serde_json::{Value, json};
use wasm_bindgen::JsValue;

const HOME: &str = "/home/demo";

thread_local! {
    static STATE: RefCell<Value> = RefCell::new(json!({
        "volume": { "level": 0.6, "muted": false },
        "brightness": 0.8,
        "profile": "balanced",
        "layout": 0,
        "wifi": "Ev",
        "bluetooth": true,
        "night": "off",
        "high_contrast": false,
        "firewall": true,
        "tor": false,
        "language": "tr_TR.UTF-8",
        "keyboard": ["tr", ""],
        "timezone": "Europe/Istanbul",
        "scale": 1.0,
        "install": null,
    }));
}

fn state() -> Value {
    STATE.with(|s| s.borrow().clone())
}

fn set(key: &str, value: Value) {
    STATE.with(|s| s.borrow_mut()[key] = value);
}

fn arg(args: &[(&str, JsValue)], name: &str) -> Value {
    let Some((_, value)) = args.iter().find(|(key, _)| *key == name) else {
        return Value::Null;
    };
    if let Some(s) = value.as_string() {
        json!(s)
    } else if let Some(b) = value.as_bool() {
        json!(b)
    } else if let Some(n) = value.as_f64() {
        json!(n)
    } else {
        Value::Null
    }
}

fn now() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

fn jitter(value: f64, spread: f64) -> f64 {
    (value + (js_sys::Math::random() - 0.5) * spread).max(0.0)
}

/// Answers a karakulakd command the way the real daemon would.
pub fn reply(cmd: &str, args: &[(&str, JsValue)]) -> Result<Value, String> {
    let s = state();
    Ok(match cmd {
        "session" => json!({ "guest": false, "live": true }),
        "battery" => json!({ "capacity": 76, "status": "Discharging" }),
        "volume" => s["volume"].clone(),
        "set-volume" => {
            set("volume", json!({ "level": arg(args, "level"), "muted": false }));
            state()["volume"].clone()
        }
        "toggle-mute" => {
            let muted = !s["volume"]["muted"].as_bool().unwrap_or(false);
            set("volume", json!({ "level": s["volume"]["level"], "muted": muted }));
            state()["volume"].clone()
        }
        "brightness" | "set-brightness" => {
            if cmd == "set-brightness" {
                set("brightness", arg(args, "level"));
            }
            json!([{ "id": "eDP-1", "name": "Built-in screen", "level": state()["brightness"], "method": "backlight", "software": false }])
        }
        "power-profile" | "set-power-profile" => {
            if cmd == "set-power-profile" {
                set("profile", arg(args, "profile"));
            }
            json!({ "active": state()["profile"], "available": ["power-saver", "balanced", "performance"] })
        }
        "keyboard-layout" | "next-layout" => {
            if cmd == "next-layout" {
                set("layout", json!((s["layout"].as_u64().unwrap_or(0) + 1) % 2));
            }
            json!({ "layouts": ["tr", "us"], "current": state()["layout"] })
        }
        "wifi-list" | "wifi-connect" => {
            if cmd == "wifi-connect" {
                set("wifi", arg(args, "ssid"));
            }
            let active = state()["wifi"].clone();
            let networks = [("Ev", 82, true), ("Kafe", 64, false), ("Komşu", 41, true), ("Ofis-5G", 30, true)];
            let list: Vec<Value> = networks
                .iter()
                .map(|(ssid, signal, secure)| {
                    json!({ "ssid": ssid, "signal": signal, "secure": secure, "active": active == *ssid, "known": *ssid == "Ev" || active == *ssid })
                })
                .collect();
            if cmd == "wifi-connect" { Value::Null } else { json!(list) }
        }
        "bluetooth" | "bluetooth-power" => {
            if cmd == "bluetooth-power" {
                set("bluetooth", arg(args, "on"));
            }
            let powered = state()["bluetooth"].as_bool().unwrap_or(false);
            let devices = if powered {
                json!([{ "path": "/demo/headphones", "name": "Kulaklık", "connected": true }])
            } else {
                json!([])
            };
            json!({ "powered": powered, "devices": devices })
        }
        "bluetooth-scan" => json!([{ "path": "/demo/speaker", "name": "Hoparlör" }]),
        "tasks" => {
            let processes = [
                ("firefox-bin", 0.06, 498_000_000u64),
                ("Web Content", 0.03, 212_000_000),
                ("karakulak-compositor", 0.02, 96_000_000),
                ("WebExtensions", 0.01, 178_000_000),
                ("karakulakd", 0.0, 9_000_000),
                ("pipewire", 0.0, 14_000_000),
                ("NetworkManager", 0.0, 21_000_000),
            ];
            let list: Vec<Value> = processes
                .iter()
                .enumerate()
                .map(|(i, (name, cpu, memory))| json!({ "pid": 1200 + i * 37, "name": name, "cpu": jitter(*cpu, 0.02), "memory": memory }))
                .collect();
            json!({
                "cpu": jitter(0.12, 0.06),
                "memory": { "used": 1_600_000_000u64, "total": 8_000_000_000u64 },
                "disk": { "used": 9_400_000_000u64, "total": 120_000_000_000u64 },
                "processes": list,
            })
        }
        "task-stop" => Value::Null,
        "files-places" => json!([
            { "name": "Ev", "path": HOME },
            { "name": "Belgeler", "path": format!("{HOME}/Belgeler") },
            { "name": "Resimler", "path": format!("{HOME}/Resimler") },
            { "name": "İndirilenler", "path": format!("{HOME}/İndirilenler") },
        ]),
        "files-list" => files(arg(args, "path").as_str().unwrap_or(HOME)),
        "files-search" => {
            let query = arg(args, "query").as_str().unwrap_or("").to_lowercase();
            let mut listing = files(HOME);
            let mut found = vec![];
            for dir in ["Belgeler", "Resimler", "İndirilenler"] {
                if let Some(entries) = files(&format!("{HOME}/{dir}"))["entries"].as_array() {
                    found.extend(entries.iter().filter(|e| e["name"].as_str().unwrap_or("").to_lowercase().contains(&query)).cloned());
                }
            }
            listing["entries"] = json!(found);
            listing
        }
        "usb-list" => json!([{ "id": "sdb1", "name": "USB bellek (16 GB)", "mounted": null }]),
        "usb-mount" => json!({ "path": "/media/demo/USB" }),
        "files-open" | "files-mkdir" | "files-rename" | "files-trash" | "usb-eject" => {
            return Err("demo: dosyalar değiştirilemez".into());
        }
        "screens" | "set-screen" => {
            if cmd == "set-screen" {
                set("scale", arg(args, "scale"));
            }
            json!([{
                "id": "eDP-1",
                "name": "Built-in screen",
                "current": [1920, 1080, 60000],
                "scale": state()["scale"],
                "modes": [[1920, 1080, 60000], [1600, 900, 60000], [1280, 720, 60000]],
            }])
        }
        "set-software-brightness" => json!([{ "id": "eDP-1", "name": "Built-in screen", "method": "backlight", "software": arg(args, "on") }]),
        "night-light" => {
            if let Some(mode) = arg(args, "mode").as_str() {
                set("night", json!(mode));
            }
            json!({ "mode": state()["night"] })
        }
        "accessibility" | "set-high-contrast" => {
            if cmd == "set-high-contrast" {
                set("high_contrast", arg(args, "on"));
            }
            json!({ "high_contrast": state()["high_contrast"] })
        }
        "regional" | "set-language" | "set-keyboard" | "set-timezone" => {
            match cmd {
                "set-language" => set("language", arg(args, "language")),
                "set-keyboard" => set("keyboard", json!([arg(args, "layout"), arg(args, "variant")])),
                "set-timezone" => set("timezone", arg(args, "timezone")),
                _ => {}
            }
            let s = state();
            json!({
                "language": s["language"],
                "layout": s["keyboard"][0],
                "variant": s["keyboard"][1],
                "timezone": s["timezone"],
                "timezones": ["Europe/Istanbul", "Europe/Berlin", "Europe/London", "America/New_York", "Asia/Tokyo"],
            })
        }
        "updates" => json!({ "pending": 0, "state": "inactive" }),
        "update-now" => json!({ "pending": 0, "state": "activating" }),
        "firewall" | "set-firewall" => {
            if cmd == "set-firewall" {
                set("firewall", arg(args, "on"));
            }
            json!({ "on": state()["firewall"] })
        }
        "tor" | "set-tor" => {
            if cmd == "set-tor" {
                set("tor", arg(args, "on"));
            }
            json!({ "on": state()["tor"], "ready": state()["tor"] })
        }
        "sound-devices" | "set-mic-level" => json!({
            "outputs": [{ "id": "1", "name": "Hoparlörler", "default": true, "level": s["volume"]["level"] }],
            "inputs": [{ "id": "2", "name": "Dahili mikrofon", "default": true, "level": 0.7 }],
        }),
        "vpn-list" => json!([]),
        "backups" => json!([]),
        "report" => json!({
            "path": format!("{HOME}/karakulak-rapor.txt"),
            "text": "Karakulak problem report\n\nThis is the demo: on a real system the report holds the versions, the hardware and the recent log.\n",
        }),
        "install-info" => install(),
        "install-start" => {
            set("install", json!(now()));
            install()
        }
        "lock" | "reboot" | "guest-start" | "guest-end" | "files-thumbnail" => {
            return Err("demo: bu tarayıcıda çalışmaz".into());
        }
        _ => return Err(format!("demo: {cmd}")),
    })
}

/// The installer pretends to take half a minute.
fn install() -> Value {
    let disks = json!([
        { "name": "nvme0n1", "model": "Samsung SSD 980", "size": 512e9, "usb": false },
        { "name": "sdb", "model": "SanDisk Ultra", "size": 16e9, "usb": true },
    ]);
    let status = state()["install"].as_u64().map(|started| {
        let progress = (now() - started) as f64 / 30.0;
        let step = match progress {
            p if p < 0.1 => "partition",
            p if p < 0.7 => "copy",
            p if p < 0.9 => "configure",
            _ => "bootloader",
        };
        let state = if progress >= 1.0 { "done" } else { "running" };
        json!({ "step": step, "progress": progress.min(1.0), "state": state, "error": null })
    });
    json!({ "live": true, "disks": disks, "status": status })
}

fn files(path: &str) -> Value {
    let day = 86_400;
    let t = now();
    let entry = |name: &str, dir: bool, size: u64, age: u64| {
        json!({ "name": name, "path": format!("{path}/{name}"), "dir": dir, "size": size, "modified": t - age })
    };
    let entries = match path.strip_prefix(HOME).unwrap_or("") {
        "" => vec![
            entry("Belgeler", true, 0, 2 * day),
            entry("Resimler", true, 0, 5 * day),
            entry("İndirilenler", true, 0, 3600),
            entry("notlar.txt", false, 2_400, 600),
        ],
        "/Belgeler" => vec![
            entry("CV.pdf", false, 182_000, 30 * day),
            entry("Bütçe 2026.ods", false, 24_000, 4 * day),
            entry("Toplantı notları.odt", false, 31_000, day),
        ],
        "/Resimler" => vec![
            entry("Tatil", true, 0, 60 * day),
            entry("Ekran görüntüsü.png", false, 410_000, 2 * day),
        ],
        "/İndirilenler" => vec![
            entry("karakulak-v0.9.7-amd64.iso", false, 1_897_660_416, 3600),
            entry("fatura.pdf", false, 96_000, 7 * day),
        ],
        _ => vec![],
    };
    let parent = (path != HOME).then(|| path.rsplit_once('/').map(|(p, _)| p.to_string())).flatten();
    json!({ "path": path, "parent": parent, "entries": entries, "truncated": false })
}
