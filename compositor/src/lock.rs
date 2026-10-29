//! Lock screen. While locked, every window is taken off the screens and only
//! the lock window is shown; input can only reach it. The lock window is a
//! separate kiosk Firefox with an empty profile (app id `karakulak-lock`)
//! showing the lock extension's page, started from KARAKULAK_LOCK_COMMAND.
//!
//! The password is checked here with PAM, so only the right password unlocks;
//! the lock page merely forwards it (lock page -> karakulakd -> control socket).
//! Locks at startup, before sleep and on request.

use std::os::unix::net::UnixStream;

use smithay::{
    desktop::Window,
    reexports::calloop::channel::{self, Channel},
    utils::IsAlive,
    wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData},
};

use crate::State;

pub const LOCK_APP_ID: &str = "karakulak-lock";

/// Work finished outside the event loop (PAM is slow on purpose after a wrong
/// password; logind signals arrive on their own thread).
pub enum LockEvent {
    /// Reply to the waiting client, unlock if the password was right.
    Checked { ok: bool, reply_to: UnixStream },
    /// The machine is about to sleep.
    Sleep,
}

#[derive(Default)]
pub struct Lock {
    pub locked: bool,
    pub window: Option<Window>,
    hidden: Vec<Window>,
    sender: Option<channel::Sender<LockEvent>>,
}

pub fn app_id(window: &Window) -> Option<String> {
    let surface = window.toplevel()?.wl_surface().clone();
    with_states(&surface, |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()?
            .lock()
            .ok()?
            .app_id
            .clone()
    })
}

/// Sets up locking if a lock command is configured; locks right away unless
/// KARAKULAK_LOCK_AT_START=0 (the live system, where nobody knows a password).
pub fn init(state: &mut State) -> Option<Channel<LockEvent>> {
    std::env::var_os("KARAKULAK_LOCK_COMMAND")?;
    let (sender, channel) = channel::channel();
    state.lock.sender = Some(sender.clone());
    watch_sleep(sender);
    spawn_lock_window();
    if std::env::var("KARAKULAK_LOCK_AT_START").as_deref() != Ok("0") {
        state.lock();
    }
    Some(channel)
}

fn spawn_lock_window() {
    let Ok(command) = std::env::var("KARAKULAK_LOCK_COMMAND") else {
        return;
    };
    if let Err(e) = std::process::Command::new("sh").arg("-c").arg(&command).spawn() {
        tracing::error!("could not start the lock screen: {e}");
    }
}

/// Locks before suspend, so the screen is locked when the machine wakes up.
fn watch_sleep(sender: channel::Sender<LockEvent>) {
    std::thread::spawn(move || {
        let watch = || -> zbus::Result<()> {
            let bus = zbus::blocking::Connection::system()?;
            let manager = zbus::blocking::Proxy::new(
                &bus,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
            )?;
            for signal in manager.receive_signal("PrepareForSleep")? {
                let (going_to_sleep,): (bool,) = signal.body().deserialize()?;
                if going_to_sleep && sender.send(LockEvent::Sleep).is_err() {
                    break;
                }
            }
            Ok(())
        };
        if let Err(e) = watch() {
            tracing::warn!("not locking before sleep: {e}");
        }
    });
}

/// Checks `password` for the session user on a worker thread.
pub fn check_password(state: &State, password: String, reply_to: UnixStream) {
    let Some(sender) = state.lock.sender.clone() else {
        return;
    };
    std::thread::spawn(move || {
        let service = std::env::var("KARAKULAK_PAM_SERVICE").unwrap_or_else(|_| "karakulak".into());
        let user = std::env::var("USER").unwrap_or_default();
        let ok = crate::pam::authenticate(&service, &user, &password);
        let _ = sender.send(LockEvent::Checked { ok, reply_to });
    });
}

impl State {
    pub fn handle_lock_event(&mut self, event: LockEvent) {
        match event {
            LockEvent::Checked { ok, mut reply_to } => {
                if ok {
                    self.unlock();
                }
                let reply = if ok {
                    serde_json::json!({ "ok": true, "result": null })
                } else {
                    serde_json::json!({ "ok": false, "error": "wrong password" })
                };
                let _ = std::io::Write::write_all(&mut reply_to, format!("{reply}\n").as_bytes());
            }
            LockEvent::Sleep => self.lock(),
        }
    }

    pub fn lock(&mut self) {
        if self.lock.locked || self.lock.sender.is_none() {
            return;
        }
        self.lock.locked = true;
        let windows: Vec<Window> = self.space.elements().cloned().collect();
        for window in windows {
            self.space.unmap_elem(&window);
            self.lock.hidden.push(window);
        }
        self.show_lock_window();
    }

    fn unlock(&mut self) {
        if !self.lock.locked {
            return;
        }
        self.lock.locked = false;
        if let Some(window) = self.lock.window.clone() {
            self.space.unmap_elem(&window);
        }
        for window in std::mem::take(&mut self.lock.hidden) {
            if window.alive() {
                self.place(window);
            }
        }
        self.fill_empty_screens();
    }

    /// A new window from the lock command.
    pub fn lock_window_created(&mut self, window: Window) {
        self.lock.window = Some(window);
        if self.lock.locked {
            self.show_lock_window();
        }
    }

    /// A normal window that showed up while locked waits until unlock.
    pub fn hide_while_locked(&mut self, window: Window) -> Option<Window> {
        if self.lock.locked {
            self.lock.hidden.push(window);
            return None;
        }
        Some(window)
    }

    fn show_lock_window(&mut self) {
        if let Some(window) = self.lock.window.clone() {
            self.place_on_primary(window);
        }
    }

    /// The lock window went away (crashed or closed): start a new one.
    pub fn lock_window_gone(&mut self) {
        self.lock.window = None;
        spawn_lock_window();
    }
}
