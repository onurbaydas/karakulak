//! Karakulak's Wayland compositor. Every window is fullscreen; there is
//! nothing else to show. Started from Smithay's smallvil example (MIT,
//! Copyright (c) 2017 Victor Berger and Victoria Brekenfeld).
//!
//! Usage: karakulak-compositor -- firefox --no-remote

mod handlers;
mod input;
mod ipc;
mod state;
mod winit;

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
pub use state::State;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let mut event_loop: EventLoop<State> = EventLoop::try_new()?;
    let display: Display<State> = Display::new()?;
    let mut state = State::new(&mut event_loop, display);

    // For now the compositor runs as a window inside another desktop, which
    // is how it is developed and tested. Running on real screens comes next.
    winit::init_winit(&mut event_loop, &mut state)?;

    let control = ipc::init(&mut event_loop, &state.socket_name.to_string_lossy())?;

    // Child processes connect to us, not to the desktop we run inside, and
    // karakulakd finds our control socket.
    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
        std::env::set_var("KARAKULAK_SOCKET", &control);
    }
    spawn_session();

    event_loop.run(None, &mut state, |_| {})?;
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
