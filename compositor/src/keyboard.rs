//! Keyboard layout from the system setting (localed), which Settings changes.
//! Up to two layouts ("tr,us"); Super+Space switches between them.

use serde_json::{Value, json};
use smithay::input::keyboard::XkbConfig;

use crate::State;

/// `(layout, variant)` as set in localed, e.g. `("tr", "f")`.
pub fn system_layout() -> (String, String) {
    let read = || -> zbus::Result<(String, String)> {
        let bus = zbus::blocking::Connection::system()?;
        let locale = zbus::blocking::Proxy::new(
            &bus,
            "org.freedesktop.locale1",
            "/org/freedesktop/locale1",
            "org.freedesktop.locale1",
        )?;
        Ok((locale.get_property("X11Layout")?, locale.get_property("X11Variant")?))
    };
    read().unwrap_or_else(|e| {
        tracing::warn!("keyboard layout unknown, using the default: {e}");
        (String::new(), String::new())
    })
}

impl State {
    pub fn reload_keyboard(&mut self) {
        let (layout, variant) = system_layout();
        let config = XkbConfig {
            layout: &layout,
            variant: &variant,
            ..Default::default()
        };
        let keyboard = self.seat.get_keyboard().unwrap();
        if let Err(e) = keyboard.set_xkb_config(self, config) {
            tracing::warn!("could not load keyboard layout {layout} {variant}: {e:?}");
        }
        self.layouts = layout_names(&layout);
    }

    /// Super+Space: the next layout.
    pub fn next_layout(&mut self) {
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.with_xkb_state(self, |mut context| context.cycle_next_layout());
    }

    /// `{"layouts": ["tr", "us"], "current": 0}`
    pub fn layouts_json(&mut self) -> Value {
        let keyboard = self.seat.get_keyboard().unwrap();
        let current = keyboard.with_xkb_state(self, |context| context.xkb().lock().unwrap().active_layout().0);
        json!({ "layouts": self.layouts, "current": current })
    }
}

/// "tr,us" -> ["tr", "us"]; the system default when nothing is set.
pub fn layout_names(layout: &str) -> Vec<String> {
    let names: Vec<String> = layout.split(',').filter(|l| !l.is_empty()).map(String::from).collect();
    if names.is_empty() { vec!["us".into()] } else { names }
}
