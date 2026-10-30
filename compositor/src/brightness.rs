//! Brightness per screen, hardware first:
//!
//! 1. Built-in panels: the kernel backlight, written through logind.
//! 2. External monitors: DDC/CI. The value is read back; a monitor that
//!    ignores it is switched to software.
//! 3. Software: the GPU's gamma table on real screens, a black layer when
//!    that is not available (or when running as a window).
//!
//! Some hardware accepts a value and does nothing (seen on a laptop whose only
//! backlight device belongs to the wrong GPU); for those, Settings can force
//! software per screen. Idle dimming and night light are always software.

use std::collections::HashMap;
use std::path::PathBuf;

use karakulak_ddc::Monitor;
use serde_json::{Value, json};
use smithay::{
    backend::renderer::element::{
        Kind,
        solid::{SolidColorBuffer, SolidColorRenderElement},
    },
    output::Output,
    reexports::calloop::{
        EventLoop,
        channel::{self, Event},
    },
};

use crate::State;

/// Darkest allowed level: a black screen looks like a dead machine.
const MIN_LEVEL: f64 = 0.1;
/// Night light: how much of each color channel is kept.
pub const NIGHT_CHANNELS: [f64; 3] = [1.0, 0.82, 0.62];

#[derive(Clone, Debug, PartialEq)]
pub enum Method {
    Backlight(String),
    Ddc,
    Software,
}

impl Method {
    fn name(&self) -> &'static str {
        match self {
            Method::Backlight(_) => "backlight",
            Method::Ddc => "ddc",
            Method::Software => "software",
        }
    }
}

pub struct Screen {
    pub level: f64,
    pub method: Method,
    /// Gamma state last written to the GPU: (software level, night light).
    pub gamma_applied: Option<(f64, bool)>,
    /// False when the GPU refused a gamma table; the black layer is used.
    pub gamma_works: bool,
    overlay: SolidColorBuffer,
}

enum Report {
    /// The monitor ignored a DDC/CI write.
    DdcIgnored(String),
    /// The monitor's level, read once at start.
    DdcLevel(String, f64),
}

#[derive(Default)]
pub struct Brightness {
    pub screens: HashMap<String, Screen>,
    reports: Option<channel::Sender<Report>>,
}

pub fn init(event_loop: &mut EventLoop<'static, State>, state: &mut State) {
    let (sender, receiver) = channel::channel();
    state.brightness.reports = Some(sender);
    event_loop
        .handle()
        .insert_source(receiver, |event, _, state| {
            let Event::Msg(report) = event else { return };
            match report {
                Report::DdcIgnored(name) => {
                    tracing::info!("{name} ignores DDC/CI brightness, using software");
                    if let Some(screen) = state.brightness.screens.get_mut(&name) {
                        screen.method = Method::Software;
                    }
                }
                Report::DdcLevel(name, level) => {
                    if let Some(screen) = state.brightness.screens.get_mut(&name) {
                        screen.level = level;
                    }
                }
            }
        })
        .expect("brightness reports");
}

fn is_builtin(output: &Output) -> bool {
    let name = output.name();
    name.starts_with("eDP") || name.starts_with("LVDS") || name.starts_with("DSI")
}

/// The backlight device, preferring firmware over platform over raw ones.
fn backlight() -> Option<PathBuf> {
    let mut devices: Vec<(u8, PathBuf)> = std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .map(|e| {
            let kind = std::fs::read_to_string(e.path().join("type")).unwrap_or_default();
            let rank = match kind.trim() {
                "firmware" => 0,
                "platform" => 1,
                _ => 2,
            };
            (rank, e.path())
        })
        .collect();
    devices.sort();
    devices.into_iter().next().map(|(_, path)| path)
}

fn read_number(path: PathBuf) -> Option<f64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn backlight_level(dir: &std::path::Path) -> Option<f64> {
    let current = read_number(dir.join("brightness"))?;
    let max = read_number(dir.join("max_brightness"))?.max(1.0);
    Some(current / max)
}

fn set_backlight(dir: &std::path::Path, level: f64) {
    let Some(max) = read_number(dir.join("max_brightness")) else { return };
    let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let value = (level * max).round() as u32;
    let call = || -> zbus::Result<()> {
        let bus = zbus::blocking::Connection::system()?;
        let session = zbus::blocking::Proxy::new(
            &bus,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
        )?;
        session.call_method("SetBrightness", &("backlight", name.as_str(), value))?;
        Ok(())
    };
    if let Err(e) = call() {
        tracing::warn!("could not set the backlight: {e}");
    }
}

/// The DDC/CI channel of this monitor.
fn ddc_monitor(connector: &str, model: &str) -> Option<Monitor> {
    Monitor::find(Some(connector), model)
}

impl State {
    fn screen(&mut self, output: &Output) -> &mut Screen {
        let name = output.name();
        if !self.brightness.screens.contains_key(&name) {
            let forced = crate::screens::software_brightness(output);
            let backlight = backlight().filter(|_| is_builtin(output) && !forced);
            let real_screens = self.udev.is_some();
            let (method, level) = match backlight {
                Some(dir) => {
                    let level = backlight_level(&dir).unwrap_or(1.0);
                    (Method::Backlight(dir.to_string_lossy().into_owned()), level)
                }
                None if real_screens && !forced && !is_builtin(output) => {
                    self.read_ddc_level(output);
                    (Method::Ddc, 1.0)
                }
                None => (Method::Software, 1.0),
            };
            self.brightness.screens.insert(
                name.clone(),
                Screen {
                    level,
                    method,
                    gamma_applied: None,
                    gamma_works: real_screens,
                    overlay: SolidColorBuffer::default(),
                },
            );
        }
        self.brightness.screens.get_mut(&name).unwrap()
    }

    fn read_ddc_level(&self, output: &Output) {
        let name = output.name();
        let model = output.physical_properties().model;
        let reports = self.brightness.reports.clone();
        std::thread::spawn(move || {
            let report = match ddc_monitor(&name, &model).and_then(|mut m| m.brightness()) {
                Some((value, max)) => Report::DdcLevel(name, f64::from(value) / f64::from(max.max(1))),
                None => Report::DdcIgnored(name),
            };
            if let Some(reports) = reports {
                let _ = reports.send(report);
            }
        });
    }

    /// `[{"id", "name", "level", "method", "software"}]` for every screen.
    pub fn brightness_list(&mut self) -> Value {
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();
        let screens: Vec<Value> = outputs
            .iter()
            .map(|output| {
                let props = output.physical_properties();
                let screen = self.screen(output);
                json!({
                    "id": output.name(),
                    "name": format!("{} {}", props.make, props.model),
                    "level": screen.level,
                    "method": screen.method.name(),
                    "software": crate::screens::software_brightness(output),
                })
            })
            .collect();
        Value::Array(screens)
    }

    pub fn set_brightness(&mut self, id: &str, level: f64) {
        let Some(output) = self.space.outputs().find(|o| o.name() == id).cloned() else {
            return;
        };
        let level = level.clamp(MIN_LEVEL, 1.0);
        let reports = self.brightness.reports.clone();
        let screen = self.screen(&output);
        screen.level = level;
        match screen.method.clone() {
            Method::Backlight(dir) => set_backlight(std::path::Path::new(&dir), level),
            Method::Ddc => {
                let name = output.name();
                let model = output.physical_properties().model;
                std::thread::spawn(move || {
                    let Some(mut monitor) = ddc_monitor(&name, &model) else { return };
                    let Some((_, max)) = monitor.brightness() else { return };
                    let value = (level * f64::from(max)).round() as u16;
                    monitor.set_brightness(value);
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    let read_back = monitor.brightness().map(|(v, _)| v);
                    if read_back != Some(value)
                        && let Some(reports) = reports
                    {
                        let _ = reports.send(Report::DdcIgnored(name));
                    }
                });
            }
            Method::Software => {}
        }
    }

    /// Settings: always dim this screen in software (hardware that ignores us).
    pub fn set_software_brightness(&mut self, id: &str, on: bool) {
        let Some(output) = self.space.outputs().find(|o| o.name() == id).cloned() else {
            return;
        };
        crate::screens::set_software_brightness(&output, on);
        let level = self.screen(&output).level;
        self.brightness.screens.remove(&output.name());
        self.screen(&output).level = level;
    }

    pub fn brightness_level(&mut self, output: &Output) -> f64 {
        self.screen(output).level
    }

    /// How far software dims this screen (brightness if done in software,
    /// times idle dimming), and whether night light is on.
    pub fn software_state(&mut self, output: &Output) -> (f64, bool) {
        let idle = self.idle_factor();
        let night = self.overlays.night_light;
        let screen = self.screen(output);
        let own = if screen.method == Method::Software { screen.level } else { 1.0 };
        (own * idle, night)
    }

    /// The black layer for screens without a working gamma table.
    pub(crate) fn dimmer(&mut self, output: &Output) -> Option<SolidColorRenderElement> {
        let size = self.space.output_geometry(output)?.size;
        let (level, _) = self.software_state(output);
        let screen = self.screen(output);
        if screen.gamma_works || level >= 1.0 {
            return None;
        }
        screen.overlay.update(size, [0.0, 0.0, 0.0, (1.0 - level) as f32]);
        Some(SolidColorRenderElement::from_buffer(
            &screen.overlay,
            (0, 0),
            output.current_scale().fractional_scale(),
            1.0,
            Kind::Unspecified,
        ))
    }

    /// Whether night light has to be drawn as a layer on this screen.
    pub fn night_layer_needed(&mut self, output: &Output) -> bool {
        self.overlays.night_light && !self.screen(output).gamma_works
    }
}

/// A gamma ramp of `size` entries for brightness `level` and night light.
pub fn gamma_ramp(size: usize, level: f64, night: bool) -> [Vec<u16>; 3] {
    let channel = |factor: f64| -> Vec<u16> {
        (0..size)
            .map(|i| {
                let x = i as f64 / (size.max(2) - 1) as f64;
                (x * level * factor * 65535.0).round().clamp(0.0, 65535.0) as u16
            })
            .collect()
    };
    let [r, g, b] = if night { NIGHT_CHANNELS } else { [1.0; 3] };
    [channel(r), channel(g), channel(b)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_scales_and_tints() {
        let [r, g, b] = gamma_ramp(256, 1.0, false);
        assert_eq!((r[0], r[255]), (0, 65535));
        assert_eq!(r, g);
        assert_eq!(g, b);
        let [r, _, b] = gamma_ramp(256, 0.5, true);
        assert_eq!(r[255], 32768);
        assert!(b[255] < r[255]);
    }
}
