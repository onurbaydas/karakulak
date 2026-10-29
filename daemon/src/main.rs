//! Native messaging host: Firefox extensions send `{"cmd": "...", ...args}` and get
//! back `{"ok": true, "result": ...}` or `{"ok": false, "error": "..."}`.
//! Each message is a 4-byte native-endian length followed by JSON.

mod audio;
mod bluetooth;
mod brightness;
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

fn handle(msg: &Value) -> Result<Value, String> {
    let arg = |name: &str| msg[name].as_str().unwrap_or("");
    match arg("cmd") {
        "ping" => Ok(json!("pong")),
        "battery" => Ok(battery()),
        cmd @ ("poweroff" | "reboot" | "suspend") => systemctl(cmd),
        "volume" => audio::get(),
        "set-volume" => audio::set(msg["level"].as_f64().ok_or("missing level")?),
        "toggle-mute" => audio::toggle_mute(),
        "wifi-list" => wifi::list(),
        "wifi-connect" => wifi::connect(arg("ssid"), arg("password")),
        "brightness" => brightness::get(),
        "set-brightness" => brightness::set(arg("id"), msg["level"].as_f64().ok_or("missing level")?),
        "bluetooth" => bluetooth::list(),
        "bluetooth-power" => bluetooth::set_powered(msg["on"].as_bool().unwrap_or(false)),
        "bluetooth-connect" => bluetooth::set_connected(arg("path"), true),
        "bluetooth-disconnect" => bluetooth::set_connected(arg("path"), false),
        "bluetooth-scan" => bluetooth::scan(),
        "bluetooth-pair" => bluetooth::pair(arg("path")),
        "lock" => brightness::compositor(json!({ "cmd": "lock" })).unwrap_or(Err("not in Karakulak's compositor".into())),
        "unlock" => brightness::compositor(json!({ "cmd": "unlock", "password": arg("password") }))
            .unwrap_or(Err("not in Karakulak's compositor".into())),
        cmd => Err(format!("unknown command: {cmd}")),
    }
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
fn battery() -> Value {
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

fn main() -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    while let Some(msg) = read_message(&mut input)? {
        let reply = match handle(&msg) {
            Ok(result) => json!({ "ok": true, "result": result }),
            Err(error) => json!({ "ok": false, "error": error }),
        };
        write_message(&mut output, &reply)?;
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
