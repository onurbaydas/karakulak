//! Keyboard shortcuts page (shortcuts.html), opened with Super+/.

use karakulak_browser::t;
use leptos::prelude::*;

/// (keys, Turkish, English)
const KARAKULAK: [(&str, &str, &str); 12] = [
    ("Super + ←  /  →", "Pencereyi ekranın sol / sağ yarısına al", "Window to the left / right half"),
    ("Super + ↑", "Pencere ekranı kaplasın", "Window fills the screen"),
    ("Super + Shift + ←  /  →", "Pencereyi diğer ekrana taşı", "Move the window to another screen"),
    ("Alt + Tab", "Aynı ekrandaki pencereler arasında geç", "Next window on this screen"),
    ("Super + +  /  −", "Büyüteç: yakınlaştır / uzaklaştır", "Magnifier: zoom in / out"),
    ("Super + 0", "Büyüteci kapat", "Magnifier off"),
    ("Super + /", "Bu sayfa", "This page"),
    ("Print Screen", "Ekran görüntüsü (Resimler klasörüne)", "Screenshot (to the Pictures folder)"),
    ("🔊 🔉 🔇", "Ses tuşları", "Volume keys"),
    ("☀", "Parlaklık tuşları", "Brightness keys"),
    ("Ctrl + Alt + F1 … F12", "Başka bir terminale geç", "Switch to another terminal"),
    ("Ctrl + Alt + Backspace", "Acil çıkış: oturumu kapat", "Emergency exit: end the session"),
];

const FIREFOX: [(&str, &str, &str); 8] = [
    ("Ctrl + T", "Yeni sekme", "New tab"),
    ("Ctrl + W", "Sekmeyi kapat", "Close tab"),
    ("Ctrl + Shift + T", "Kapanan sekmeyi geri aç", "Reopen closed tab"),
    ("Ctrl + Tab", "Sonraki sekme", "Next tab"),
    ("Ctrl + L", "Adres çubuğuna git", "Go to the address bar"),
    ("Ctrl + N", "Yeni pencere", "New window"),
    ("Ctrl + Shift + P", "Gizli pencere", "Private window"),
    ("F11", "Tam ekran", "Full screen"),
];

fn table(rows: &'static [(&'static str, &'static str, &'static str)]) -> impl IntoView {
    view! {
        <section class="card">
            <table class="keys">
                {rows
                    .iter()
                    .map(|(keys, tr, en)| view! { <tr><td><kbd>{*keys}</kbd></td><td>{t(tr, en)}</td></tr> })
                    .collect_view()}
            </table>
        </section>
    }
}

#[component]
pub fn Shortcuts() -> impl IntoView {
    view! {
        <main class="settings">
            <h1>{t("Kısayollar", "Keyboard shortcuts")}</h1>
            <h2 class="group">"Karakulak"</h2>
            {table(&KARAKULAK)}
            <h2 class="group">"Firefox"</h2>
            {table(&FIREFOX)}
        </main>
    }
}
