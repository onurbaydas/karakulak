//! System settings through systemd's own services: language and keyboard
//! (localed), time zone (timedated), updates and firewall (units started via
//! systemd). A polkit rule lets the active local admin do exactly these
//! without a password prompt (config/includes.chroot/etc/polkit-1/rules.d).

use serde_json::{Value, json};
use std::process::Command;
use zbus::blocking::{Connection, Proxy};

const UPDATE_UNIT: &str = "karakulak-update.service";
const FIREWALL_UNIT: &str = "nftables.service";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn proxy<'a>(bus: &Connection, service: &'a str, path: &'a str, interface: &'a str) -> Result<Proxy<'a>, String> {
    Proxy::new(bus, service, path, interface).map_err(err)
}

fn locale1(bus: &Connection) -> Result<Proxy<'_>, String> {
    proxy(bus, "org.freedesktop.locale1", "/org/freedesktop/locale1", "org.freedesktop.locale1")
}

fn timedate1(bus: &Connection) -> Result<Proxy<'_>, String> {
    proxy(bus, "org.freedesktop.timedate1", "/org/freedesktop/timedate1", "org.freedesktop.timedate1")
}

fn systemd(bus: &Connection) -> Result<Proxy<'_>, String> {
    proxy(bus, "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager")
}

/// `{"language", "layout", "variant", "timezone", "timezones": [...]}`
pub fn regional() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let locale = locale1(&bus)?;
    let settings: Vec<String> = locale.get_property("Locale").map_err(err)?;
    let language = settings
        .iter()
        .find_map(|s| s.strip_prefix("LANG="))
        .unwrap_or("C.UTF-8")
        .to_string();
    let layout: String = locale.get_property("X11Layout").map_err(err)?;
    let variant: String = locale.get_property("X11Variant").map_err(err)?;
    let timezone: String = timedate1(&bus)?.get_property("Timezone").map_err(err)?;
    let timezones = Command::new("timedatectl")
        .arg("list-timezones")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(json!({
        "language": language,
        "layout": layout,
        "variant": variant,
        "timezone": timezone,
        "timezones": timezones,
    }))
}

/// Takes effect at the next login.
pub fn set_language(lang: &str) -> Result<Value, String> {
    if !lang.chars().all(|c| c.is_ascii_alphanumeric() || "_.-@".contains(c)) {
        return Err("invalid language".into());
    }
    let bus = Connection::system().map_err(err)?;
    let _: () = locale1(&bus)?
        .call("SetLocale", &(vec![format!("LANG={lang}")], false))
        .map_err(err)?;
    regional()
}

/// Saved for the console and new sessions; the compositor picks it up at once.
pub fn set_keyboard(layout: &str, variant: &str) -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let _: () = locale1(&bus)?
        .call("SetX11Keyboard", &(layout, "", variant, "", true, false))
        .map_err(err)?;
    let _ = crate::brightness::compositor(json!({ "cmd": "reload-keyboard" }));
    regional()
}

pub fn set_timezone(zone: &str) -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let _: () = timedate1(&bus)?.call("SetTimezone", &(zone, false)).map_err(err)?;
    regional()
}

/// ActiveState of a unit ("active", "activating", "inactive", "failed").
fn unit_state(bus: &Connection, unit: &str) -> Result<String, String> {
    let path: zbus::zvariant::OwnedObjectPath = systemd(bus)?.call("LoadUnit", &(unit,)).map_err(err)?;
    let unit = proxy(bus, "org.freedesktop.systemd1", path.as_str(), "org.freedesktop.systemd1.Unit")?;
    unit.get_property("ActiveState").map_err(err)
}

/// Packages with a newer version in the already downloaded lists.
fn pending_updates() -> usize {
    Command::new("apt-get")
        .args(["-s", "-o", "Debug::NoLocking=1", "dist-upgrade"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.starts_with("Inst ")).count())
        .unwrap_or(0)
}

/// `{"pending", "state"}`; state is the update unit's ActiveState.
pub fn updates() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    Ok(json!({ "pending": pending_updates(), "state": unit_state(&bus, UPDATE_UNIT)? }))
}

/// Starts the update service (apt-get update && full-upgrade, as root).
pub fn update_now() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let _: zbus::zvariant::OwnedObjectPath = systemd(&bus)?
        .call("StartUnit", &(UPDATE_UNIT, "replace"))
        .map_err(err)?;
    updates()
}

/// `{"on"}`
pub fn firewall() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    Ok(json!({ "on": unit_state(&bus, FIREWALL_UNIT)? == "active" }))
}

/// Turning it off lasts until the next boot; it always starts on.
pub fn set_firewall(on: bool) -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let method = if on { "StartUnit" } else { "StopUnit" };
    let _: zbus::zvariant::OwnedObjectPath = systemd(&bus)?
        .call(method, &(FIREWALL_UNIT, "replace"))
        .map_err(err)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    firewall()
}
