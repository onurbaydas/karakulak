//! Control socket for karakulakd: one JSON request per line, one JSON reply
//! per line. The path is exported as KARAKULAK_SOCKET to everything we start.
//!
//!   {"cmd": "brightness"}                          -> [{"id", "name", "level"}]
//!   {"cmd": "set-brightness", "id": .., "level": ..} -> same list
//!   {"cmd": "set-software-brightness", "id": .., "on": bool} -> same list
//!   {"cmd": "screens"}                             -> [{"id", "name", "current", "scale", "modes"}]
//!   {"cmd": "set-screen", "id", "width", "height", "refresh", "scale"} -> same list
//!   {"cmd": "lock"}
//!   {"cmd": "unlock", "password": ..}              -> error if wrong

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};

use crate::State;

pub fn init(event_loop: &mut EventLoop<State>, socket_name: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let path = dir.join(format!("karakulak-{socket_name}.sock"));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;

    event_loop
        .handle()
        .insert_source(Generic::new(listener, Interest::READ, Mode::Level), |_, listener, state| {
            while let Ok((stream, _)) = listener.accept() {
                if let Err(e) = serve(state, stream) {
                    tracing::warn!("control socket: {e}");
                }
            }
            Ok(PostAction::Continue)
        })
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(path)
}

fn serve(state: &mut State, stream: UnixStream) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line).unwrap_or(Value::Null);

    let reply = match request["cmd"].as_str() {
        Some("screenshot") => {
            state.request_screenshot();
            json!({ "ok": true, "result": null })
        }
        Some("night-light") => {
            use crate::power::NightMode;
            if let Some(mode) = request["mode"].as_str().and_then(NightMode::from_name) {
                state.power.night_mode = mode;
                mode.save();
                state.overlays.night_light = mode == NightMode::On;
            }
            json!({ "ok": true, "result": { "mode": state.power.night_mode.name() } })
        }
        Some("reload-keyboard") => {
            state.reload_keyboard();
            json!({ "ok": true, "result": null })
        }
        Some("screens") => json!({ "ok": true, "result": state.screens_list() }),
        Some("set-screen") => {
            let n = |name: &str| request[name].as_i64().unwrap_or(0) as i32;
            let mode = (n("width"), n("height"), n("refresh"));
            let scale = request["scale"].as_f64().unwrap_or(1.0);
            match state.set_screen(request["id"].as_str().unwrap_or(""), mode, scale) {
                Ok(()) => json!({ "ok": true, "result": state.screens_list() }),
                Err(e) => json!({ "ok": false, "error": e }),
            }
        }
        Some("guest") => json!({ "ok": true, "result": { "active": state.guest_active() } }),
        Some("guest-start") => match state.start_guest() {
            Ok(()) => json!({ "ok": true, "result": null }),
            Err(e) => json!({ "ok": false, "error": e }),
        },
        Some("guest-end") => {
            state.end_guest();
            json!({ "ok": true, "result": null })
        }
        Some("lock") => {
            state.lock();
            json!({ "ok": true, "result": null })
        }
        Some("unlock") => {
            // Answered from the PAM worker once the password is checked.
            let password = request["password"].as_str().unwrap_or("").to_string();
            crate::lock::check_password(state, password, stream);
            return Ok(());
        }
        Some("brightness") => json!({ "ok": true, "result": state.brightness_list() }),
        Some("set-software-brightness") => {
            let id = request["id"].as_str().unwrap_or("");
            state.set_software_brightness(id, request["on"].as_bool().unwrap_or(false));
            json!({ "ok": true, "result": state.brightness_list() })
        }
        Some("set-brightness") => {
            let id = request["id"].as_str().unwrap_or("");
            let level = request["level"].as_f64().unwrap_or(1.0);
            state.set_brightness(id, level);
            json!({ "ok": true, "result": state.brightness_list() })
        }
        _ => json!({ "ok": false, "error": "unknown command" }),
    };
    let mut stream = stream;
    writeln!(stream, "{reply}")
}
