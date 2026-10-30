//! Power mode: Performance / Balanced / Power saver, through
//! power-profiles-daemon. Any active local user may switch (its own polkit
//! default), so no rule of ours is needed.

use serde_json::{Value, json};
use std::collections::HashMap;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn profiles(bus: &Connection) -> Result<Proxy<'_>, String> {
    Proxy::new(
        bus,
        "org.freedesktop.UPower.PowerProfiles",
        "/org/freedesktop/UPower/PowerProfiles",
        "org.freedesktop.UPower.PowerProfiles",
    )
    .map_err(err)
}

/// `{"active": "balanced", "available": ["power-saver", "balanced", "performance"]}`
pub fn get() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let proxy = profiles(&bus)?;
    let active: String = proxy.get_property("ActiveProfile").map_err(err)?;
    let list: Vec<HashMap<String, OwnedValue>> = proxy.get_property("Profiles").map_err(err)?;
    let available: Vec<String> = list
        .iter()
        .filter_map(|p| String::try_from(p.get("Profile")?.try_clone().ok()?).ok())
        .collect();
    Ok(json!({ "active": active, "available": available }))
}

pub fn set(profile: &str) -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    profiles(&bus)?.set_property("ActiveProfile", profile).map_err(err)?;
    get()
}
