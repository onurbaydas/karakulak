//! Problem report: system facts and the compositor's recent log in one text
//! file in Downloads. Nothing is sent anywhere; the page shows the text so the
//! user can read it before attaching it to an issue.

use serde_json::{Value, json};
use std::process::Command;

fn run(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|e| format!("({program}: {e})"))
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// `{"path", "text"}`
pub fn create() -> Result<Value, String> {
    let os = read("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
        .unwrap_or_default();
    let cpu = read("/proc/cpuinfo")
        .lines()
        .find_map(|l| l.strip_prefix("model name").and_then(|v| v.split_once(':')).map(|(_, v)| v.trim().to_string()))
        .unwrap_or_default();
    let memory = read("/proc/meminfo").lines().next().unwrap_or_default().to_string();
    let gpus: Vec<String> = run("lspci", &[])
        .lines()
        .filter(|l| l.contains("VGA") || l.contains("3D") || l.contains("Display"))
        .map(String::from)
        .collect();
    let karakulak = run("dpkg-query", &["-W", "-f=${Version}", "karakulak"]);
    let firefox = run("firefox", &["--version"]);
    let log = run("journalctl", &["--user", "-t", "karakulak-compositor", "-n", "300", "--no-pager", "-o", "short-iso"]);
    let log = if log.is_empty() {
        run("journalctl", &["-t", "karakulak-compositor", "-n", "300", "--no-pager", "-o", "short-iso"])
    } else {
        log
    };

    let text = format!(
        "Karakulak problem report\n\n\
         Karakulak: {karakulak}\nSystem: {os}\nKernel: {kernel}\nFirefox: {firefox}\nCPU: {cpu}\n{memory}\nGPU:\n{gpus}\n\n\
         Compositor log (last 300 lines):\n{log}\n",
        kernel = run("uname", &["-r"]),
        gpus = gpus.join("\n"),
    );

    let folder = dirs::download_dir().or_else(dirs::home_dir).ok_or("no folder to save to")?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join(format!("karakulak-report-{}.txt", crate::backup::stamp()));
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    Ok(json!({ "path": path, "text": text }))
}
