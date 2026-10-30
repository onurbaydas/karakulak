//! Screen settings (resolution, scale), remembered per monitor in
//! ~/.config/karakulak/screens.json and applied when it is connected.

use serde_json::{Value, json};
use smithay::output::{Output, Scale};
use std::path::PathBuf;

use crate::State;

/// A resolution: width, height, refresh rate in mHz.
pub type Mode = (i32, i32, i32);

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("karakulak/screens.json"))
}

/// Identifies a monitor across reboots and ports.
fn key(output: &Output) -> String {
    let p = output.physical_properties();
    format!("{} {} {}", p.make, p.model, p.serial_number)
}

fn load() -> Value {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

/// Scales offered: 100% to 300% in 25% steps.
pub fn valid_scale(scale: f64) -> f64 {
    ((scale * 4.0).round() / 4.0).clamp(1.0, 3.0)
}

/// The saved `(mode, scale)` for this monitor.
pub fn saved(output: &Output) -> (Option<Mode>, Option<f64>) {
    let all = load();
    let entry = &all[key(output)];
    let n = |name: &str| entry[name].as_i64().map(|v| v as i32);
    let mode = match (n("width"), n("height"), n("refresh")) {
        (Some(w), Some(h), Some(r)) => Some((w, h, r)),
        _ => None,
    };
    (mode, entry["scale"].as_f64().map(valid_scale))
}

/// Updates some fields of this monitor's saved settings.
fn update(output: &Output, fields: Value) {
    let Some(path) = config_path() else {
        return;
    };
    let mut all = load();
    let entry = &mut all[key(output)];
    if !entry.is_object() {
        *entry = json!({});
    }
    for (name, value) in fields.as_object().into_iter().flatten() {
        entry[name] = value.clone();
    }
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Err(e) = std::fs::write(&path, all.to_string()) {
        tracing::warn!("could not save screen settings: {e}");
    }
}

fn save(output: &Output, mode: Mode, scale: f64) {
    update(output, json!({ "width": mode.0, "height": mode.1, "refresh": mode.2, "scale": scale }));
}

/// Settings forced software brightness for this monitor.
pub fn software_brightness(output: &Output) -> bool {
    load()[key(output)]["software_brightness"].as_bool().unwrap_or(false)
}

pub fn set_software_brightness(output: &Output, on: bool) {
    update(output, json!({ "software_brightness": on }));
}

impl State {
    /// `[{"id", "name", "current": [w, h, mHz], "scale", "modes": [[w, h, mHz]]}]`
    pub fn screens_list(&self) -> Value {
        let screens: Vec<Value> = self
            .space
            .outputs()
            .map(|output| {
                let current = output.current_mode().map(|m| (m.size.w, m.size.h, m.refresh));
                let mut modes = self.output_modes(output);
                if modes.is_empty() {
                    modes.extend(current);
                }
                let p = output.physical_properties();
                json!({
                    "id": output.name(),
                    "name": format!("{} {}", p.make, p.model),
                    "current": current,
                    "scale": output.current_scale().fractional_scale(),
                    "modes": modes,
                })
            })
            .collect();
        Value::Array(screens)
    }

    /// The screen under the pointer, or the first one: the screen to share.
    pub fn pointer_screen(&self) -> Option<String> {
        let pos = self.seat.get_pointer()?.current_location();
        let output = self.space.output_under(pos).next().or_else(|| self.space.outputs().next())?;
        Some(output.name())
    }

    pub fn set_screen(&mut self, id: &str, mode: Mode, scale: f64) -> Result<(), String> {
        let output = self
            .space
            .outputs()
            .find(|o| o.name() == id)
            .cloned()
            .ok_or("no such screen")?;
        let current = output.current_mode().map(|m| (m.size.w, m.size.h, m.refresh));
        if current != Some(mode) {
            self.change_mode(&output, mode)?;
        }
        let scale = valid_scale(scale);
        output.change_current_state(None, None, Some(Scale::Fractional(scale)), None);
        save(&output, mode, scale);
        self.arrange_outputs();
        Ok(())
    }

    /// Modes the monitor offers; empty when running as a window.
    fn output_modes(&self, output: &Output) -> Vec<Mode> {
        if self.udev.is_some() { self.udev_modes(output) } else { Vec::new() }
    }

    fn change_mode(&mut self, output: &Output, mode: Mode) -> Result<(), String> {
        if self.udev.is_some() {
            self.udev_change_mode(output, mode)
        } else {
            Err("the window size decides the resolution here".into())
        }
    }

    fn arrange_outputs(&mut self) {
        if self.udev.is_some() {
            self.layout_outputs();
        } else {
            self.refit_windows();
        }
    }
}
