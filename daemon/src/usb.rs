//! USB sticks and other removable disks through udisks2, the service every
//! Linux desktop uses. Mounting needs no root for the active local user.

use serde_json::{Value, json};
use std::collections::HashMap;
use karakulak_dbus::{Connection, Proxy};
use karakulak_dbus::{OwnedObjectPath, OwnedVariant, Variant};

const UDISKS: &str = "org.freedesktop.UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedVariant>>>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn prop<T: TryFrom<OwnedVariant>>(props: &HashMap<String, OwnedVariant>, name: &str) -> Option<T> {
    props.get(name)?.try_clone().ok()?.try_into().ok()
}

fn objects(bus: &Connection) -> Result<Objects, String> {
    Proxy::new(bus, UDISKS, "/org/freedesktop/UDisks2", "org.freedesktop.DBus.ObjectManager")
        .and_then(|manager| manager.call("GetManagedObjects", &()))
        .map_err(err)
}

/// udisks keeps mount points as NUL-terminated byte strings.
fn mount_points(props: &HashMap<String, OwnedVariant>) -> Vec<String> {
    prop::<Vec<Vec<u8>>>(props, "MountPoints")
        .unwrap_or_default()
        .into_iter()
        .map(|p| String::from_utf8_lossy(p.strip_suffix(&[0]).unwrap_or(&p)).into_owned())
        .collect()
}

/// Filesystems on removable drives:
/// `[{"id", "name", "size", "mounted": path or null}]`
pub fn list() -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let objects = objects(&bus)?;
    let mut disks = Vec::new();
    for (path, interfaces) in &objects {
        let (Some(block), Some(filesystem)) = (interfaces.get(BLOCK), interfaces.get(FILESYSTEM)) else {
            continue;
        };
        if prop::<bool>(block, "HintIgnore").unwrap_or(false) {
            continue;
        }
        let drive = prop::<OwnedObjectPath>(block, "Drive").and_then(|d| objects.get(&d)?.get(DRIVE));
        let removable = drive.is_some_and(|d| {
            prop::<bool>(d, "Removable").unwrap_or(false) || prop::<String>(d, "ConnectionBus").as_deref() == Some("usb")
        });
        if !removable {
            continue;
        }
        let label = prop::<String>(block, "IdLabel").filter(|l| !l.is_empty());
        let size = prop::<u64>(block, "Size").unwrap_or(0);
        let name = label.unwrap_or_else(|| format!("USB bellek ({} GB)", size / 1_000_000_000));
        disks.push(json!({
            "id": path.as_str(),
            "name": name,
            "size": size,
            "mounted": mount_points(filesystem).into_iter().next(),
        }));
    }
    Ok(Value::Array(disks))
}

fn checked(id: &str) -> Result<&str, String> {
    if id.starts_with("/org/freedesktop/UDisks2/block_devices/") {
        Ok(id)
    } else {
        Err("not a disk".into())
    }
}

/// Mounts (if needed) and returns the folder: `{"path"}`.
pub fn mount(id: &str) -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let fs = Proxy::new(&bus, UDISKS, checked(id)?, FILESYSTEM).map_err(err)?;
    let mounted: Vec<Vec<u8>> = fs.get_property("MountPoints").map_err(err)?;
    let path = match mounted.first() {
        Some(p) => String::from_utf8_lossy(p.strip_suffix(&[0]).unwrap_or(p)).into_owned(),
        None => fs
            .call("Mount", &(HashMap::<&str, Variant>::new(),))
            .map_err(err)?,
    };
    Ok(json!({ "path": path }))
}

/// Unmounts, then powers the stick off so it is safe to pull out.
pub fn eject(id: &str) -> Result<Value, String> {
    let bus = Connection::system().map_err(err)?;
    let no_options = HashMap::<&str, Variant>::new();
    let fs = Proxy::new(&bus, UDISKS, checked(id)?, FILESYSTEM).map_err(err)?;
    let mounted: Vec<Vec<u8>> = fs.get_property("MountPoints").map_err(err)?;
    if !mounted.is_empty() {
        fs.call_method("Unmount", &(&no_options,)).map_err(err)?;
    }
    let block = Proxy::new(&bus, UDISKS, id, BLOCK).map_err(err)?;
    let drive: OwnedObjectPath = block.get_property("Drive").map_err(err)?;
    let drive = Proxy::new(&bus, UDISKS, drive.as_str(), DRIVE).map_err(err)?;
    if drive.get_property::<bool>("CanPowerOff").unwrap_or(false) {
        // Other partitions on the same stick may still be mounted; then the
        // stick stays on but this partition is safe.
        let _ = drive.call_method("PowerOff", &(&no_options,));
    }
    list()
}
