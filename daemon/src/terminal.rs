//! Terminals for the terminal page, over a long-lived native messaging port
//! (`runtime.connectNative`). A shell runs in a pseudo-terminal; its output
//! is pushed as events, base64 because chunks may split UTF-8 characters:
//!
//!   {"event": "output", "term": id, "data": base64}
//!   {"event": "exit", "term": id}

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

struct Term {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    _child: Box<dyn Child + Send + Sync>,
}

fn terms() -> MutexGuard<'static, HashMap<u32, Term>> {
    static TERMS: OnceLock<Mutex<HashMap<u32, Term>>> = OnceLock::new();
    TERMS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn size(cols: u64, rows: u64) -> PtySize {
    PtySize {
        cols: cols.clamp(10, 1000) as u16,
        rows: rows.clamp(2, 500) as u16,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Starts the user's shell in the home folder: `{"term": id}`.
pub fn open(cols: u64, rows: u64) -> Result<Value, String> {
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);
    let pair = native_pty_system().openpty(size(cols, rows)).map_err(err)?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let mut command = CommandBuilder::new(shell);
    command.env("TERM", "xterm-256color");
    if let Some(home) = dirs::home_dir() {
        command.cwd(home);
    }
    let child = pair.slave.spawn_command(command).map_err(err)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(err)?;
    let writer = pair.master.take_writer().map_err(err)?;
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    terms().insert(
        id,
        Term {
            writer,
            master: pair.master,
            _child: child,
        },
    );

    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            let event = json!({ "event": "output", "term": id, "data": BASE64.encode(&buf[..n]) });
            if crate::emit(&event).is_err() {
                break;
            }
        }
        terms().remove(&id);
        let _ = crate::emit(&json!({ "event": "exit", "term": id }));
    });
    Ok(json!({ "term": id }))
}

/// Keys typed on the page.
pub fn input(id: u64, data: &str) -> Result<Value, String> {
    let mut terms = terms();
    let term = terms.get_mut(&(id as u32)).ok_or("no such terminal")?;
    term.writer.write_all(data.as_bytes()).map_err(err)?;
    term.writer.flush().map_err(err)?;
    Ok(Value::Null)
}

pub fn resize(id: u64, cols: u64, rows: u64) -> Result<Value, String> {
    let terms = terms();
    let term = terms.get(&(id as u32)).ok_or("no such terminal")?;
    term.master.resize(size(cols, rows)).map_err(err)?;
    Ok(Value::Null)
}
