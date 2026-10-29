//! WireGuard VPNs through NetworkManager: import a provider's .conf file,
//! switch on and off, remove. nmcli does the parsing; the file only passes
//! through a private temporary folder.

use serde_json::{Value, json};
use std::process::Command;

fn nmcli(args: &[&str]) -> Result<String, String> {
    let output = Command::new("nmcli").args(args).output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// `[{"id", "name", "active"}]`
pub fn list() -> Result<Value, String> {
    let out = nmcli(&["-t", "-f", "NAME,UUID,TYPE,DEVICE", "connection", "show"])?;
    let vpns: Vec<Value> = out
        .lines()
        .filter_map(|line| {
            // nmcli escapes ':' inside fields as '\\:'.
            let fields: Vec<String> = line
                .replace("\\:", "\u{1}")
                .split(':')
                .map(|f| f.replace('\u{1}', ":"))
                .collect();
            let [name, uuid, kind, device] = fields.as_slice() else { return None };
            (kind == "wireguard").then(|| json!({ "id": uuid, "name": name, "active": !device.is_empty() }))
        })
        .collect();
    Ok(Value::Array(vpns))
}

/// Interface names are at most 15 characters: letters, digits, '-' and '_'.
fn interface_name(name: &str) -> String {
    let clean: String = name
        .trim_end_matches(".conf")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(15)
        .collect();
    if clean.is_empty() { "wg0".into() } else { clean }
}

pub fn import(name: &str, config: &str) -> Result<Value, String> {
    if !config.contains("[Interface]") || !config.contains("[Peer]") {
        return Err("not a WireGuard file".into());
    }
    let dir = tempfile_dir()?;
    let path = dir.join(format!("{}.conf", interface_name(name)));
    let written = std::fs::write(&path, config).map_err(|e| e.to_string());
    let result = written.and_then(|()| {
        nmcli(&["connection", "import", "type", "wireguard", "file", &path.to_string_lossy()])
    });
    let _ = std::fs::remove_dir_all(&dir);
    result?;
    list()
}

/// A folder only this user can read, for the key-holding file.
fn tempfile_dir() -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("karakulak-vpn-{}", std::process::id()));
    std::fs::DirBuilder::new().mode(0o700).create(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn checked(id: &str) -> Result<&str, String> {
    if id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        Ok(id)
    } else {
        Err("not a connection id".into())
    }
}

pub fn set_active(id: &str, on: bool) -> Result<Value, String> {
    nmcli(&["connection", if on { "up" } else { "down" }, "uuid", checked(id)?])?;
    list()
}

pub fn remove(id: &str) -> Result<Value, String> {
    nmcli(&["connection", "delete", "uuid", checked(id)?])?;
    list()
}

#[cfg(test)]
mod tests {
    #[test]
    fn interface_names_are_safe() {
        assert_eq!(super::interface_name("mullvad-se 12!.conf"), "mullvad-se12");
        assert_eq!(super::interface_name("a-very-long-provider-name"), "a-very-long-pro");
        assert_eq!(super::interface_name("../.."), "wg0");
    }
}
