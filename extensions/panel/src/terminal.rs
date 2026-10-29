//! Terminal page (terminal.html). karakulakd runs the shell in a
//! pseudo-terminal over a long-lived native port; vt100 turns its output into
//! a screen of styled cells, which is drawn as text.

use std::cell::RefCell;
use std::num::NonZeroU16;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use js_sys::{Function, Promise};
use karakulak_browser::{call, get, object};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
use wasm_bindgen_futures::JsFuture;

/// Character cell size in CSS pixels for the 14px monospace font.
const CELL_W: f64 = 8.43;
const CELL_H: f64 = 18.0;
const SCROLLBACK: usize = 5000;

thread_local! {
    static PORT: RefCell<Option<JsValue>> = const { RefCell::new(None) };
    static TERM: RefCell<Option<u64>> = const { RefCell::new(None) };
    static PARSER: RefCell<vt100::Parser> = RefCell::new(vt100::Parser::new(nz(24), nz(80), SCROLLBACK));
}

fn nz(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n.max(1)).unwrap()
}

fn post(message: JsValue) {
    PORT.with_borrow(|port| {
        if let Some(port) = port
            && let Ok(post) = get(port, "postMessage").dyn_into::<Function>()
        {
            let _ = post.call1(port, &message);
        }
    });
}

fn send_input(data: &str) {
    if let Some(term) = TERM.with_borrow(|t| *t) {
        post(object(&[("cmd", "term-input".into()), ("term", (term as f64).into()), ("data", data.into())]));
    }
}

/// Rows and columns that fit the window.
fn fit() -> (u16, u16) {
    let px = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
    let w = px(window().inner_width()) - 16.0;
    let h = px(window().inner_height()) - 16.0;
    (((h / CELL_H).floor() as u16).max(2), ((w / CELL_W).floor() as u16).max(10))
}

const BASE16: [&str; 16] = [
    "#1d1a16", "#cc3e28", "#5f9e3a", "#d7a032", "#3f74b4", "#a35bb1", "#3a9e9a", "#d8d2c8", "#6b645b", "#f2766b",
    "#8cc66a", "#f0c35a", "#6fa0e0", "#c98bd6", "#6cc9c4", "#fbf8f2",
];

fn color(c: vt100::Color, fallback: &str) -> String {
    match c {
        vt100::Color::Default => fallback.into(),
        vt100::Color::Idx(i @ 0..=15) => BASE16[i as usize].into(),
        vt100::Color::Idx(i @ 16..=231) => {
            let level = |v: u8| if v == 0 { 0 } else { 55 + v as u16 * 40 };
            let i = i - 16;
            format!("rgb({},{},{})", level(i / 36), level(i / 6 % 6), level(i % 6))
        }
        vt100::Color::Idx(i) => {
            let v = 8 + (i as u16 - 232) * 10;
            format!("rgb({v},{v},{v})")
        }
        vt100::Color::Rgb(r, g, b) => format!("rgb({r},{g},{b})"),
    }
}

/// One line as runs of equally styled text: `(text, css)`.
type Line = Vec<(String, String)>;

fn snapshot() -> Vec<Line> {
    PARSER.with_borrow(|parser| {
        let screen = parser.screen();
        let (rows, cols) = screen.size();
        let cursor = (!screen.hide_cursor()).then(|| screen.cursor_position());
        (0..rows.get())
            .map(|row| {
                let mut line: Line = Vec::new();
                for col in 0..cols.get() {
                    let Some(cell) = screen.cell(row, col) else { continue };
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    let mut fg = color(cell.fgcolor(), "var(--fg)");
                    let mut bg = color(cell.bgcolor(), "transparent");
                    if cell.inverse() != (cursor == Some((row, col))) {
                        (fg, bg) = (if bg == "transparent" { "var(--bg)".into() } else { bg }, fg);
                    }
                    let mut css = format!("color:{fg};background:{bg}");
                    if cell.bold() {
                        css.push_str(";font-weight:bold");
                    }
                    if cell.italic() {
                        css.push_str(";font-style:italic");
                    }
                    if cell.underline() {
                        css.push_str(";text-decoration:underline");
                    }
                    let text = if cell.has_contents() { cell.contents() } else { " " };
                    match line.last_mut() {
                        Some((run, run_css)) if *run_css == css => run.push_str(text),
                        _ => line.push((text.to_string(), css)),
                    }
                }
                line
            })
            .collect()
    })
}

/// What a key press sends to the shell.
fn key_input(ev: &leptos::ev::KeyboardEvent) -> Option<String> {
    let app = PARSER.with_borrow(|p| p.screen().application_cursor());
    let arrow = |c: char| Some(if app { format!("\x1bO{c}") } else { format!("\x1b[{c}") });
    let key = ev.key();
    if ev.ctrl_key() && !ev.alt_key() && key.chars().count() == 1 {
        let c = key.chars().next()?.to_ascii_lowercase();
        return match c {
            'a'..='z' => Some(((c as u8 - b'a' + 1) as char).to_string()),
            '[' => Some("\x1b".into()),
            ' ' => Some("\0".into()),
            _ => None,
        };
    }
    let text = match key.as_str() {
        "Enter" => "\r".into(),
        "Backspace" => "\x7f".into(),
        "Tab" => "\t".into(),
        "Escape" => "\x1b".into(),
        "ArrowUp" => return arrow('A'),
        "ArrowDown" => return arrow('B'),
        "ArrowRight" => return arrow('C'),
        "ArrowLeft" => return arrow('D'),
        "Home" => "\x1b[H".into(),
        "End" => "\x1b[F".into(),
        "PageUp" => "\x1b[5~".into(),
        "PageDown" => "\x1b[6~".into(),
        "Delete" => "\x1b[3~".into(),
        "Insert" => "\x1b[2~".into(),
        k if k.chars().count() == 1 => k.into(),
        _ => return None,
    };
    Some(if ev.alt_key() { format!("\x1b{text}") } else { text })
}

/// Calls `navigator.clipboard.<method>(...)`.
async fn clipboard(method: &str, args: &[JsValue]) -> Option<JsValue> {
    let clipboard = get(&get(&js_sys::global(), "navigator"), "clipboard");
    let f: Function = get(&clipboard, method).dyn_into().ok()?;
    let promise: Promise = f.apply(&clipboard, &args.iter().collect()).ok()?.dyn_into().ok()?;
    JsFuture::from(promise).await.ok()
}

#[component]
pub fn Terminal() -> impl IntoView {
    let lines = RwSignal::new(Vec::<Line>::new());
    let (ended, set_ended) = signal(false);

    let on_message = Closure::<dyn FnMut(JsValue)>::new(move |msg: JsValue| match get(&msg, "event").as_string().as_deref() {
        Some("output") => {
            if let Some(data) = get(&msg, "data").as_string()
                && let Ok(bytes) = BASE64.decode(data)
            {
                PARSER.with_borrow_mut(|p| p.process(&bytes));
                lines.set(snapshot());
            }
        }
        Some("exit") => set_ended.set(true),
        _ => {
            if let Some(term) = get(&get(&msg, "result"), "term").as_f64() {
                TERM.with_borrow_mut(|t| *t = Some(term as u64));
            }
        }
    });

    spawn_local(async move {
        let Ok(port) = call("runtime.connectNative", &["karakulakd".into()]).await else {
            set_ended.set(true);
            return;
        };
        let on_message_event = get(&port, "onMessage");
        if let Ok(add) = get(&on_message_event, "addListener").dyn_into::<Function>() {
            let _ = add.call1(&on_message_event, on_message.as_ref());
        }
        on_message.forget();
        PORT.with_borrow_mut(|p| *p = Some(port));

        let (rows, cols) = fit();
        PARSER.with_borrow_mut(|p| p.screen_mut().set_size(nz(rows), nz(cols)));
        post(object(&[
            ("cmd", "term-open".into()),
            ("rows", f64::from(rows).into()),
            ("cols", f64::from(cols).into()),
        ]));
    });

    let resize = window_event_listener(leptos::ev::resize, move |_| {
        let (rows, cols) = fit();
        PARSER.with_borrow_mut(|p| p.screen_mut().set_size(nz(rows), nz(cols)));
        lines.set(snapshot());
        if let Some(term) = TERM.with_borrow(|t| *t) {
            post(object(&[
                ("cmd", "term-resize".into()),
                ("term", (term as f64).into()),
                ("rows", f64::from(rows).into()),
                ("cols", f64::from(cols).into()),
            ]));
        }
    });
    on_cleanup(move || resize.remove());

    let keydown = move |ev: leptos::ev::KeyboardEvent| {
        if ev.ctrl_key() && ev.shift_key() {
            match ev.key().to_lowercase().as_str() {
                "c" => {
                    ev.prevent_default();
                    let selection = get(&js_sys::global(), "getSelection")
                        .dyn_into::<Function>()
                        .ok()
                        .and_then(|f| f.call0(&js_sys::global()).ok());
                    let selected = selection.map(|s| String::from(js_sys::Object::from(s).to_string()));
                    if let Some(text) = selected {
                        spawn_local(async move {
                            clipboard("writeText", &[text.into()]).await;
                        });
                    }
                    return;
                }
                "v" => {
                    ev.prevent_default();
                    spawn_local(async move {
                        if let Some(text) = clipboard("readText", &[]).await.and_then(|t| t.as_string()) {
                            send_input(&text);
                        }
                    });
                    return;
                }
                _ => {}
            }
        }
        if let Some(input) = key_input(&ev) {
            ev.prevent_default();
            send_input(&input);
        }
    };

    view! {
        <pre class="terminal" tabindex="0" autofocus on:keydown=keydown>
            {move || {
                lines
                    .get()
                    .into_iter()
                    .map(|line| {
                        let runs = line
                            .into_iter()
                            .map(|(text, css)| view! { <span style=css>{text}</span> })
                            .collect_view();
                        view! { <div>{runs}</div> }
                    })
                    .collect_view()
            }}
        </pre>
        <Show when=move || ended.get()>
            <p class="ended">"Oturum kapandı. Yeni bir terminal için sekmeyi yenile."</p>
        </Show>
    }
}
