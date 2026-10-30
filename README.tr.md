<p align="center"><img src="docs/brand/logo.svg" width="160" alt="Karakulak logosu"></p>

# Karakulak

**Tarayıcının işletim sistemi olduğu bir Linux dağıtımı.**

[English](README.md)

Karakulak açılır açılmaz Firefox'a düşer. Masaüstü ortamı yoktur: sekmeler görev
çubuğudur, her araç (dosya yöneticisi, ayarlar, sistem tepsisi) bir tarayıcı
eklentisidir.

> *Karakulak*, Türkçede "kara kulak" demek. Dünya bu hayvanı bizim verdiğimiz
> adla tanır: caracal.

## Durum

Sürüm 0.9: günlük kullanım için gereken her şey var, 1.0'dan önce deneniyor.
Her sürümde neyin değiştiği [CHANGELOG.md](CHANGELOG.md) dosyasında.

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
