//! Installs the running live system to a disk, as root:
//! `karakulak-install <request.json>`. karakulakd starts it through sudo and
//! systemd-run (the live user may run anything without a password); see
//! docs/adr/0014-installer-in-the-browser.md.
//!
//! The request (deleted once read):
//!   {"disk": "sda", "user", "fullname", "password", "hostname", "groups": [..]}
//! Progress goes to /run/karakulak-install.json for karakulakd to read:
//!   {"step", "progress": 0..1, "state": "running" | "done" | "failed", "error"}
//!
//! The whole disk is erased: GPT with an EFI partition (UEFI) or a BIOS boot
//! partition, and the rest for the system (ext4).

use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const STATUS: &str = "/run/karakulak-install.json";
const TARGET: &str = "/mnt/karakulak-install";
/// The live system's root, as built (live-boot mounts it here).
const SOURCE: &str = "/run/live/rootfs/filesystem.squashfs";
/// The ISO's live folder: kernel, the size of the root, live-only packages.
const MEDIUM: &str = "/run/live/medium/live";
const MIN_SIZE: u64 = 10_000_000_000;

fn set_status(step: &str, progress: f64, state: &str, error: Option<&str>) {
    let status = json!({ "step": step, "progress": progress, "state": state, "error": error });
    let tmp = format!("{STATUS}.tmp");
    if fs::write(&tmp, status.to_string()).and_then(|()| fs::rename(&tmp, STATUS)).is_err() {
        eprintln!("could not write {STATUS}");
    }
}

fn step(name: &str, progress: f64) {
    println!("{name}");
    set_status(name, progress, "running", None);
}

/// Runs a program, failing with the end of its error output.
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .env("DEBIAN_FRONTEND", "noninteractive")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(3).collect();
    Err(format!("{program} {}: {}", args.join(" "), tail.into_iter().rev().collect::<Vec<_>>().join(" / ")))
}

fn in_target(args: &[&str]) -> Result<String, String> {
    let mut all = vec![TARGET];
    all.extend_from_slice(args);
    run("chroot", &all)
}

fn target(path: &str) -> PathBuf {
    Path::new(TARGET).join(path.trim_start_matches('/'))
}

struct Request {
    disk: String,
    user: String,
    fullname: String,
    password: String,
    hostname: String,
    groups: Vec<String>,
}

fn valid_name(name: &str, max: usize) -> bool {
    !name.is_empty()
        && name.len() <= max
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn read_request(path: &str) -> Result<Request, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let _ = fs::remove_file(path);
    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let field = |name: &str| value[name].as_str().unwrap_or("").to_string();
    let request = Request {
        disk: field("disk"),
        user: field("user"),
        fullname: field("fullname"),
        password: field("password"),
        hostname: field("hostname"),
        groups: value["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| g.as_str().map(String::from))
            .collect(),
    };
    if request.disk.is_empty() || !request.disk.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("invalid disk".into());
    }
    if !valid_name(&request.user, 32) {
        return Err("invalid user name".into());
    }
    if !valid_name(&request.hostname, 63) {
        return Err("invalid computer name".into());
    }
    if request.fullname.contains([':', ',', '\n']) {
        return Err("invalid full name".into());
    }
    if request.password.is_empty() || request.password.contains('\n') {
        return Err("invalid password".into());
    }
    Ok(request)
}

/// Refuses disks that are too small or in use, such as the live medium.
fn check_disk(disk: &str) -> Result<(), String> {
    let dev = format!("/dev/{disk}");
    let size: u64 = run("blockdev", &["--getsize64", &dev])?.trim().parse().map_err(|_| "unknown disk size")?;
    if size < MIN_SIZE {
        return Err(format!("{dev} is smaller than 10 GB"));
    }
    let mounts = run("lsblk", &["-n", "-r", "-o", "MOUNTPOINTS", &dev])?;
    if mounts.lines().any(|l| !l.trim().is_empty()) {
        return Err(format!("{dev} is in use (mounted)"));
    }
    Ok(())
}

/// /dev/sda1, /dev/nvme0n1p1
fn partition(disk: &str, n: u32) -> String {
    if disk.ends_with(|c: char| c.is_ascii_digit()) {
        format!("/dev/{disk}p{n}")
    } else {
        format!("/dev/{disk}{n}")
    }
}

fn uuid(dev: &str) -> Result<String, String> {
    Ok(run("blkid", &["-s", "UUID", "-o", "value", dev])?.trim().to_string())
}

fn used_bytes(path: &str) -> u64 {
    let Ok(c_path) = std::ffi::CString::new(path) else { return 0 };
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return 0;
    }
    (stat.f_blocks - stat.f_bfree) as u64 * stat.f_frsize as u64
}

/// Copies the live system's root, reporting progress from 0.05 to 0.8.
fn copy_system() -> Result<(), String> {
    let total: u64 = fs::read_to_string(format!("{MEDIUM}/filesystem.size"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let start = used_bytes(TARGET);
    let mut copy = Command::new("cp")
        .args(["-a", &format!("{SOURCE}/."), TARGET])
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cp: {e}"))?;
    loop {
        if let Some(status) = copy.try_wait().map_err(|e| e.to_string())? {
            if status.success() {
                return Ok(());
            }
            let mut error = String::new();
            use std::io::Read;
            let _ = copy.stderr.take().map(|mut s| s.read_to_string(&mut error));
            return Err(format!("copying the system failed: {}", error.lines().last().unwrap_or("")));
        }
        if total > 0 {
            let done = used_bytes(TARGET).saturating_sub(start) as f64 / total as f64;
            set_status("copy", 0.05 + 0.75 * done.min(1.0), "running", None);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// /dev, /proc and /sys inside the new system, for grub and initramfs.
fn mount_system_dirs() -> Result<(), String> {
    for dir in ["dev", "sys"] {
        let path = target(dir);
        let path = path.to_str().unwrap();
        run("mount", &["--rbind", &format!("/{dir}"), path])?;
        // Unmounting the new system later must not unmount the live one's.
        run("mount", &["--make-rslave", path])?;
    }
    run("mount", &["-t", "proc", "proc", target("proc").to_str().unwrap()])?;
    Ok(())
}

/// The kernel the system was built with (live-build may keep it only on the
/// ISO); returns its version.
fn install_kernel() -> Result<String, String> {
    let modules = fs::read_dir(target("lib/modules")).map_err(|e| format!("no kernel modules: {e}"))?;
    let version = modules
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .max()
        .ok_or("no kernel modules")?;
    let kernel = target(&format!("boot/vmlinuz-{version}"));
    if !kernel.exists() {
        let from = [format!("{MEDIUM}/vmlinuz-{version}"), format!("{MEDIUM}/vmlinuz")]
            .into_iter()
            .find(|p| Path::new(p).exists())
            .ok_or("no kernel on the live medium")?;
        fs::copy(&from, &kernel).map_err(|e| format!("copying the kernel: {e}"))?;
    }
    Ok(version)
}

/// Packages only the live system needs (live-boot, live-config, user-setup...).
fn remove_live_packages() -> Result<(), String> {
    let list = fs::read_to_string(format!("{MEDIUM}/filesystem.packages-remove")).unwrap_or_default();
    let installed: Vec<&str> = list
        .split_whitespace()
        .filter(|p| {
            in_target(&["dpkg-query", "-W", "-f=${db:Status-Status}", p]).is_ok_and(|s| s == "installed")
        })
        .collect();
    if !installed.is_empty() {
        let mut args = vec!["dpkg", "--purge"];
        args.extend(installed);
        in_target(&args)?;
    }
    Ok(())
}

/// Language, keyboard, time zone and Wi-Fi networks as chosen in the live system.
fn copy_live_settings() -> Result<(), String> {
    for file in ["etc/default/locale", "etc/default/keyboard", "etc/timezone", "etc/locale.gen"] {
        let from = Path::new("/").join(file);
        if from.exists() {
            fs::copy(&from, target(file)).map_err(|e| format!("{file}: {e}"))?;
        }
    }
    if let Ok(zone) = fs::read_link("/etc/localtime") {
        let link = target("etc/localtime");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(zone, link).map_err(|e| format!("etc/localtime: {e}"))?;
    }
    in_target(&["locale-gen"])?;
    let networks = "etc/NetworkManager/system-connections";
    if let Ok(entries) = fs::read_dir(Path::new("/").join(networks)) {
        fs::create_dir_all(target(networks)).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            fs::copy(entry.path(), target(networks).join(entry.file_name())).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn write(path: &str, text: &str) -> Result<(), String> {
    fs::write(target(path), text).map_err(|e| format!("{path}: {e}"))
}

fn configure(request: &Request, efi: Option<&str>, root: &str) -> Result<(), String> {
    let mut fstab = format!("UUID={} / ext4 errors=remount-ro 0 1\n", uuid(root)?);
    if let Some(efi) = efi {
        fstab += &format!("UUID={} /boot/efi vfat umask=0077 0 1\n", uuid(efi)?);
    }
    write("etc/fstab", &fstab)?;
    write("etc/hostname", &format!("{}\n", request.hostname))?;
    write(
        "etc/hosts",
        &format!(
            "127.0.0.1\tlocalhost\n127.0.1.1\t{}\n::1\tlocalhost ip6-localhost ip6-loopback\nff02::1\tip6-allnodes\nff02::2\tip6-allrouters\n",
            request.hostname
        ),
    )?;
    // A new id on first boot.
    write("etc/machine-id", "")?;

    copy_live_settings()?;

    // The live user's groups (sound, video, network, sudo...) that exist here.
    let existing = fs::read_to_string(target("etc/group")).unwrap_or_default();
    let mut groups: Vec<&str> = request
        .groups
        .iter()
        .map(String::as_str)
        .filter(|g| *g != request.user && existing.lines().any(|l| l.split(':').next() == Some(g)))
        .collect();
    if !groups.contains(&"sudo") {
        groups.push("sudo");
    }
    let groups = groups.join(",");
    in_target(&["useradd", "-m", "-s", "/bin/bash", "-c", &request.fullname, "-G", &groups, &request.user])?;
    let mut chpasswd = Command::new("chroot")
        .args([TARGET, "chpasswd"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    writeln!(chpasswd.stdin.take().unwrap(), "{}:{}", request.user, request.password).map_err(|e| e.to_string())?;
    if !chpasswd.wait().map_err(|e| e.to_string())?.success() {
        return Err("could not set the password".into());
    }

    // Auto-login the new user instead of the live one; the lock screen asks
    // for the password.
    let autologin = "etc/systemd/system/getty@tty1.service.d/autologin.conf";
    let text = fs::read_to_string(target(autologin)).map_err(|e| format!("{autologin}: {e}"))?;
    let text = text.replace("--autologin karakulak", &format!("--autologin {}", request.user));
    write(autologin, &text)
}

fn install_bootloader(disk: &str, uefi: bool, kernel: &str) -> Result<(), String> {
    write(
        "etc/default/grub",
        "GRUB_DEFAULT=0\nGRUB_TIMEOUT=2\nGRUB_TIMEOUT_STYLE=hidden\nGRUB_DISTRIBUTOR=Karakulak\n\
         GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\nGRUB_CMDLINE_LINUX=\"\"\n",
    )?;
    if uefi {
        // "debian": the signed GRUB looks for its configuration there (Secure
        // Boot). The extra copy in EFI/BOOT boots even when the firmware
        // forgets its boot entries.
        let args = [
            "grub-install",
            "--target=x86_64-efi",
            "--efi-directory=/boot/efi",
            "--bootloader-id=debian",
            "--uefi-secure-boot",
            "--force-extra-removable",
        ];
        if in_target(&args).is_err() {
            // Some firmware refuses new boot entries.
            let mut args = args.to_vec();
            args.push("--no-nvram");
            in_target(&args)?;
        }
    } else {
        in_target(&["grub-install", "--target=i386-pc", &format!("/dev/{disk}")])?;
    }
    // Without live-boot's hooks; created if the ISO carried the only one.
    let create = if target(&format!("boot/initrd.img-{kernel}")).exists() { "-u" } else { "-c" };
    in_target(&["update-initramfs", create, "-k", kernel])?;
    in_target(&["update-grub"])?;
    Ok(())
}

fn install(request: &Request) -> Result<(), String> {
    let uefi = Path::new("/sys/firmware/efi").exists();
    let dev = format!("/dev/{}", request.disk);
    check_disk(&request.disk)?;

    step("partition", 0.0);
    let layout = if uefi {
        "label: gpt\nsize=512MiB, type=U\ntype=L\n"
    } else {
        "label: gpt\nsize=1MiB, type=21686148-6449-6E6F-744E-656564454649\ntype=L\n"
    };
    run("wipefs", &["-a", &dev])?;
    let mut sfdisk = Command::new("sfdisk")
        .args(["--wipe", "always", "--wipe-partitions", "always", &dev])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    sfdisk.stdin.take().unwrap().write_all(layout.as_bytes()).map_err(|e| e.to_string())?;
    if !sfdisk.wait().map_err(|e| e.to_string())?.success() {
        return Err(format!("could not partition {dev}"));
    }
    run("udevadm", &["settle"])?;
    let efi = uefi.then(|| partition(&request.disk, 1));
    let root = partition(&request.disk, 2);
    if let Some(efi) = &efi {
        run("mkfs.fat", &["-F", "32", "-n", "EFI", efi])?;
    }
    run("mkfs.ext4", &["-F", "-q", "-L", "Karakulak", &root])?;
    fs::create_dir_all(TARGET).map_err(|e| e.to_string())?;
    run("mount", &[&root, TARGET])?;

    step("copy", 0.05);
    copy_system()?;

    step("configure", 0.8);
    if let Some(efi) = &efi {
        fs::create_dir_all(target("boot/efi")).map_err(|e| e.to_string())?;
        run("mount", &[efi, target("boot/efi").to_str().unwrap()])?;
    }
    mount_system_dirs()?;
    let kernel = install_kernel()?;
    remove_live_packages()?;
    configure(request, efi.as_deref(), &root)?;

    step("bootloader", 0.9);
    install_bootloader(&request.disk, uefi, &kernel)?;
    run("sync", &[])?;
    Ok(())
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: karakulak-install <request.json>");
        std::process::exit(2);
    };
    let result = read_request(&path).and_then(|request| install(&request));
    if Path::new(TARGET).exists() {
        let _ = run("umount", &["-R", TARGET]);
    }
    match result {
        Ok(()) => {
            set_status("done", 1.0, "done", None);
            println!("Karakulak is installed");
        }
        Err(error) => {
            set_status("failed", 0.0, "failed", Some(&error));
            println!("failed: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_names() {
        assert_eq!(partition("sda", 1), "/dev/sda1");
        assert_eq!(partition("vda", 2), "/dev/vda2");
        assert_eq!(partition("nvme0n1", 2), "/dev/nvme0n1p2");
        assert_eq!(partition("mmcblk0", 1), "/dev/mmcblk0p1");
    }

    #[test]
    fn names() {
        assert!(valid_name("ayse", 32));
        assert!(valid_name("my-pc2", 63));
        assert!(!valid_name("Ayse", 32));
        assert!(!valid_name("2pc", 63));
        assert!(!valid_name("a b", 32));
        assert!(!valid_name("", 32));
    }
}
