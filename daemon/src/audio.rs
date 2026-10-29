//! Volume of the default output, through PipeWire's `wpctl`.

use serde_json::{json, Value};
use std::process::Command;

const SINK: &str = "@DEFAULT_AUDIO_SINK@";

fn wpctl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("wpctl")
        .args(args)
        .output()
        .map_err(|e| format!("wpctl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `{"level": 0.0..=1.0, "muted": bool}`
pub fn get() -> Result<Value, String> {
    parse(&wpctl(&["get-volume", SINK])?)
}

/// Parses `wpctl get-volume` output, e.g. `Volume: 0.40 [MUTED]`.
fn parse(output: &str) -> Result<Value, String> {
    let level: f64 = output
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("unexpected wpctl output: {output}"))?;
    Ok(json!({ "level": level, "muted": output.contains("[MUTED]") }))
}

pub fn set(level: f64) -> Result<Value, String> {
    let level = level.clamp(0.0, 1.0).to_string();
    wpctl(&["set-volume", SINK, &level])?;
    get()
}

pub fn toggle_mute() -> Result<Value, String> {
    wpctl(&["set-mute", SINK, "toggle"])?;
    get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_volume() {
        assert_eq!(parse("Volume: 0.40\n").unwrap(), json!({ "level": 0.4, "muted": false }));
        assert_eq!(parse("Volume: 1.00 [MUTED]\n").unwrap(), json!({ "level": 1.0, "muted": true }));
    }
}
