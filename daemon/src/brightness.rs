//! Screen brightness. Laptop panels through the kernel backlight (written via
//! logind, no root needed); external monitors over DDC/CI.

use ddc_hi::{Ddc, Display};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use zbus::blocking::{Connection, Proxy};

/// VCP feature code for brightness in the DDC/CI standard.
const VCP_BRIGHTNESS: u8 = 0x10;
const BACKLIGHT_PREFIX: &str = "backlight:";

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

fn backlight_level(dir: &Path) -> Result<f64, String> {
    let current = read_number(dir.join("brightness"))?;
    let max = read_number(dir.join("max_brightness"))?;
    Ok(f64::from(current) / f64::from(max.max(1)))
}

/// Every screen whose brightness we can change:
/// `[{"id", "name", "level": 0.0..=1.0}]`
pub fn get() -> Result<Value, String> {
    let mut screens = Vec::new();
    if let Some(dir) = backlight() {
        let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        screens.push(json!({
            "id": format!("{BACKLIGHT_PREFIX}{name}"),
            "name": "Dahili ekran",
            "level": backlight_level(&dir)?,
        }));
    }
    for mut display in Display::enumerate() {
        // Monitors without DDC/CI support are skipped.
        let Ok(value) = display.handle.get_vcp_feature(VCP_BRIGHTNESS) else {
            continue;
        };
        screens.push(json!({
            "id": display.info.id,
            "name": display.info.model_name.clone().unwrap_or_else(|| "Harici ekran".into()),
            "level": f64::from(value.value()) / f64::from(value.maximum().max(1)),
        }));
    }
    Ok(Value::Array(screens))
}

pub fn set(id: &str, level: f64) -> Result<Value, String> {
    if let Some(name) = id.strip_prefix(BACKLIGHT_PREFIX) {
        set_backlight(name, level)?;
        return get();
    }
    let mut display = Display::enumerate()
        .into_iter()
        .find(|d| d.info.id == id)
        .ok_or("screen not found")?;
    let max = display
        .handle
        .get_vcp_feature(VCP_BRIGHTNESS)
        .map_err(|e| e.to_string())?
        .maximum();
    let value = (level.clamp(0.0, 1.0) * f64::from(max)).round() as u16;
    display
        .handle
        .set_vcp_feature(VCP_BRIGHTNESS, value)
        .map_err(|e| e.to_string())?;
    get()
}

fn set_backlight(name: &str, level: f64) -> Result<(), String> {
    let dir = PathBuf::from("/sys/class/backlight").join(name);
    if name.contains('/') || !dir.exists() {
        return Err("no such backlight".into());
    }
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
    .and_then(|session| session.call_method("SetBrightness", &("backlight", name, value)))
    .map_err(|e| e.to_string())?;
    Ok(())
}
