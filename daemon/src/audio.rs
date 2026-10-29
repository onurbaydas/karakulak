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

fn pactl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("pactl").args(args).output().map_err(|e| format!("pactl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Level of the first channel, 0.0..=1.0.
fn first_channel_level(device: &Value) -> f64 {
    device["volume"]
        .as_object()
        .and_then(|channels| channels.values().next())
        .and_then(|c| c["value_percent"].as_str())
        .and_then(|p| p.trim_end_matches('%').parse::<f64>().ok())
        .map_or(0.0, |p| p / 100.0)
}

fn list_devices(kind: &str, default: &str) -> Result<Vec<Value>, String> {
    let all: Value = serde_json::from_str(&pactl(&["-f", "json", "list", kind])?).map_err(|e| e.to_string())?;
    Ok(all
        .as_array()
        .into_iter()
        .flatten()
        // Monitors replay an output; they are not microphones.
        .filter(|d| !d["name"].as_str().unwrap_or("").ends_with(".monitor"))
        .map(|d| {
            json!({
                "id": d["name"],
                "name": d["description"],
                "default": d["name"].as_str() == Some(default),
                "level": first_channel_level(d),
            })
        })
        .collect())
}

/// `{"outputs": [{"id", "name", "default", "level"}], "inputs": [...]}`
pub fn devices() -> Result<Value, String> {
    let sink = pactl(&["get-default-sink"])?;
    let source = pactl(&["get-default-source"])?;
    Ok(json!({
        "outputs": list_devices("sinks", &sink)?,
        "inputs": list_devices("sources", &source)?,
    }))
}

fn checked(id: &str) -> Result<&str, String> {
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || "._-:".contains(c)) {
        Ok(id)
    } else {
        Err("not a sound device".into())
    }
}

/// Playing sounds follow the new default by themselves.
pub fn set_output(id: &str) -> Result<Value, String> {
    pactl(&["set-default-sink", checked(id)?])?;
    devices()
}

pub fn set_input(id: &str) -> Result<Value, String> {
    pactl(&["set-default-source", checked(id)?])?;
    devices()
}

pub fn set_input_level(level: f64) -> Result<Value, String> {
    let percent = format!("{}%", (level.clamp(0.0, 1.0) * 100.0).round());
    pactl(&["set-source-volume", "@DEFAULT_SOURCE@", &percent])?;
    devices()
}
