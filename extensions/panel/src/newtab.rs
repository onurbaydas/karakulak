//! New tab and home page (newtab.html): clock, search, most visited sites.

use std::time::Duration;

use karakulak_browser::{call, get, object, t};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone)]
struct Site {
    url: String,
    title: String,
}

fn clock() -> (String, String) {
    let now = js_sys::Date::new_0();
    let time = now.to_locale_time_string_with_options(
        karakulak_browser::locale(),
        &object(&[("hour", "2-digit".into()), ("minute", "2-digit".into())]),
    );
    let date = now.to_locale_date_string(
        karakulak_browser::locale(),
        &object(&[("weekday", "long".into()), ("day", "numeric".into()), ("month", "long".into())]),
    );
    (time.into(), date.into())
}

/// "www.example.com" from a URL, for tiles without a title.
fn host(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url).to_string()
}

#[component]
pub fn NewTab() -> impl IntoView {
    let (time, set_time) = signal(clock());
    set_interval(move || set_time.set(clock()), Duration::from_secs(1));

    let sites = RwSignal::new(Vec::<Site>::new());
    spawn_local(async move {
        let Ok(top) = call("topSites.get", &[object(&[("limit", 8.into())])]).await else {
            return;
        };
        let list = js_sys::Array::from(&top)
            .iter()
            .filter_map(|site| {
                let url = get(&site, "url").as_string()?;
                let title = get(&site, "title").as_string().filter(|t| !t.is_empty()).unwrap_or_else(|| host(&url));
                Some(Site { url, title })
            })
            .collect();
        sites.set(list);
    });

    // On the live system: a way to the installer.
    let live = RwSignal::new(false);
    spawn_local(async move {
        if let Ok(Some(session)) = crate::send::<crate::Session>("session", &[]).await {
            live.set(session.live);
        }
    });

    let query = RwSignal::new(String::new());
    let search = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let text = query.get();
        if text.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            let _ = call("search.query", &[object(&[("text", text.into()), ("disposition", "CURRENT_TAB".into())])]).await;
        });
    };

    view! {
        <main class="newtab">
            <time>{move || time.get().0}</time>
            <p class="date">{move || time.get().1}</p>
            <form on:submit=search>
                <input type="search" placeholder={t("Ara", "Search")} autofocus bind:value=query />
            </form>
            <nav class="sites">
                <For each=move || sites.get() key=|s| s.url.clone() let:site>
                    <a href=site.url.clone()>
                        <span class="letter">{site.title.chars().next().unwrap_or('?').to_uppercase().to_string()}</span>
                        <span class="title">{site.title.clone()}</span>
                    </a>
                </For>
            </nav>
            <Show when=move || live.get()>
                <a class="install" href="install.html">{t("Karakulak'ı bu bilgisayara kur", "Install Karakulak on this computer")}</a>
            </Show>
        </main>
    }
}
