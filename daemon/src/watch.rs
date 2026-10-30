//! System events for notifications, pushed over a long-lived native port:
//!
//!   {"event": "usb-added", "name": ".."}
//!   {"event": "update-finished", "ok": bool}
//!   {"event": "battery-low", "percent": n}
//!
//! The page words them in the user's language.

use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const BATTERY_LOW: u8 = 15;

fn emit(event: serde_json::Value) -> bool {
    crate::emit(&event).is_ok()
}

/// New removable filesystems, from udisks2.
fn watch_usb() -> zbus::Result<()> {
    let bus = Connection::system()?;
    let manager = Proxy::new(&bus, "org.freedesktop.UDisks2", "/org/freedesktop/UDisks2", "org.freedesktop.DBus.ObjectManager")?;
    for signal in manager.receive_signal("InterfacesAdded")? {
        type Interfaces = HashMap<String, HashMap<String, OwnedValue>>;
        let (_path, interfaces): (OwnedObjectPath, Interfaces) = signal.body().deserialize()?;
        let (Some(block), Some(_)) = (interfaces.get("org.freedesktop.UDisks2.Block"), interfaces.get("org.freedesktop.UDisks2.Filesystem"))
        else {
            continue;
        };
        let ignored = block.get("HintIgnore").and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
        let system = block.get("HintSystem").and_then(|v| bool::try_from(v).ok()).unwrap_or(true);
        if ignored || system {
            continue;
        }
        let name = block
            .get("IdLabel")
            .and_then(|v| v.try_clone().ok())
            .and_then(|v| String::try_from(v).ok())
            .unwrap_or_default();
        if !emit(json!({ "event": "usb-added", "name": name })) {
            break;
        }
    }
    Ok(())
}

/// The "Update now" service finishing.
fn watch_updates() -> zbus::Result<()> {
    let bus = Connection::system()?;
    let systemd = Proxy::new(&bus, "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager")?;
    let unit: OwnedObjectPath = systemd.call("LoadUnit", &("karakulak-update.service",))?;
    let unit = Proxy::new(&bus, "org.freedesktop.systemd1", unit.as_str(), "org.freedesktop.systemd1.Unit")?;
    let mut last: String = unit.get_property("ActiveState")?;
    for _ in unit.receive_property_changed::<String>("ActiveState") {
        let state: String = unit.get_property("ActiveState")?;
        if last == "activating" && (state == "inactive" || state == "failed") && !emit(json!({ "event": "update-finished", "ok": state == "inactive" })) {
            break;
        }
        last = state;
    }
    Ok(())
}

/// Battery running low, checked every minute; warns once per discharge.
fn watch_battery() {
    let mut warned = false;
    loop {
        if let Some(battery) = crate::battery().as_object().cloned() {
            let percent = battery.get("capacity").and_then(|c| c.as_u64()).unwrap_or(100) as u8;
            let discharging = battery.get("status").and_then(|s| s.as_str()) == Some("Discharging");
            if !discharging {
                warned = false;
            } else if percent <= BATTERY_LOW && !warned {
                warned = true;
                if !emit(json!({ "event": "battery-low", "percent": percent })) {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

/// Starts watching; events arrive until the port closes.
pub fn start() -> Result<serde_json::Value, String> {
    // Firefox passes this to its own output, which the session logs; the boot
    // test looks for it as proof that the panel extension runs.
    eprintln!("karakulakd: watching system events");
    std::thread::spawn(|| {
        if let Err(e) = watch_usb() {
            eprintln!("not watching USB: {e}");
        }
    });
    std::thread::spawn(|| {
        if let Err(e) = watch_updates() {
            eprintln!("not watching updates: {e}");
        }
    });
    std::thread::spawn(watch_battery);
    Ok(json!("watching"))
}
