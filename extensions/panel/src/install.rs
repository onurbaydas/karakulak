//! Installer page (install.html), on the live system: pick a disk, a name and
//! a password; karakulakd's root installer does the rest. Language, keyboard
//! and time zone are carried over from Settings.

use std::time::Duration;

use karakulak_browser::t;
use leptos::prelude::*;
use serde::Deserialize;

use crate::run;

/// karakulak-install refuses smaller disks.
const MIN_SIZE: f64 = 10e9;

#[derive(Clone, Deserialize)]
struct Disk {
    name: String,
    model: String,
    size: f64,
    usb: bool,
}

#[derive(Clone, Deserialize)]
struct Status {
    step: String,
    progress: f64,
    state: String,
    error: Option<String>,
}

#[derive(Clone, Deserialize)]
struct Info {
    live: bool,
    disks: Vec<Disk>,
    status: Option<Status>,
}

/// "Ayşe Yılmaz" -> "ayse": a valid user name from the first name.
fn user_name(full_name: &str) -> String {
    let name: String = full_name
        .split_whitespace()
        .next()
        .unwrap_or("")
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' | 'i' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            c => c,
        })
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect();
    name.trim_start_matches(|c: char| c.is_ascii_digit()).to_string()
}

/// The same rule as karakulak-install: lowercase letters, digits and dashes,
/// starting with a letter.
fn valid_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn step_text(step: &str) -> &'static str {
    match step {
        "partition" => t("Disk hazırlanıyor", "Preparing the disk"),
        "copy" => t("Sistem kopyalanıyor", "Copying the system"),
        "configure" => t("Ayarlar yapılıyor", "Setting up"),
        "bootloader" => t("Açılış ayarlanıyor", "Setting up booting"),
        _ => t("Başlıyor", "Starting"),
    }
}

#[component]
pub fn Install() -> impl IntoView {
    let (error, set_error) = signal(String::new());
    let info = RwSignal::new(None::<Info>);
    let refresh = move || run::<Info>("install-info", vec![], set_error, move |i| info.set(i));
    refresh();
    // Follows the installer while it runs.
    set_interval(
        move || {
            if info.get_untracked().and_then(|i| i.status).is_some_and(|s| s.state == "running") {
                refresh();
            }
        },
        Duration::from_secs(2),
    );

    let disk = RwSignal::new(String::new());
    let full_name = RwSignal::new(String::new());
    let user = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    let hostname = RwSignal::new("karakulak".to_string());
    let erase = RwSignal::new(false);

    let problem = move || {
        if disk.get().is_empty() {
            Some(t("Bir disk seç.", "Choose a disk."))
        } else if full_name.get().trim().is_empty() {
            Some(t("Adını yaz.", "Enter your name."))
        } else if !valid_name(&user.get()) {
            Some(t("Kullanıcı adı küçük harf ve rakam olmalı.", "The user name takes lowercase letters and digits."))
        } else if password.get().is_empty() {
            Some(t("Bir şifre seç.", "Choose a password."))
        } else if password.get() != repeat.get() {
            Some(t("Şifreler aynı değil.", "The passwords differ."))
        } else if !valid_name(&hostname.get()) {
            Some(t("Bilgisayarın adı küçük harf, rakam ve tire olmalı.", "The computer name takes lowercase letters, digits and dashes."))
        } else if !erase.get() {
            Some(t("Diskin silineceğini onayla.", "Confirm that the disk will be erased."))
        } else {
            None
        }
    };

    let start = move |_| {
        let args = vec![
            ("disk", disk.get_untracked().into()),
            ("fullname", full_name.get_untracked().trim().into()),
            ("user", user.get_untracked().into()),
            ("password", password.get_untracked().into()),
            ("hostname", hostname.get_untracked().into()),
        ];
        run::<Info>("install-start", args, set_error, move |i| info.set(i));
    };

    let form = move |disks: Vec<Disk>| {
        let disk_list = disks
            .into_iter()
            .map(|d| {
                let name = d.name.clone();
                let big_enough = d.size >= MIN_SIZE;
                let label = format!(
                    "{} {} ({:.0} GB{})",
                    d.name,
                    d.model,
                    d.size / 1e9,
                    if d.usb { ", USB" } else { "" }
                );
                view! {
                    <label>
                        <span>
                            {label}
                            {(!big_enough).then(|| view! { <small class="hint">{t("10 GB'tan küçük", "Smaller than 10 GB")}</small> })}
                        </span>
                        <input
                            type="radio"
                            name="disk"
                            disabled=!big_enough
                            prop:checked=move || disk.get() == name
                            on:change={
                                let name = d.name.clone();
                                move |_| disk.set(name.clone())
                            }
                        />
                    </label>
                }
            })
            .collect_view();
        view! {
            <section class="card">
                <h2>{t("Disk", "Disk")}</h2>
                {disk_list}
            </section>
            <section class="card">
                <h2>{t("Sen", "You")}</h2>
                <label>
                    {t("Adın", "Your name")}
                    <input
                        type="text"
                        prop:value=move || full_name.get()
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            // The user name follows the name until it is edited.
                            if user.get_untracked() == user_name(&full_name.get_untracked()) {
                                user.set(user_name(&value));
                            }
                            full_name.set(value);
                        }
                    />
                </label>
                <label>
                    {t("Kullanıcı adı", "User name")}
                    <input type="text" bind:value=user />
                </label>
                <label>
                    {t("Şifre", "Password")}
                    <input type="password" bind:value=password />
                </label>
                <label>
                    {t("Şifre (tekrar)", "Password (again)")}
                    <input type="password" bind:value=repeat />
                </label>
                <label>
                    {t("Bilgisayarın adı", "Computer name")}
                    <input type="text" bind:value=hostname />
                </label>
                <p class="hint">
                    {t(
                        "Dil, klavye, saat dilimi ve Wi-Fi ağları şu anki ayarlardan alınır. ",
                        "Language, keyboard, time zone and Wi-Fi networks are taken from the current settings. ",
                    )}
                    <a href="settings.html">{t("Ayarlar", "Settings")}</a>
                </p>
            </section>
            <section class="card">
                <label>
                    {move || {
                        let name = disk.get();
                        if name.is_empty() {
                            t("Seçilen diskteki her şey silinecek.", "Everything on the chosen disk will be erased.").to_string()
                        } else {
                            format!("{} {name}", t("Her şey silinecek:", "Everything will be erased:"))
                        }
                    }}
                    <input type="checkbox" bind:checked=erase />
                </label>
                <p class="hint">{move || problem().unwrap_or("")}</p>
                <button class="danger" disabled=move || problem().is_some() on:click=start>
                    {t("Kur", "Install")}
                </button>
            </section>
        }
        .into_any()
    };

    let progress = |s: Status| {
        let width = format!("width: {:.0}%", (s.progress * 100.0).clamp(0.0, 100.0));
        view! {
            <section class="card">
                <p>{step_text(&s.step)}</p>
                <div class="track"><div class="fill" style=width></div></div>
                <p class="hint">{t("Bilgisayarı kapatma. Birkaç dakika sürer.", "Do not turn the computer off. This takes a few minutes.")}</p>
            </section>
        }
        .into_any()
    };

    let finished = move || {
        view! {
            <section class="card">
                <h2>{t("Karakulak kuruldu", "Karakulak is installed")}</h2>
                <p>{t(
                    "Yeniden başlat, açılırken USB belleği ya da diski çıkar.",
                    "Restart, and remove the USB stick or disc while it starts.",
                )}</p>
                <button on:click={move |_| run::<()>("reboot", vec![], set_error, |_| {})}>{t("Yeniden başlat", "Restart")}</button>
            </section>
        }
        .into_any()
    };

    view! {
        <main class="settings">
            <h1>{t("Karakulak'ı kur", "Install Karakulak")}</h1>
            <p class="error" role="alert">{error}</p>
            {move || {
                let i = info.get()?;
                if !i.live {
                    return Some(view! { <p>{t("Karakulak bu bilgisayarda zaten kurulu.", "Karakulak is already installed on this computer.")}</p> }.into_any());
                }
                Some(match i.status {
                    Some(s) if s.state == "running" => progress(s),
                    Some(s) if s.state == "done" => finished(),
                    Some(s) => {
                        let message = format!("{}: {}", t("Kurulum olmadı", "The installation failed"), s.error.unwrap_or_default());
                        view! { <p class="error">{message}</p> {form(i.disks)} }.into_any()
                    }
                    None => form(i.disks),
                })
            }}
        </main>
    }
}
