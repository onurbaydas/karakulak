//! Layers the compositor draws over the windows: brightness dimming, night
//! light, and a small level bar (OSD) when volume or brightness keys are used.

use std::process::Command;
use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::element::{
        Kind,
        solid::{SolidColorBuffer, SolidColorRenderElement},
    },
    output::Output,
};

use crate::State;

const OSD_WIDTH: i32 = 280;
const OSD_HEIGHT: i32 = 8;
const OSD_FROM_BOTTOM: i32 = 120;
const OSD_TIME: Duration = Duration::from_millis(1500);

/// Warm orange laid over the screen in the evening.
const NIGHT_COLOR: [f32; 4] = [0.18, 0.08, 0.0, 0.18];

pub struct Osd {
    level: f64,
    until: Instant,
    /// Shown in red, e.g. muted sound or a low battery.
    alert: bool,
    track: SolidColorBuffer,
    fill: SolidColorBuffer,
}

#[derive(Default)]
pub struct Overlays {
    pub osd: Option<Osd>,
    pub night_light: bool,
    night: std::collections::HashMap<String, SolidColorBuffer>,
}

impl State {
    /// Shows the level bar on the primary screen for a moment.
    pub fn show_osd(&mut self, level: f64, alert: bool) {
        let osd = self.overlays.osd.get_or_insert_with(|| Osd {
            level: 0.0,
            until: Instant::now(),
            alert: false,
            track: SolidColorBuffer::default(),
            fill: SolidColorBuffer::default(),
        });
        osd.level = level.clamp(0.0, 1.0);
        osd.alert = alert;
        osd.until = Instant::now() + OSD_TIME;
    }

    /// Everything drawn over the windows of `output`, front first.
    pub fn overlays(&mut self, output: &Output) -> Vec<SolidColorRenderElement> {
        let Some(size) = self.space.output_geometry(output).map(|g| g.size) else {
            return Vec::new();
        };
        let scale = output.current_scale().fractional_scale();
        let mut elements = Vec::new();

        let is_primary = self.primary_output().as_ref() == Some(output);
        if let Some(osd) = self.overlays.osd.as_mut().filter(|o| is_primary && o.until > Instant::now()) {
            let x = (size.w - OSD_WIDTH) / 2;
            let y = size.h - OSD_FROM_BOTTOM;
            let fill_color = if osd.alert { [0.95, 0.46, 0.42, 1.0] } else { [0.78, 0.51, 0.25, 1.0] };
            osd.fill.update(((OSD_WIDTH as f64 * osd.level) as i32, OSD_HEIGHT), fill_color);
            osd.track.update((OSD_WIDTH, OSD_HEIGHT), [0.13, 0.12, 0.10, 0.9]);
            let at = |x: i32, y: i32| smithay::utils::Point::<i32, smithay::utils::Logical>::from((x, y)).to_physical_precise_round(scale);
            elements.push(SolidColorRenderElement::from_buffer(&osd.fill, at(x, y), scale, 1.0, Kind::Unspecified));
            elements.push(SolidColorRenderElement::from_buffer(&osd.track, at(x, y), scale, 1.0, Kind::Unspecified));
        }
        elements.extend(self.dimmer(output));
        if self.overlays.night_light {
            let buffer = self.overlays.night.entry(output.name()).or_default();
            buffer.update(size, NIGHT_COLOR);
            elements.push(SolidColorRenderElement::from_buffer(buffer, (0, 0), scale, 1.0, Kind::Unspecified));
        }
        elements
    }

    /// Volume keys: `Some(step)` changes the level, `None` toggles mute.
    pub fn change_volume(&mut self, step: Option<i32>) {
        const SINK: &str = "@DEFAULT_AUDIO_SINK@";
        let args: Vec<String> = match step {
            Some(step) => {
                let change = format!("{}%{}", step.abs(), if step > 0 { "+" } else { "-" });
                vec!["set-volume".into(), "-l".into(), "1.0".into(), SINK.into(), change]
            }
            None => vec!["set-mute".into(), SINK.into(), "toggle".into()],
        };
        let _ = Command::new("wpctl").args(&args).status();
        // `Volume: 0.40 [MUTED]`
        let Ok(output) = Command::new("wpctl").args(["get-volume", SINK]).output() else {
            return;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let level = text.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        self.show_osd(level, text.contains("[MUTED]"));
    }

    /// Brightness keys change the primary (built-in) screen.
    pub fn change_brightness(&mut self, step: f64) {
        let Some(output) = self.primary_output() else { return };
        let current = self.dimmers.get(&output.name()).map_or(1.0, |(level, _)| *level);
        self.set_brightness(&output.name(), current + step);
        let level = self.dimmers.get(&output.name()).map_or(1.0, |(level, _)| *level);
        self.show_osd(level, false);
    }
}
