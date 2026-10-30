//! Help (help.html) and problem report (report.html) pages.

use karakulak_browser::t;
use leptos::prelude::*;
use serde::Deserialize;

use crate::run;

/// (Turkish title, English title, Turkish text, English text)
const TOPICS: [(&str, &str, &str, &str); 9] = [
    (
        "Panel",
        "Panel",
        "Araç çubuğundaki güç simgesi. Saat, pil, ses, parlaklık, Wi-Fi, Bluetooth, VPN; Dosyalar, Terminal, Görevler ve Ayarlar buradan açılır.",
        "The power icon in the toolbar. Clock, battery, sound, brightness, Wi-Fi, Bluetooth, VPN; Files, Terminal, Tasks and Settings open from here.",
    ),
    (
        "Pencereler ve ekranlar",
        "Windows and screens",
        "Her ekranda bir Firefox penceresi açılır. Super + ok tuşlarıyla pencereleri yan yana dizebilirsin; bütün kısayollar için Super + /.",
        "Each screen gets a Firefox window. Arrange windows side by side with Super + arrow keys; Super + / lists every shortcut.",
    ),
    (
        "Görüntülü görüşme",
        "Video calls",
        "Meet, Zoom, Discord gibi sitelerde ekranını paylaşabilirsin; şimdilik ilk ekran paylaşılır. Klavyen iki dilliyse Ayarlar'dan ikinci klavyeyi seç, Super + Boşluk ile geç.",
        "Share your screen on sites like Meet, Zoom or Discord; for now the first screen is shared. For a second keyboard layout, pick it in Settings and switch with Super + Space.",
    ),
    (
        "Dosyalar",
        "Files",
        "Dosyaya çift tıklayınca Firefox'ta açılır: PDF, resim, video, metin. USB bellek takınca solda görünür; çıkarmadan önce ⏏ ile ayır.",
        "Double-click a file to open it in Firefox: PDF, images, video, text. A USB stick shows up on the left; press ⏏ before pulling it out.",
    ),
    (
        "Programlar",
        "Programs",
        "Karakulak'ta programlar Firefox eklentileridir. Ayarlar'daki önerilen eklentilerle başla; Mozilla'nın eklenti mağazasında binlercesi var.",
        "In Karakulak, programs are Firefox extensions. Start with the recommended ones in Settings; Mozilla's add-on store has thousands more.",
    ),
    (
        "Gizlilik",
        "Privacy",
        "Gizlilik seviyesi, Tor ve VPN Ayarlar'da. Telemetri hep kapalı. Wi-Fi ağları bilgisayarını tanıyamaz: her ağa farklı bir adres gider.",
        "Privacy level, Tor and VPN are in Settings. Telemetry is always off. Wi-Fi networks cannot recognize this computer: each sees a different address.",
    ),
    (
        "Kilit ve misafir",
        "Lock and guest",
        "Panelden Kilitle; uyurken ve boşta kalınca da kilitlenir. Başkası kısa süre kullanacaksa Misafir: iz bırakmaz, bitince her şey silinir.",
        "Lock from the panel; it also locks when sleeping or idle. For someone else, briefly: Guest. It leaves no trace and everything is deleted at the end.",
    ),
    (
        "Güncellemeler ve yedek",
        "Updates and backup",
        "Güvenlik güncellemeleri kendiliğinden kurulur; Ayarlar'da Şimdi güncelle de var. Yer imleri ve şifreler için Ayarlar'dan USB'ye yedek al.",
        "Security updates install themselves; Settings also has Update now. Back up bookmarks and passwords to a USB stick from Settings.",
    ),
    (
        "Bir şey bozulursa",
        "If something breaks",
        "Ctrl + Alt + Backspace oturumu kapatır ve yeniden başlatır. Sorun sürerse Sorun bildir sayfasında bir rapor oluşturup GitHub'da paylaş.",
        "Ctrl + Alt + Backspace ends the session and starts it again. If the problem stays, create a report on the Report a problem page and share it on GitHub.",
    ),
];

#[component]
pub fn Help() -> impl IntoView {
    view! {
        <main class="settings">
            <h1>{t("Yardım", "Help")}</h1>
            {TOPICS.map(|(tr_title, en_title, tr_text, en_text)| {
                view! {
                    <section class="card">
                        <h2>{t(tr_title, en_title)}</h2>
                        <p>{t(tr_text, en_text)}</p>
                    </section>
                }
            })}
            <p>
                <a href="shortcuts.html">{t("Klavye kısayolları", "Keyboard shortcuts")}</a>
                " · "
                <a href="report.html">{t("Sorun bildir", "Report a problem")}</a>
            </p>
        </main>
    }
}

#[derive(Clone, Deserialize)]
struct Report {
    path: String,
    text: String,
}

#[component]
pub fn ReportPage() -> impl IntoView {
    let (error, set_error) = signal(String::new());
    let report = RwSignal::new(None::<Report>);
    let busy = RwSignal::new(false);
    let create = move |_| {
        busy.set(true);
        run::<Report>("report", vec![], set_error, move |r| {
            busy.set(false);
            report.set(r);
        });
    };
    view! {
        <main class="settings">
            <h1>{t("Sorun bildir", "Report a problem")}</h1>
            <p>
                {t(
                    "Rapor; Karakulak sürümünü, donanımı ve ekran yöneticisinin son kayıtlarını tek bir dosyada toplar. Hiçbir şey kendiliğinden gönderilmez: okuyup istersen GitHub'daki sorun kaydına eklersin.",
                    "The report collects the Karakulak version, the hardware and the compositor's recent log in one file. Nothing is sent by itself: read it, then attach it to an issue on GitHub if you like.",
                )}
            </p>
            <button prop:disabled=move || busy.get() on:click=create>{t("Rapor oluştur", "Create report")}</button>
            <p class="error" role="alert">{error}</p>
            {move || report.get().map(|r| view! {
                <p>{t("Kaydedildi:", "Saved:")} " " <code>{r.path}</code></p>
                <p>
                    <a href="https://github.com/onurbaydas/karakulak/issues/new?template=bug_report.md" target="_blank">
                        {t("GitHub'da sorun kaydı aç", "Open an issue on GitHub")}
                    </a>
                </p>
                <pre class="report">{r.text}</pre>
            })}
        </main>
    }
}
