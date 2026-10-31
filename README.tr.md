<p align="center"><img src="docs/brand/logo.svg" width="160" alt="Karakulak logosu"></p>

# Karakulak

**Tarayıcının işletim sistemi olduğu bir Linux dağıtımı.**

[English](README.md)

Karakulak açılır açılmaz Firefox'a düşer. Masaüstü ortamı yoktur: sekmeler görev
çubuğudur, her araç (dosya yöneticisi, ayarlar, sistem tepsisi) bir tarayıcı
eklentisidir.

> *Karakulak*, Türkçede "kara kulak" demek. Dünya bu hayvanı bizim verdiğimiz
> adla tanır: caracal.

<p align="center"><img src="docs/screenshots/newtab.png" alt="Karakulak'ın yeni sekme sayfası"></p>

**[ISO'yu indir](https://github.com/onurbaydas/karakulak/releases/latest)** ·
**[Ekran görüntüleriyle tur](https://onurbaydas.github.io/karakulak/)**

## Durum

Sürüm 0.9.7: günlük kullanım için gereken her şey var. Açılıyor, diske
kuruluyor (BIOS ve UEFI) ve kendini güncelliyor. Her derleme sanal makinede
test ediliyor. Her sürümde neyin değiştiği [CHANGELOG.md](CHANGELOG.md)
dosyasında.

Katkıya açık.
[good first issue](https://github.com/onurbaydas/karakulak/labels/good%20first%20issue)
etiketli işler küçük, ne yapılacağı yazılı ve başlamak için iyi bir yer.

## Ekran görüntüleri

| | |
|---|---|
| ![Panel](docs/screenshots/panel.png) | ![Dosyalar](docs/screenshots/files.png) |
| ![Ayarlar](docs/screenshots/settings.png) | ![Görev yöneticisi](docs/screenshots/tasks.png) |
| ![Terminal](docs/screenshots/terminal.png) | ![Kurulum](docs/screenshots/install.png) |

## Sistem gereksinimleri

- 64 bit x86 (Intel ya da AMD), 2 çekirdek
- 4 GB bellek
- En az 10 GB disk (20 GB önerilir)
- BIOS ya da UEFI

Sanal makinede ekran kartı olarak virtio'yu, UEFI için q35 makine tipini seç.

## İçinde neler var

- Debian (trixie) tabanlı ISO: diske kurulur, istersen önce canlı denenir
- Otomatik giriş, ardından Karakulak'ın kendi Wayland ekran yöneticisi (Rust,
  Smithay); her ekranda tam ekran Firefox, şifreyle kilitlenir
- Firefox, Mozilla'nın resmi APT deposundan; hoş geldin sayfasında gizlilik
  seviyesi seçimi: standart, sıkı ya da paranoyak
- Sekmeler görev çubuğu; Wi-Fi, Bluetooth, ses, parlaklık, pil ve güç için
  panel eklentisi
- Dosyalar, terminal, görevler ve ayarlar eklenti sayfası; arkalarında native
  messaging ile konuşan Rust daemon'u (karakulakd)
- Karakulak'ın kendi imzalı APT deposundan güncellemeler

## Derleme

Root erişimi olan Debian tabanlı bir sistem (ya da container) gerekir.

```sh
sudo apt install live-build
cargo build --release --manifest-path daemon/Cargo.toml
sudo install -D daemon/target/release/karakulakd config/includes.chroot/usr/bin/karakulakd
lb config
sudo lb build
```

ISO, reponun kök dizinine düşer. `main` dalına yapılan her push GitHub
Actions'ta da ISO üretir. ISO'yu ilgili çalıştırmanın artifact'lerinden
indirebilirsin.

Sanal makinede denemek için:

```sh
qemu-system-x86_64 -enable-kvm -m 4G -cdrom live-image-amd64.hybrid.iso
```

## Katkı

[CONTRIBUTING.md](CONTRIBUTING.md) dosyasına bak. Commit'ler imzalı (DCO) olmalı.

## Lisans

[GPL-3.0](LICENSE)
