//! Task manager page (tasks.html): system load and the user's programs.

use std::time::Duration;

use karakulak_browser::t;
use leptos::prelude::*;
use serde::Deserialize;

use crate::run;

#[derive(Clone, Deserialize)]
struct Usage {
    used: u64,
    total: u64,
}

#[derive(Clone, Deserialize)]
struct Process {
    pid: u64,
    name: String,
    cpu: f64,
    memory: u64,
}

#[derive(Clone, Deserialize)]
struct Overview {
    cpu: f64,
    memory: Usage,
    disk: Usage,
    processes: Vec<Process>,
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / f64::from(1u32 << 30))
}

fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / f64::from(1u32 << 20))
}

/// A labelled bar for a 0..1 share.
fn bar(label: &'static str, share: f64, text: String) -> impl IntoView {
    let width = format!("width: {:.0}%", (share * 100.0).clamp(0.0, 100.0));
    view! {
        <div class="meter">
            <span>{label}</span>
            <div class="track"><div class="fill" style=width></div></div>
            <small>{text}</small>
        </div>
    }
}

#[component]
pub fn Tasks() -> impl IntoView {
    let (error, set_error) = signal(String::new());
    let overview = RwSignal::new(None::<Overview>);
    let refresh = move || run::<Overview>("tasks", vec![], set_error, move |o| overview.set(o));
    refresh();
    set_interval(refresh, Duration::from_secs(2));

    let stop = move |pid: u64, force: bool| {
        run::<()>("task-stop", vec![("pid", (pid as f64).into()), ("force", force.into())], set_error, move |_| refresh());
    };

    view! {
        <main class="tasks">
            <h1>{t("Görev yöneticisi", "Task manager")}</h1>
            <p class="error" role="alert">{error}</p>
            {move || {
                overview.get().map(|o| {
                    let rows = o
                        .processes
                        .into_iter()
                        .map(|p| {
                            let pid = p.pid;
                            view! {
                                <tr>
                                    <td>{p.name}</td>
                                    <td class="num">{format!("{:.0}%", p.cpu * 100.0)}</td>
                                    <td class="num">{mb(p.memory)}</td>
                                    <td class="actions">
                                        <button on:click=move |_| stop(pid, false)>{t("Kapat", "Quit")}</button>
                                        <button class="danger" title=t("Yanıt vermiyorsa", "If it does not respond") on:click=move |_| stop(pid, true)>
                                            {t("Zorla kapat", "Force quit")}
                                        </button>
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view();
                    view! {
                        <section class="meters">
                            {bar(t("İşlemci", "CPU"), o.cpu, format!("{:.0}%", o.cpu * 100.0))}
                            {bar(
                                t("Bellek", "Memory"),
                                o.memory.used as f64 / o.memory.total.max(1) as f64,
                                format!("{} / {}", gb(o.memory.used), gb(o.memory.total)),
                            )}
                            {bar(
                                t("Disk", "Disk"),
                                o.disk.used as f64 / o.disk.total.max(1) as f64,
                                format!("{} / {}", gb(o.disk.used), gb(o.disk.total)),
                            )}
                        </section>
                        <table>
                            <thead>
                                <tr>
                                    <th>{t("Program", "Program")}</th>
                                    <th class="num">{t("İşlemci", "CPU")}</th>
                                    <th class="num">{t("Bellek", "Memory")}</th>
                                    <th></th>
                                </tr>
                            </thead>
                            <tbody>{rows}</tbody>
                        </table>
                    }
                })
            }}
        </main>
    }
}
