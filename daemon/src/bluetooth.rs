//! Bluetooth through BlueZ's D-Bus API: power and already paired devices.
//! Pairing new devices comes later (it needs a pairing agent).

use serde_json::{json, Value};
use std::collections::HashMap;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as Variant};

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn objects(bus: &Connection) -> Result<Objects, String> {
    Proxy::new(bus, BLUEZ, "/", "org.freedesktop.DBus.ObjectManager")
        .and_then(|manager| manager.call("GetManagedObjects", &()))
        .map_err(err)
}

fn prop<T: TryFrom<OwnedValue>>(props: &HashMap<String, OwnedValue>, name: &str) -> Option<T> {
    props.get(name)?.try_clone().ok()?.try_into().ok()
}

fn adapter(objects: &Objects) -> Option<(&OwnedObjectPath, &HashMap<String, OwnedValue>)> {
    objects
        .iter()
        .find_map(|(path, interfaces)| interfaces.get(ADAPTER).map(|props| (path, props)))
}

/// `{"powered": bool, "devices": [{"path", "name", "connected"}]}`,
/// or null without a Bluetooth adapter.
pub fn list() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let objects = objects(&bus)?;
    let Some((_, adapter)) = adapter(&objects) else {
        return Ok(Value::Null);
    };

    let mut devices: Vec<Value> = objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let props = interfaces.get(DEVICE)?;
            prop::<bool>(props, "Paired").filter(|&paired| paired)?;
            Some(json!({
                "path": path.as_str(),
                "name": prop::<String>(props, "Alias").unwrap_or_default(),
                "connected": prop::<bool>(props, "Connected").unwrap_or(false),
            }))
        })
        .collect();
    devices.sort_by_key(|d| d["name"].to_string());

    Ok(json!({
        "powered": prop::<bool>(adapter, "Powered").unwrap_or(false),
        "devices": devices,
    }))
}

pub fn set_powered(on: bool) -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let objects = objects(&bus)?;
    let (path, _) = adapter(&objects).ok_or("no Bluetooth adapter")?;
    Proxy::new(&bus, BLUEZ, path.as_str(), "org.freedesktop.DBus.Properties")
        .and_then(|p| p.call_method("Set", &(ADAPTER, "Powered", Variant::from(on))))
        .map_err(err)?;
    list()
}

/// Connects or disconnects a paired device.
pub fn set_connected(path: &str, connect: bool) -> Result<Value, String> {
    if !path.starts_with("/org/bluez/") {
        return Err("not a Bluetooth device".into());
    }
    let bus = Connection::system().map_err(err)?;
    Proxy::new(&bus, BLUEZ, path, DEVICE)
        .and_then(|d| d.call_method(if connect { "Connect" } else { "Disconnect" }, &()))
        .map_err(err)?;
    list()
}
