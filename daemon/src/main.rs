//! Native messaging host: Firefox extensions send `{"cmd": "...", ...args}` and get
//! back `{"ok": true, "result": ...}` or `{"ok": false, "error": "..."}`.
//! Each message is a 4-byte native-endian length followed by JSON.

mod audio;
mod bluetooth;
mod brightness;
mod files;
mod terminal;
mod system;
mod tasks;
mod usb;
mod vpn;
mod watch;
mod wifi;

use serde_json::{json, Value};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::Command;

fn read_message(input: &mut impl Read) -> io::Result<Option<Value>> {
    let mut len = [0u8; 4];
    match input.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let mut buf = vec![0u8; u32::from_ne_bytes(len) as usize];
    input.read_exact(&mut buf)?;
    Ok(Some(serde_json::from_slice(&buf)?))
}

fn write_message(output: &mut impl Write, msg: &Value) -> io::Result<()> {
    let buf = serde_json::to_vec(msg)?;
    output.write_all(&(buf.len() as u32).to_ne_bytes())?;
    output.write_all(&buf)?;
    output.flush()
}

/// What a guest session may do; everything else (files, terminal, settings,
/// VPN, programs) belongs to the owner. A new command is closed to guests
/// until it is added here.
const GUEST_ALLOWED: &[&str] = &[
    "ping", "session", "battery", "volume", "set-volume", "toggle-mute", "sound-devices", "set-sound-output",
    "wifi-list", "wifi-connect", "brightness", "set-brightness", "bluetooth", "bluetooth-connect",
    "bluetooth-disconnect", "watch", "guest-end", "lock", "unlock", "poweroff", "reboot", "suspend",
];

fn is_guest() -> bool {
    std::env::var("KARAKULAK_GUEST").as_deref() == Ok("1")
}

fn forward(msg: &Value, cmd: &str) -> Result<Value, String> {
    let mut request = msg.clone();
    request["cmd"] = json!(cmd);
    brightness::compositor(request).unwrap_or(Err("not in Karakulak's compositor".into()))
}

fn handle(msg: &Value) -> Result<Value, String> {
    let arg = |name: &str| msg[name].as_str().unwrap_or("");
    if is_guest() && !GUEST_ALLOWED.contains(&arg("cmd")) {
        return Err("not available in a guest session".into());
    }
    match arg("cmd") {
        "session" => Ok(json!({ "guest": is_guest() })),
        "guest-start" | "guest-end" => forward(msg, arg("cmd")),
        "ping" => Ok(json!("pong")),
        "battery" => Ok(battery()),
        cmd @ ("poweroff" | "reboot" | "suspend") => systemctl(cmd),
        "volume" => audio::get(),
        "set-volume" => audio::set(msg["level"].as_f64().ok_or("missing level")?),
        "toggle-mute" => audio::toggle_mute(),
        "sound-devices" => audio::devices(),
        "set-sound-output" => audio::set_output(arg("id")),
        "set-sound-input" => audio::set_input(arg("id")),
        "set-mic-level" => audio::set_input_level(msg["level"].as_f64().ok_or("missing level")?),
        "wifi-list" => wifi::list(),
        "wifi-connect" => wifi::connect(arg("ssid"), arg("password")),
        "brightness" => brightness::get(),
        "set-brightness" => brightness::set(arg("id"), msg["level"].as_f64().ok_or("missing level")?),
        "bluetooth" => bluetooth::list(),
        "bluetooth-power" => bluetooth::set_powered(msg["on"].as_bool().unwrap_or(false)),
        "bluetooth-connect" => bluetooth::set_connected(arg("path"), true),
        "bluetooth-disconnect" => bluetooth::set_connected(arg("path"), false),
        "bluetooth-scan" => bluetooth::scan(),
        "files-places" => Ok(files::places()),
        "files-list" => files::list(arg("path"), msg["hidden"].as_bool().unwrap_or(false)),
        "files-open" => files::open(arg("path")),
        "files-search" => files::search(arg("path"), arg("query")),
        "files-thumbnail" => files::thumbnail(arg("path")),
        "files-mkdir" => files::mkdir(arg("path"), arg("name")),
        "files-rename" => files::rename(arg("path"), arg("name")),
        "files-copy" => files::transfer(&strings(&msg["paths"]), arg("to"), true),
        "files-move" => files::transfer(&strings(&msg["paths"]), arg("to"), false),
        "files-trash" => files::to_trash(&strings(&msg["paths"])),
        "usb-list" => usb::list(),
        "usb-mount" => usb::mount(arg("id")),
        "usb-eject" => usb::eject(arg("id")),
        "regional" => system::regional(),
        "set-language" => system::set_language(arg("language")),
        "set-keyboard" => system::set_keyboard(arg("layout"), arg("variant")),
        "set-timezone" => system::set_timezone(arg("timezone")),
        "updates" => system::updates(),
        "update-now" => system::update_now(),
        "firewall" => system::firewall(),
        "set-firewall" => system::set_firewall(msg["on"].as_bool().unwrap_or(true)),
        "vpn-list" => vpn::list(),
        "vpn-import" => vpn::import(arg("name"), arg("config")),
        "vpn-set" => vpn::set_active(arg("id"), msg["on"].as_bool().unwrap_or(false)),
        "vpn-remove" => vpn::remove(arg("id")),
        "tasks" => tasks::overview(),
        "watch" => watch::start(),
        "task-stop" => tasks::stop(num(msg, "pid"), msg["force"].as_bool().unwrap_or(false)),
        "term-open" => terminal::open(num(msg, "cols"), num(msg, "rows")),
        "term-input" => terminal::input(num(msg, "term"), arg("data")),
        "term-resize" => terminal::resize(num(msg, "term"), num(msg, "cols"), num(msg, "rows")),
        "bluetooth-pair" => bluetooth::pair(arg("path")),
        "set-software-brightness" => {
            let mut request = msg.clone();
            request["cmd"] = json!("set-software-brightness");
            brightness::compositor(request).unwrap_or(Err("not in Karakulak's compositor".into()))
        }
        "night-light" => {
            let mut request = msg.clone();
            request["cmd"] = json!("night-light");
            brightness::compositor(request).unwrap_or(Err("not in Karakulak's compositor".into()))
        }
        "screens" => brightness::compositor(json!({ "cmd": "screens" })).unwrap_or(Err("not in Karakulak's compositor".into())),
        "set-screen" => {
            let mut request = msg.clone();
            request["cmd"] = json!("set-screen");
            brightness::compositor(request).unwrap_or(Err("not in Karakulak's compositor".into()))
        }
        "lock" => brightness::compositor(json!({ "cmd": "lock" })).unwrap_or(Err("not in Karakulak's compositor".into())),
        "unlock" => brightness::compositor(json!({ "cmd": "unlock", "password": arg("password") }))
            .unwrap_or(Err("not in Karakulak's compositor".into())),
        cmd => Err(format!("unknown command: {cmd}")),
    }
}

fn num(msg: &Value, name: &str) -> u64 {
    msg[name].as_u64().unwrap_or(0)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(String::from))
        .collect()
}

fn systemctl(action: &str) -> Result<Value, String> {
    let status = Command::new("systemctl")
        .arg(action)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(Value::Null)
    } else {
        Err(format!("systemctl {action} failed: {status}"))
    }
}

/// First battery found, or null on machines without one.
pub fn battery() -> Value {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return Value::Null;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if read_trimmed(&dir, "type").as_deref() != Some("Battery") {
            continue;
        }
        let capacity = read_trimmed(&dir, "capacity").and_then(|s| s.parse::<u8>().ok());
        return json!({ "capacity": capacity, "status": read_trimmed(&dir, "status") });
    }
    Value::Null
}

fn read_trimmed(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_string())
}

/// Sends one message to Firefox. Terminal threads use it too; the stdout lock
/// keeps messages from interleaving.
pub fn emit(msg: &Value) -> io::Result<()> {
    write_message(&mut io::stdout().lock(), msg)
}

fn main() -> io::Result<()> {
    let mut input = io::stdin().lock();
    while let Some(msg) = read_message(&mut input)? {
        let reply = match handle(&msg) {
            Ok(result) => json!({ "ok": true, "result": result }),
            Err(error) => json!({ "ok": false, "error": error }),
        };
        emit(&reply)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_round_trip() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({ "cmd": "ping" })).unwrap();
        let msg = read_message(&mut buf.as_slice()).unwrap().unwrap();
        assert_eq!(msg, json!({ "cmd": "ping" }));
    }

    #[test]
    fn end_of_input_is_none() {
        assert!(read_message(&mut [].as_slice()).unwrap().is_none());
    }

    #[test]
    fn unknown_command_is_error() {
        assert!(handle(&json!({ "cmd": "rm -rf /" })).is_err());
    }
}
