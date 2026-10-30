//! File manager page (files.html). The work is done by karakulakd; files are
//! opened in Firefox itself.

use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::JsValue;

use karakulak_browser::t;

use crate::run;

#[derive(Clone, Deserialize)]
struct Place {
    name: String,
    path: String,
}

#[derive(Clone, Deserialize)]
struct Disk {
    id: String,
    name: String,
    mounted: Option<String>,
}

#[derive(Clone, Deserialize)]
struct Mounted {
    path: String,
}

#[derive(Clone, Deserialize)]
struct Entry {
    name: String,
    path: String,
    dir: bool,
    size: u64,
    modified: Option<u64>,
}

#[derive(Clone, Deserialize)]
struct Thumbnail {
    url: String,
}

#[derive(Clone, Deserialize)]
struct Listing {
    path: String,
    parent: Option<String>,
    entries: Vec<Entry>,
    truncated: bool,
}

/// Paths to paste, and whether they were cut (move) or copied.
#[derive(Clone)]
struct Clipboard {
    paths: Vec<String>,
    cut: bool,
}

fn size_text(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn date_text(secs: Option<u64>) -> String {
    let Some(secs) = secs else {
        return String::new();
    };
    let date = js_sys::Date::new(&JsValue::from_f64(secs as f64 * 1000.0));
    String::from(date.to_locale_date_string(karakulak_browser::locale(), &JsValue::UNDEFINED))
        + " "
        + &String::from(date.to_locale_time_string_with_options(
            karakulak_browser::locale(),
            &karakulak_browser::object(&[("hour", "2-digit".into()), ("minute", "2-digit".into())]),
        ))
}

fn ask(question: &str, default: &str) -> Option<String> {
    window()
        .prompt_with_message_and_default(question, default)
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
}

#[component]
pub fn Files() -> impl IntoView {
    let (error, set_error) = signal(String::new());
    let places = RwSignal::new(Vec::<Place>::new());
    let listing = RwSignal::new(None::<Listing>);
    let history = RwSignal::new(Vec::<String>::new());
    let selected = RwSignal::new(Vec::<String>::new());
    let clipboard = RwSignal::new(None::<Clipboard>);
    let hidden = RwSignal::new(false);
    let disks = RwSignal::new(Vec::<Disk>::new());

    // Plugged-in sticks show up by themselves.
    let refresh_disks = move || run::<Vec<Disk>>("usb-list", vec![], set_error, move |d| disks.set(d.unwrap_or_default()));
    refresh_disks();
    set_interval(refresh_disks, std::time::Duration::from_secs(3));

    let load = move |path: String| {
        set_error.set(String::new());
        let args = vec![("path", path.into()), ("hidden", hidden.get_untracked().into())];
        run::<Listing>("files-list", args, set_error, move |l| {
            selected.set(Vec::new());
            listing.set(l);
        });
    };
    let current = move || listing.get_untracked().map(|l| l.path).unwrap_or_default();
    let go = move |path: String| {
        let here = current();
        if !here.is_empty() {
            history.update(|h| h.push(here));
        }
        load(path);
    };
    let reload = move || load(current());

    run::<Vec<Place>>("files-places", vec![], set_error, move |p| {
        let p = p.unwrap_or_default();
        if let Some(home) = p.first() {
            load(home.path.clone());
        }
        places.set(p);
    });

    let back = move |_| {
        let mut previous = None;
        history.update(|h| previous = h.pop());
        if let Some(path) = previous {
            load(path);
        }
    };
    let up = move |_| {
        if let Some(parent) = listing.get_untracked().and_then(|l| l.parent) {
            go(parent);
        }
    };
    let new_folder = move |_| {
        if let Some(name) = ask(t("Yeni klasörün adı:", "Name of the new folder:"), t("Yeni klasör", "New folder")) {
            run::<()>("files-mkdir", vec![("path", current().into()), ("name", name.into())], set_error, move |_| {
                reload()
            });
        }
    };
    let rename = move |_| {
        let Some(path) = selected.get_untracked().first().cloned() else {
            return;
        };
        let old = path.rsplit('/').next().unwrap_or_default().to_string();
        if let Some(name) = ask(t("Yeni ad:", "New name:"), &old) {
            run::<()>("files-rename", vec![("path", path.into()), ("name", name.into())], set_error, move |_| {
                reload()
            });
        }
    };
    let copy = move |cut: bool| {
        let paths = selected.get_untracked();
        if !paths.is_empty() {
            clipboard.set(Some(Clipboard { paths, cut }));
        }
    };
    let paste = move |_| {
        let Some(clip) = clipboard.get_untracked() else {
            return;
        };
        let paths = js_sys::Array::new();
        for path in &clip.paths {
            paths.push(&JsValue::from_str(path));
        }
        let cmd = if clip.cut { "files-move" } else { "files-copy" };
        if clip.cut {
            clipboard.set(None);
        }
        run::<()>(cmd, vec![("paths", paths.into()), ("to", current().into())], set_error, move |_| reload());
    };
    let trash = move |_| {
        let selection = selected.get_untracked();
        if selection.is_empty() {
            return;
        }
        let paths = js_sys::Array::new();
        for path in &selection {
            paths.push(&JsValue::from_str(path));
        }
        run::<()>("files-trash", vec![("paths", paths.into())], set_error, move |_| reload());
    };
    let toggle_hidden = move |_| {
        hidden.update(|h| *h = !*h);
        reload();
    };

    // Search below the current folder; Enter in the box starts it.
    let query = RwSignal::new(String::new());
    let searching = RwSignal::new(false);
    let search = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let text = query.get_untracked();
        if text.trim().is_empty() {
            return;
        }
        let args = vec![("path", current().into()), ("query", text.into())];
        run::<Listing>("files-search", args, set_error, move |l| {
            selected.set(Vec::new());
            searching.set(true);
            listing.set(l);
        });
    };
    let end_search = move |_| {
        query.set(String::new());
        searching.set(false);
        reload();
    };

    // A picture of the selected image.
    let preview = RwSignal::new(None::<String>);
    Effect::new(move || {
        let selection = selected.get();
        preview.set(None);
        let [path] = selection.as_slice() else { return };
        let lower = path.to_lowercase();
        if [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"].iter().any(|ext| lower.ends_with(ext)) {
            run::<Thumbnail>("files-thumbnail", vec![("path", path.clone().into())], set_error, move |t| {
                preview.set(t.map(|t| t.url));
            });
        }
    });

    let rows = move || {
        listing.get().map(|l| {
            l.entries
                .into_iter()
                .map(|entry| {
                    let path = entry.path.clone();
                    let is_selected = {
                        let path = path.clone();
                        move || selected.get().contains(&path)
                    };
                    let click = {
                        let path = path.clone();
                        move |ev: leptos::ev::MouseEvent| {
                            let path = path.clone();
                            selected.update(|s| {
                                if ev.ctrl_key() {
                                    if let Some(i) = s.iter().position(|p| *p == path) {
                                        s.remove(i);
                                    } else {
                                        s.push(path);
                                    }
                                } else {
                                    *s = vec![path];
                                }
                            });
                        }
                    };
                    let open = {
                        let path = path.clone();
                        move |_| {
                            if entry.dir {
                                go(path.clone());
                            } else {
                                run::<()>("files-open", vec![("path", path.clone().into())], set_error, |_| {});
                            }
                        }
                    };
                    view! {
                        <tr class:selected=is_selected on:click=click on:dblclick=open>
                            <td class="name">{if entry.dir { "📁 " } else { "📄 " }}{entry.name}</td>
                            <td class="size">{if entry.dir { String::new() } else { size_text(entry.size) }}</td>
                            <td class="date">{date_text(entry.modified)}</td>
                        </tr>
                    }
                })
                .collect_view()
        })
    };

    view! {
        <div class="files">
            <nav class="places">
                <For each=move || places.get() key=|p| p.path.clone() let:place>
                    {
                        let path = place.path.clone();
                        let active = {
                            let path = path.clone();
                            move || listing.get().is_some_and(|l| l.path == path)
                        };
                        view! {
                            <button class:active=active on:click=move |_| go(path.clone())>{place.name}</button>
                        }
                    }
                </For>
                <Show when=move || !disks.get().is_empty()>
                    <h2>"USB"</h2>
                </Show>
                <For each=move || disks.get() key=|d| (d.id.clone(), d.mounted.clone()) let:disk>
                    {
                        let id = disk.id.clone();
                        let open_disk = {
                            let id = id.clone();
                            move |_| {
                                run::<Mounted>("usb-mount", vec![("id", id.clone().into())], set_error, move |m| {
                                    if let Some(m) = m {
                                        go(m.path);
                                    }
                                    refresh_disks();
                                })
                            }
                        };
                        let eject = move |_| {
                            run::<Vec<Disk>>("usb-eject", vec![("id", id.clone().into())], set_error, move |d| {
                                disks.set(d.unwrap_or_default());
                            })
                        };
                        view! {
                            <div class="disk">
                                <button on:click=open_disk>{disk.name}</button>
                                <Show when=move || disk.mounted.is_some()>
                                    <button title={t("Çıkar", "Eject")} on:click=eject.clone()>"⏏"</button>
                                </Show>
                            </div>
                        }
                    }
                </For>
            </nav>
            <main>
                <header class="toolbar">
                    <button title={t("Geri", "Back")} prop:disabled=move || history.get().is_empty() on:click=back>"←"</button>
                    <button title={t("Üst klasör", "Parent folder")} on:click=up>"↑"</button>
                    <span class="path">
                        {move || {
                            let path = listing.get().map(|l| l.path).unwrap_or_default();
                            if searching.get() { format!("{path}: “{}”", query.get()) } else { path }
                        }}
                    </span>
                    <form class="search" on:submit=search>
                        <input type="search" placeholder={t("Ara", "Search")} bind:value=query />
                    </form>
                    <Show when=move || searching.get()>
                        <button on:click=end_search>{t("Aramayı bitir", "End search")}</button>
                    </Show>
                    <button on:click=new_folder>{t("Yeni klasör", "New folder")}</button>
                    <button prop:disabled=move || selected.get().len() != 1 on:click=rename>{t("Yeniden adlandır", "Rename")}</button>
                    <button prop:disabled=move || selected.get().is_empty() on:click=move |_| copy(false)>{t("Kopyala", "Copy")}</button>
                    <button prop:disabled=move || selected.get().is_empty() on:click=move |_| copy(true)>{t("Kes", "Cut")}</button>
                    <button prop:disabled=move || clipboard.get().is_none() on:click=paste>{t("Yapıştır", "Paste")}</button>
                    <button class="danger" prop:disabled=move || selected.get().is_empty() on:click=trash>{t("Çöpe at", "Move to trash")}</button>
                    <label>
                        <input type="checkbox" prop:checked=hidden on:change=toggle_hidden />
                        {t("Gizli", "Hidden")}
                    </label>
                </header>
                <p class="error" role="alert">{error}</p>
                <table>
                    <thead>
                        <tr><th>{t("Ad", "Name")}</th><th>{t("Boyut", "Size")}</th><th>{t("Değiştirilme", "Modified")}</th></tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
                <Show when=move || preview.get().is_some()>
                    <aside class="preview">
                        <img src=move || preview.get().unwrap_or_default() alt="" />
                    </aside>
                </Show>
                <Show when=move || listing.get().is_some_and(|l| l.truncated)>
                    <p class="note">{t("Bu klasörde çok fazla dosya var, ilk 3000 tanesi gösteriliyor.", "This folder has too many files; the first 3000 are shown.")}</p>
                </Show>
            </main>
        </div>
    }
}
