//! Keyboard layout from the system setting (localed), which Settings changes.

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
    }
}
