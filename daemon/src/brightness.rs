//! Screen brightness. Read from sysfs, written through logind so no root is
//! needed.

use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use zbus::blocking::{Connection, Proxy};

fn backlight() -> Option<PathBuf> {
    fs::read_dir("/sys/class/backlight").ok()?.flatten().next().map(|e| e.path())
}

fn read_number(path: PathBuf) -> Result<u32, String> {
    fs::read_to_string(&path)
        .map_err(|e| e.to_string())?
        .trim()
        .parse()
        .map_err(|_| format!("bad number in {}", path.display()))
}

/// `{"level": 0.0..=1.0}`, or null without a backlight (desktops).
pub fn get() -> Result<Value, String> {
    let Some(dir) = backlight() else {
        return Ok(Value::Null);
    };
    let current = read_number(dir.join("brightness"))?;
    let max = read_number(dir.join("max_brightness"))?;
    Ok(json!({ "level": f64::from(current) / f64::from(max.max(1)) }))
}

pub fn set(level: f64) -> Result<Value, String> {
    let dir = backlight().ok_or("no backlight")?;
    let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let max = read_number(dir.join("max_brightness"))?;
    // Never fully dark: a black screen looks like a dead machine.
    let value = (level.clamp(0.05, 1.0) * f64::from(max)).round() as u32;

    let bus = Connection::system().map_err(|e| e.to_string())?;
    Proxy::new(
        &bus,
        "org.freedesktop.login1",
        "/org/freedesktop/login1/session/auto",
        "org.freedesktop.login1.Session",
    )
    .and_then(|session| session.call_method("SetBrightness", &("backlight", name.as_str(), value)))
    .map_err(|e| e.to_string())?;
    get()
}
