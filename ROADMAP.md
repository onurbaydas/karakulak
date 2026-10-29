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

- [x] Ayarlar sayfası: dil, klavye, saat dilimi, gizlilik seviyesi (polkit kuralı,
  sadece bu ayarlar için şifresiz)
- [x] Güncellemeler: güvenlik güncellemeleri her gün kendiliğinden, "Şimdi güncelle"
- [x] Güvenlik duvarı (nftables): içeri bağlantı yok, Ayarlar'dan aç/kapa
- [x] Yazıcı: CUPS, sürücüsüz yazıcılar; Ayarlar'dan CUPS arayüzü

Bitti sayılır: Terminal açmadan sistemi güncelleyip ayarlarını değiştirebiliyorsun.

## v0.6 — Günlük kullanım ve test

- [x] `./vm.sh`: ISO'yu tek komutla sanal makinede açar
- [ ] GitHub'da her ISO için otomatik açılış testi (ekran görüntüsüyle)
- [x] Klavyedeki ses ve parlaklık tuşları
- [x] Touchpad: dokunarak tıklama, doğal kaydırma
- [x] Boşta kalınca ekran kararır ve kilitlenir (video oynarken değil); ekranı gerçekten
  kapatmak (DPMS) sonra
- [x] Pil azalınca uyarı, kritik seviyede uyku
- [x] Print Screen ile ekran görüntüsü (Resimler klasörüne)
- [x] Gece ışığı
- [x] Ses ve parlaklık değişince ekranda küçük gösterge
- [x] Önerilen eklentiler sayfası (uBlock Origin gibi, tek tıkla kurulur)

Bitti sayılır: ISO sanal makinede açılıyor ve otomatik test geçiyor; laptopta
tuşlar, touchpad ve pil uyarısı çalışıyor.

## v0.7 — Gizlilik ve donanım

- [x] Parlaklık önce donanımla (ekran ışığı, DDC/CI), olmazsa gamma; boşta ekran
  gerçekten kapanır (#1, gerçek donanımda denenecek)
- [x] Uykuya geçmeden önce kesin kilit (#2)
- [x] Wi-Fi'de ağ başına farklı MAC adresi, bilgisayar adı ağa gönderilmez
- [x] WireGuard VPN: Ayarlar'dan dosya yükle, panelden aç/kapa
- [ ] Ses cihazı seçimi, mikrofon seviyesi
- [ ] Kesirli ölçek (%125, %150)
- [ ] İngilizce arayüz (sistem diline göre)
- [ ] Ekran büyüteci (Super+artı/eksi)
- [ ] Bluetooth'ta kod isteyen cihazlar

## v1.0 — İlk sürüm

- [ ] Kurulum da tarayıcının içinde (Debian'ın kurulum ekranı kalkar)
- [ ] Gerçek donanımda test
- [ ] İlk resmi sürüm ve duyuru
