# Karakulak Yol Haritası

Kural: Her şey Firefox'un içinde. Tarayıcı dışında arayüz yok.

Her aşama, "Bitti sayılır" maddeleri gerçek bir makinede ya da sanal makinede
denenince biter.

## v0.1 — Açılıyor

Durum: Yazıldı, henüz denenmedi.

- [x] Debian tabanlı ISO: canlı dene ya da diske kur
- [x] Açılışta otomatik giriş, tam ekran Firefox
- [x] Hoş geldin sayfasında gizlilik seviyesi seçimi
- [x] Lisans, katkı rehberi, CI

Bitti sayılır: ISO sanal makinede açılıyor, kurulum yapılıyor, kurulan sistem
Firefox'la açılıyor, seçilen gizlilik seviyesi uygulanıyor.

## v0.2 — Bilgisayarı kontrol et

Eklentiler bilgisayara dokunamaz. Araya Rust ile yazılmış küçük bir program
giriyor (`karakulakd`). Firefox bu programı native messaging ile çalıştırır,
eklentiler ona komut gönderir.

- [x] `karakulakd`: Firefox'tan gelen komutları alıp cevap veren Rust programı
- [x] Sistem paneli eklentisi: araç çubuğunda bir ikon
  - [x] Kapat, yeniden başlat, uyku
  - [x] Saat ve tarih
  - [x] Pil durumu
  - [x] Ses aç/kıs
  - [x] Wi-Fi listesi ve bağlanma
  - [x] Bluetooth: aç/kapat, eşleşmiş cihazlara bağlan
  - [x] Bluetooth: yeni cihaz arama ve eşleştirme (kod istemeyen cihazlar)
  - [x] Ekran parlaklığı: laptop ekranı ve DDC/CI destekli harici monitörler
- [x] Kilit ekranı: açılışta, uykudan önce ve panelden "Kilitle" ile kilitlenir.
  Kilitliyken ekran yöneticisi bütün pencereleri gizler, sadece kilit sayfasını
  (boş profilli kiosk Firefox) gösterir. Şifreyi ekran yöneticisi PAM ile
  kontrol eder.
- [x] Kapağı kapatınca uyku (systemd-logind bunu varsayılan olarak yapıyor)
- [ ] Kendi Rust ekran yöneticimiz (Smithay), cage'in yerine. Bunlar için şart:
  - [x] Kilit ekranı (yukarıda)
  - [x] Temel: Firefox'u ekranı kaplayacak şekilde açar (şimdilik başka bir
    masaüstünün içinde pencere olarak, `./dev.sh session`)
  - [x] Parlaklık: ekran yöneticisi görüntüyü karartır, her ekranda çalışır
  - [x] Gerçek ekranlarda, masaüstü olmadan çalışma (DRM/KMS, birden fazla
    ekran kartı), ISO'da cage'in yerini aldı
  - [x] Her ekranda ayrı Firefox penceresi (boş ekran için Firefox'tan yeni
    pencere istenir)
  - [ ] Parlaklık: önce donanım, olmazsa ekran kartının renk tablosu (gamma)

Bitti sayılır: Panelden Wi-Fi'ye bağlanıp bilgisayarı kapatabiliyorsun. Uykudan
dönünce şifre soruluyor. İki monitörde her ekranda ayrı Firefox açılıyor.
Hepsi önce kendi bilgisayarımızdaki Firefox'ta denenir, sonra ISO'ya girer.

## v0.3 — Dosyalar

- [x] Dosya yöneticisi (paneldeki "Dosyalar"): klasörleri gez, dosyayı Firefox'ta aç,
  yeni klasör, yeniden adlandır, kopyala, taşı, çöpe at
- [x] USB bellek takınca görünsün, tıklayınca bağlansın, "Çıkar" ile güvenle çıksın
- [x] İndirilenler klasörü solda hazır
- [x] Terminal (paneldeki "Terminal"): kabuk `karakulakd`'de gerçek bir PTY'de çalışır,
  ekranı `vt100` (Rust, WebAssembly) çizer; renkler, imleç, Ctrl+Shift+C/V
- [x] Monitör ayarları (paneldeki "Ayarlar"): çözünürlük ve ölçek, monitör başına
  hatırlanır

Bitti sayılır: USB'den bir dosyayı ev klasörüne kopyalayıp Firefox'ta açabiliyorsun.
Terminal sekmesinde komut çalıştırabiliyorsun.

## v0.4 — Görünüm

- [x] Pencere düzeni: bir ekranda birden fazla Firefox penceresi yan yana
  (Super+←/→ yarım ekran, Super+↑ tam ekran, Super+Shift+←/→ başka ekrana,
  Alt+Tab pencereler arası, sekme çubuğundan sürükleyip kenara bırakınca yapışma)
- [x] Yeni sekme ve ana sayfa: saat, arama, en çok ziyaret edilen siteler
- [x] Tema: Firefox'un çubukları altta, sekmeler görev çubuğu gibi, Karakulak renkleri
- [x] Logo (docs/brand/logo.svg) ve açılış ekranı (Plymouth, disk şifresi ekranıyla)

Bitti sayılır: Açılıştan masaüstüne kadar her ekran Karakulak'a ait görünüyor.

## v0.5 — Ayarlar

- [ ] Ayarlar sayfası: dil, klavye, saat dilimi, gizlilik seviyesi
- [ ] Sistem güncellemeleri tarayıcıdan
- [ ] Güvenlik duvarı
- [ ] Yazıcı

Bitti sayılır: Terminal açmadan sistemi güncelleyip ayarlarını değiştirebiliyorsun.

## v1.0 — İlk sürüm

- [ ] Kurulum da tarayıcının içinde (Debian'ın kurulum ekranı kalkar)
- [ ] Gerçek donanımda test
- [ ] İlk resmi sürüm ve duyuru
