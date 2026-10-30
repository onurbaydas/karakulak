//! Bluetooth through BlueZ's D-Bus API: power, paired devices, and pairing
//! new ones. While pairing, a BlueZ agent answers code requests: codes the
//! user has to type on the device (keyboards) are sent to the page as
//! `{"event": "bluetooth-code", "code": "123456"}` over the native port.

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

/// Searches for a few seconds and returns devices that are not paired yet:
/// `[{"path", "name"}]`. Discovery belongs to our D-Bus connection, so it
/// has to finish before this short-lived process exits.
pub fn scan() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let adapter_path = adapter(&objects(&bus)?).ok_or("no Bluetooth adapter")?.0.clone();
    let adapter = Proxy::new(&bus, BLUEZ, adapter_path.as_str(), ADAPTER).map_err(err)?;
    adapter.call_method("StartDiscovery", &()).map_err(err)?;
    std::thread::sleep(std::time::Duration::from_secs(8));
    let _ = adapter.call_method("StopDiscovery", &());

    let mut found: Vec<Value> = objects(&bus)?
        .iter()
        .filter_map(|(path, interfaces)| {
            let props = interfaces.get(DEVICE)?;
            if prop::<bool>(props, "Paired").unwrap_or(false) {
                return None;
            }
            // Unnamed devices are mostly beacons nobody wants to pair with.
            let name = prop::<String>(props, "Name")?;
            Some(json!({ "path": path.as_str(), "name": name }))
        })
        .collect();
    found.sort_by_key(|d| d["name"].to_string());
    Ok(Value::Array(found))
}

const AGENT_PATH: &str = "/org/karakulak/bluetooth_agent";

/// Shows a code the user has to type on the device being paired.
fn show_code(code: &str) {
    let _ = crate::emit(&json!({ "event": "bluetooth-code", "code": code }));
}

/// Six random digits, for keyboards that want a PIN typed on them.
fn random_pin() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 4];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    format!("{:06}", u32::from_ne_bytes(bytes) % 1_000_000)
}

/// Answers BlueZ during pairing. Pairing is started by the user from the
/// panel, so confirmations are accepted; the code is always shown.
struct Agent;

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {}

    fn request_pin_code(&self, _device: OwnedObjectPath) -> String {
        let pin = random_pin();
        show_code(&pin);
        pin
    }

    fn display_pin_code(&self, _device: OwnedObjectPath, pincode: String) {
        show_code(&pincode);
    }

    /// A code shown on the other device that we would have to type in: not
    /// supported, as nothing here can ask for it during pairing.
    fn request_passkey(&self, _device: OwnedObjectPath) -> zbus::fdo::Result<u32> {
        Err(zbus::fdo::Error::Failed("entering a code is not supported".into()))
    }

    fn display_passkey(&self, _device: OwnedObjectPath, passkey: u32, _entered: u16) {
        show_code(&format!("{passkey:06}"));
    }

    fn request_confirmation(&self, _device: OwnedObjectPath, passkey: u32) {
        show_code(&format!("{passkey:06}"));
    }

    fn request_authorization(&self, _device: OwnedObjectPath) {}

    fn authorize_service(&self, _device: OwnedObjectPath, _uuid: String) {}

    fn cancel(&self) {}
}

/// Pairs with a device found by `scan`, trusts it so it reconnects by itself,
/// and connects.
pub fn pair(path: &str) -> Result<Value, String> {
    if !path.starts_with("/org/bluez/") {
        return Err("not a Bluetooth device".into());
    }
    let bus = Connection::system().map_err(err)?;
    bus.object_server().at(AGENT_PATH, Agent).map_err(err)?;
    let manager = Proxy::new(&bus, BLUEZ, "/org/bluez", "org.bluez.AgentManager1").map_err(err)?;
    let agent = OwnedObjectPath::try_from(AGENT_PATH).map_err(err)?;
    manager
        .call_method("RegisterAgent", &(&agent, "KeyboardDisplay"))
        .map_err(err)?;
    let paired = Proxy::new(&bus, BLUEZ, path, DEVICE).and_then(|d| d.call_method("Pair", &()));
    let _ = manager.call_method("UnregisterAgent", &(&agent,));
    paired.map_err(err)?;
    Proxy::new(&bus, BLUEZ, path, "org.freedesktop.DBus.Properties")
        .and_then(|p| p.call_method("Set", &(DEVICE, "Trusted", Variant::from(true))))
        .map_err(err)?;
    set_connected(path, true)
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
