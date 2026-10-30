//! Karakulak's Wayland compositor. Every window is fullscreen; there is
//! nothing else to show. Started from Smithay's smallvil example (MIT,
//! Copyright (c) 2017 Victor Berger and Victoria Brekenfeld).
//!
//! Usage: karakulak-compositor -- firefox --no-remote

mod brightness;
mod cursor;
mod guest;
mod handlers;
mod input;
mod ipc;
mod keyboard;
mod layout;
mod lock;
mod overlay;
mod pam;
mod power;
mod screencopy;
mod screens;
mod screenshot;
mod state;
mod udev;
mod winit;

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
pub use state::State;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let mut event_loop: EventLoop<State> = EventLoop::try_new()?;
    let display: Display<State> = Display::new()?;
    let mut state = State::new(&mut event_loop, display);

    // Inside another desktop (development) we run as a window; started from a
    // text console we take over the real screens.
    let nested = std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some();
    if nested {
        winit::init_winit(&mut event_loop, &mut state)?;
    } else {
        udev::init_udev(&mut event_loop, &mut state)?;
    }

    let control = ipc::init(&mut event_loop, &state.socket_name.to_string_lossy())?;

    // Child processes connect to us, not to the desktop we run inside, and
    // karakulakd finds our control socket.
    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
        std::env::set_var("KARAKULAK_SOCKET", &control);
    }
    // Services started by D-Bus (the screen sharing portal) find our screens.
    // Not when running as a window: that would point the desktop's own
    // services at us.
    if !nested {
        let _ = std::process::Command::new("dbus-update-activation-environment")
            .args(["--systemd", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"])
            .spawn();
    }
    if let Some(lock_events) = lock::init(&mut state) {
        event_loop.handle().insert_source(lock_events, |event, _, state| {
            if let smithay::reexports::calloop::channel::Event::Msg(event) = event {
                state.handle_lock_event(event);
            }
        })?;
    }
    power::init(&mut event_loop);
    brightness::init(&mut event_loop, &mut state);
    spawn_session();

    event_loop.run(None, &mut state, |state| {
        state.space.refresh();
        state.popups.cleanup();
        let _ = state.display_handle.flush_clients();
    })?;
    Ok(())
}

fn init_logging() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }
}

/// Runs the command after `--`, or Firefox.
fn spawn_session() {
    let args: Vec<String> = std::env::args().skip_while(|a| a != "--").skip(1).collect();
    let (program, rest) = match args.split_first() {
        Some((program, rest)) => (program.as_str(), rest),
        None => ("firefox", &[][..]),
    };
    if let Err(e) = std::process::Command::new(program).args(rest).spawn() {
        tracing::error!("could not start {program}: {e}");
    }
}
