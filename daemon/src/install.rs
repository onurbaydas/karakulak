//! Installing from the live system: the disks to choose from, and the root
//! installer (src/bin/karakulak-install.rs) started through sudo, which the
//! live user may use without a password. It runs as a systemd unit, so it
//! keeps going when the page or this process goes away.

use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Command;

const STATUS: &str = "/run/karakulak-install.json";

pub fn is_live() -> bool {
    std::path::Path::new("/run/live/medium").exists()
}

/// The installer's progress, or null before it started.
fn status() -> Value {
    fs::read_to_string(STATUS)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

/// Whole disks not in use: the live medium, anything mounted and compressed
/// memory (zram) are left out.
fn disks() -> Result<Vec<Value>, String> {
    let output = Command::new("lsblk")
        .args(["-J", "-b", "-o", "NAME,TYPE,SIZE,MODEL,TRAN,MOUNTPOINTS"])
        .output()
        .map_err(|e| e.to_string())?;
    let tree: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    fn mounted(device: &Value) -> bool {
        device["mountpoints"].as_array().is_some_and(|m| m.iter().any(|p| !p.is_null()))
            || device["children"].as_array().is_some_and(|c| c.iter().any(mounted))
    }
    Ok(tree["blockdevices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["type"] == "disk" && !d["name"].as_str().unwrap_or("").starts_with("zram") && !mounted(d))
        .map(|d| {
            json!({
                "name": d["name"],
                "model": d["model"].as_str().unwrap_or("").trim(),
                "size": d["size"],
                "usb": d["tran"] == "usb",
            })
        })
        .collect())
}

/// `{"live", "disks": [{"name", "model", "size", "usb"}], "status"}`
pub fn info() -> Result<Value, String> {
    let disks = if is_live() { disks()? } else { vec![] };
    Ok(json!({ "live": is_live(), "disks": disks, "status": status() }))
}

/// Starts the installer with the user's choices; `info` shows the progress.
pub fn start(msg: &Value) -> Result<Value, String> {
    if !is_live() {
        return Err("Karakulak is already installed".into());
    }
    if status()["state"] == "running" {
        return Err("the installation is already running".into());
    }
    // The new user gets the live user's groups (sound, video, network...).
    let groups = Command::new("id")
        .arg("-Gn")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().map(String::from).collect::<Vec<_>>())
        .unwrap_or_default();
    let request = json!({
        "disk": msg["disk"],
        "user": msg["user"],
        "fullname": msg["fullname"],
        "password": msg["password"],
        "hostname": msg["hostname"],
        "groups": groups,
    });
    // The password is passed in a file only we (and root) can read.
    let dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is not set")?;
    let path = std::path::Path::new(&dir).join("karakulak-install-request.json");
    let _ = fs::remove_file(&path);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, request.to_string().as_bytes()))
        .map_err(|e| e.to_string())?;

    let output = Command::new("sudo")
        .args(["-n", "systemd-run", "--collect", "--unit=karakulak-install", "/usr/sbin/karakulak-install"])
        .arg(&path)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let _ = fs::remove_file(&path);
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    // Until the installer writes its first status.
    Ok(json!({ "live": true, "disks": [], "status": { "step": "start", "progress": 0.0, "state": "running" } }))
}

/// `karakulakd --install`: the same request as the page, as JSON on stdin,
/// then waits for the end. For the install test (tests/disk-install-test.sh).
pub fn from_command_line() -> i32 {
    let mut text = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text);
    let request: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if let Err(e) = start(&request) {
        println!("failed: {e}");
        return 1;
    }
    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let status = status();
        println!("{status}");
        match status["state"].as_str() {
            Some("done") => return 0,
            Some("failed") => return 1,
            _ => {}
        }
    }
}
