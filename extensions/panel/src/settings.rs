//! Settings page (settings.html). For now: screens (resolution, scale).

use leptos::prelude::*;
use serde::Deserialize;

use crate::run;

#[derive(Clone, Deserialize)]
struct Screen {
    id: String,
    name: String,
    current: Option<(i32, i32, i32)>,
    scale: i32,
    modes: Vec<(i32, i32, i32)>,
}

fn mode_text((w, h, mhz): (i32, i32, i32)) -> String {
    format!("{w} × {h}, {} Hz", (mhz as f64 / 1000.0).round())
}

#[component]
fn ScreenSettings(screen: Screen, screens: RwSignal<Vec<Screen>>, on_error: WriteSignal<String>) -> impl IntoView {
    let id = screen.id.clone();
    let apply = move |mode: (i32, i32, i32), scale: i32| {
        let args = vec![
            ("id", id.clone().into()),
            ("width", mode.0.into()),
            ("height", mode.1.into()),
            ("refresh", mode.2.into()),
            ("scale", scale.into()),
        ];
        run::<Vec<Screen>>("set-screen", args, on_error, move |s| screens.set(s.unwrap_or_default()));
    };
    let current = screen.current.unwrap_or_default();
    let scale = screen.scale;
    let modes = screen.modes.clone();
    let apply_mode = apply.clone();

    let options = screen
        .modes
        .iter()
        .enumerate()
        .map(|(i, mode)| {
            view! { <option value=i.to_string() selected=*mode == current>{mode_text(*mode)}</option> }
        })
        .collect_view();

    view! {
        <section class="card">
            <h2>{screen.name}</h2>
            <label>
                "Çözünürlük"
                <select on:change=move |ev| {
                    if let Some(mode) = event_target_value(&ev).parse::<usize>().ok().and_then(|i| modes.get(i)) {
                        apply_mode(*mode, scale);
                    }
                }>{options}</select>
            </label>
            <label>
                "Ölçek"
                <select on:change=move |ev| {
                    if let Ok(scale) = event_target_value(&ev).parse::<i32>() {
                        apply(current, scale);
                    }
                }>
                    <option value="1" selected=scale == 1>"%100"</option>
                    <option value="2" selected=scale == 2>"%200 (yüksek çözünürlüklü ekranlar)"</option>
                </select>
            </label>
        </section>
    }
}

#[component]
pub fn Settings() -> impl IntoView {
    let (error, set_error) = signal(String::new());
    let screens = RwSignal::new(Vec::<Screen>::new());
    run::<Vec<Screen>>("screens", vec![], set_error, move |s| screens.set(s.unwrap_or_default()));

    view! {
        <main class="settings">
            <h1>"Ayarlar"</h1>
            <p class="error" role="alert">{error}</p>
            <h2 class="group">"Ekranlar"</h2>
            {move || {
                screens
                    .get()
                    .into_iter()
                    .map(|screen| view! { <ScreenSettings screen screens on_error=set_error /> })
                    .collect_view()
            }}
        </main>
    }
}
