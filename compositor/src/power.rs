//! Power: dim and lock when idle (unless a page plays video), warn on low
//! battery and suspend before it runs out, night light on a schedule.
//! Runs from a timer every few seconds.

use std::time::{Duration, Instant};

use smithay::{
    reexports::{calloop::{EventLoop, timer::{TimeoutAction, Timer}}, wayland_server::protocol::wl_surface::WlSurface},
    utils::IsAlive,
    wayland::idle_inhibit::IdleInhibitHandler,
};

use crate::State;

const TICK: Duration = Duration::from_secs(10);
const DIM_AFTER: Duration = Duration::from_secs(5 * 60);
const LOCK_AFTER: Duration = Duration::from_secs(10 * 60);
const SCREENS_OFF_AFTER: Duration = Duration::from_secs(11 * 60);
const IDLE_DIM_LEVEL: f64 = 0.3;
const BATTERY_WARN: u8 = 10;
const BATTERY_SUSPEND: u8 = 4;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum NightMode {
    #[default]
    Off,
    On,
    /// From 20:00 to 07:00.
    Evening,
}

pub struct Power {
    pub last_input: Instant,
    /// Surfaces (video players) asking the screen to stay on.
    inhibitors: Vec<WlSurface>,
    pub idle_dimmed: bool,
    /// Screens powered off (DPMS) after a long idle time.
    pub screens_off: bool,
    battery_warned_at: Option<Instant>,
    suspended_for_battery: bool,
    pub night_mode: NightMode,
}

fn night_mode_file() -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("karakulak/night-light"))
}

impl NightMode {
    pub fn name(self) -> &'static str {
        match self {
            NightMode::Off => "off",
            NightMode::On => "on",
            NightMode::Evening => "evening",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            "off" => Some(NightMode::Off),
            "on" => Some(NightMode::On),
            "evening" => Some(NightMode::Evening),
            _ => None,
        }
    }

    pub fn save(self) {
        if let Some(path) = night_mode_file() {
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let _ = std::fs::write(path, self.name());
        }
    }
}

impl Default for Power {
    fn default() -> Self {
        let night_mode = night_mode_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| NightMode::from_name(&s))
            .unwrap_or_default();
        Self {
            last_input: Instant::now(),
            inhibitors: Vec::new(),
            idle_dimmed: false,
            screens_off: false,
            battery_warned_at: None,
            suspended_for_battery: false,
            night_mode,
        }
    }
}

impl IdleInhibitHandler for State {
    fn inhibit(&mut self, surface: WlSurface) {
        self.power.inhibitors.push(surface);
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.power.inhibitors.retain(|s| *s != surface);
    }
}

pub fn init(event_loop: &mut EventLoop<'static, State>) {
    event_loop
        .handle()
        .insert_source(Timer::from_duration(TICK), |_, _, state| {
            state.power_tick();
            TimeoutAction::ToDuration(TICK)
        })
        .expect("power timer");
}

/// First battery: `(percent, discharging)`.
fn battery() -> Option<(u8, bool)> {
    for entry in std::fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        let read = |name: &str| std::fs::read_to_string(entry.path().join(name)).ok().map(|s| s.trim().to_string());
        if read("type").as_deref() == Some("Battery") {
            let capacity = read("capacity")?.parse().ok()?;
            return Some((capacity, read("status").as_deref() == Some("Discharging")));
        }
    }
    None
}

/// Local hour of the day, 0..24.
fn local_hour() -> i32 {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    tm.tm_hour
}

fn suspend() {
    let call = || -> zbus::Result<()> {
        let bus = zbus::blocking::Connection::system()?;
        let manager = zbus::blocking::Proxy::new(
            &bus,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )?;
        manager.call_method("Suspend", &(false,))?;
        Ok(())
    };
    if let Err(e) = call() {
        tracing::warn!("could not suspend: {e}");
    }
}

impl State {
    /// Any key or pointer movement.
    pub fn user_active(&mut self) {
        self.power.last_input = Instant::now();
        self.power.idle_dimmed = false;
        if self.power.screens_off {
            self.power.screens_off = false;
            self.screens_on();
        }
    }

    fn power_tick(&mut self) {
        self.power.inhibitors.retain(|s| s.alive());
        let idle = self.power.last_input.elapsed();
        if self.power.inhibitors.is_empty() {
            if idle >= LOCK_AFTER && !self.lock.locked && self.lock_on_idle() {
                self.lock();
            }
            self.power.idle_dimmed = idle >= DIM_AFTER;
            if idle >= SCREENS_OFF_AFTER && !self.power.screens_off {
                self.power.screens_off = true;
                self.screens_off();
            }
        } else {
            self.power.idle_dimmed = false;
        }

        self.overlays.night_light = match self.power.night_mode {
            NightMode::Off => false,
            NightMode::On => true,
            NightMode::Evening => {
                let hour = local_hour();
                !(7..20).contains(&hour)
            }
        };

        if let Some((percent, discharging)) = battery() {
            if !discharging {
                self.power.suspended_for_battery = false;
                self.power.battery_warned_at = None;
            } else if percent <= BATTERY_SUSPEND && !self.power.suspended_for_battery {
                self.power.suspended_for_battery = true;
                suspend();
            } else if percent <= BATTERY_WARN
                && self.power.battery_warned_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
            {
                self.power.battery_warned_at = Some(Instant::now());
                self.show_osd(f64::from(percent) / 100.0, true);
            }
        }
    }

    /// Idle locking follows startup locking (off on the live system).
    fn lock_on_idle(&self) -> bool {
        std::env::var("KARAKULAK_LOCK_AT_START").as_deref() != Ok("0")
    }

    /// Extra dimming while idle, on top of the brightness setting.
    pub fn idle_factor(&self) -> f64 {
        if self.power.idle_dimmed { IDLE_DIM_LEVEL } else { 1.0 }
    }
}
