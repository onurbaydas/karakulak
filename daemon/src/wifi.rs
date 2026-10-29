//! Wi-Fi through NetworkManager's D-Bus API. Passwords go over D-Bus, never
//! on a command line.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as Variant};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const DEVICE_TYPE_WIFI: u32 = 2;

type ConnectionSettings = HashMap<String, HashMap<String, OwnedValue>>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn proxy<'a>(bus: &Connection, path: &'a str, interface: &'a str) -> Result<Proxy<'a>, String> {
    Proxy::new(bus, NM, path.to_owned(), interface).map_err(err)
}

fn wifi_device(bus: &Connection) -> Result<OwnedObjectPath, String> {
    let devices: Vec<OwnedObjectPath> = proxy(bus, NM_PATH, NM)?
        .call("GetDevices", &())
        .map_err(err)?;
    for device in devices {
        let kind: u32 = proxy(bus, device.as_str(), DEVICE)?
            .get_property("DeviceType")
            .map_err(err)?;
        if kind == DEVICE_TYPE_WIFI {
            return Ok(device);
        }
    }
    Err("no Wi-Fi device".into())
}

/// Saved Wi-Fi connections by network name.
fn saved(bus: &Connection) -> Result<HashMap<String, OwnedObjectPath>, String> {
    let paths: Vec<OwnedObjectPath> = proxy(bus, SETTINGS_PATH, SETTINGS)?
        .call("ListConnections", &())
        .map_err(err)?;
    let mut saved = HashMap::new();
    for path in paths {
        let settings: ConnectionSettings = proxy(bus, path.as_str(), CONNECTION)?
            .call("GetSettings", &())
            .map_err(err)?;
        let ssid = settings
            .get("802-11-wireless")
            .and_then(|wireless| wireless.get("ssid"))
            .and_then(|ssid| Vec::<u8>::try_from(ssid.try_clone().ok()?).ok());
        if let Some(ssid) = ssid {
            saved.insert(String::from_utf8_lossy(&ssid).into_owned(), path);
        }
    }
    Ok(saved)
}

/// Visible networks, strongest first, one entry per name:
/// `[{"ssid", "signal", "secure", "active", "known"}]`
pub fn list() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let device = wifi_device(&bus)?;
    let wireless = proxy(&bus, device.as_str(), WIRELESS)?;

    // Fresh results arrive in the background; this call only asks for them.
    let _ = wireless.call_method("RequestScan", &(HashMap::<&str, Variant>::new(),));

    let active: OwnedObjectPath = wireless.get_property("ActiveAccessPoint").map_err(err)?;
    let access_points: Vec<OwnedObjectPath> =
        wireless.call("GetAllAccessPoints", &()).map_err(err)?;

    let saved = saved(&bus)?;
    let mut networks = Vec::new();
    for path in access_points {
        let ap = proxy(&bus, path.as_str(), ACCESS_POINT)?;
        let ssid: Vec<u8> = ap.get_property("Ssid").map_err(err)?;
        if ssid.is_empty() {
            continue;
        }
        let signal: u8 = ap.get_property("Strength").map_err(err)?;
        let flags: u32 = ap.get_property("Flags").map_err(err)?;
        let wpa: u32 = ap.get_property("WpaFlags").map_err(err)?;
        let rsn: u32 = ap.get_property("RsnFlags").map_err(err)?;
        let ssid = String::from_utf8_lossy(&ssid).into_owned();
        networks.push(json!({
            "known": saved.contains_key(&ssid),
            "ssid": ssid,
            "signal": signal,
            "secure": flags & 1 != 0 || wpa != 0 || rsn != 0,
            "active": path == active,
        }));
    }

    // Active first, then strongest; keep one entry per network name.
    networks.sort_by_key(|n| (n["active"] != true, std::cmp::Reverse(n["signal"].as_u64())));
    let mut seen = HashSet::new();
    networks.retain(|n| seen.insert(n["ssid"].to_string()));
    Ok(Value::Array(networks))
}

/// Connects to `ssid`. A saved network reuses its saved settings; otherwise a
/// new one is created with `password` (empty for open networks).
pub fn connect(ssid: &str, password: &str) -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let device = wifi_device(&bus)?;
    let root = OwnedObjectPath::try_from("/").map_err(err)?;

    if let Some(saved) = saved(&bus)?.remove(ssid) {
        let _: OwnedObjectPath = proxy(&bus, NM_PATH, NM)?
            .call("ActivateConnection", &(&saved, &device, &root))
            .map_err(err)?;
        return Ok(Value::Null);
    }

    let mut settings: HashMap<&str, HashMap<&str, Variant>> = HashMap::new();
    settings.insert(
        "802-11-wireless",
        HashMap::from([("ssid", Variant::from(ssid.as_bytes().to_vec()))]),
    );
    if !password.is_empty() {
        settings.insert(
            "802-11-wireless-security",
            HashMap::from([
                ("key-mgmt", Variant::from("wpa-psk")),
                ("psk", Variant::from(password)),
            ]),
        );
    }

    let _: (OwnedObjectPath, OwnedObjectPath) = proxy(&bus, NM_PATH, NM)?
        .call("AddAndActivateConnection", &(settings, &device, &root))
        .map_err(err)?;
    Ok(Value::Null)
}
