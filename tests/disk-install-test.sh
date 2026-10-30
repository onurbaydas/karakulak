#!/bin/sh
# Installs the ISO to an empty virtual disk the way the install page does
# (karakulakd --install, started by the kernel option karakulak.install-test),
# then boots the disk alone and takes a screenshot. Fails if the installer
# fails or the installed system shows no user interface.
#
# Usage: tests/disk-install-test.sh karakulak.iso [output-dir] [bios|uefi]
# Needs: qemu-system-x86_64, qemu-img, xorriso, socat, python3; for UEFI the
# OVMF firmware (OVMF_CODE and OVMF_VARS set its files, Ubuntu's by default)
set -e

iso=$(realpath "$1")
out=$(realpath "${2:-boot-test}")
firmware="${3:-bios}"
mkdir -p "$out"
work=$(mktemp -d)

# UEFI: the same firmware variables for both boots, so the boot entry the
# installer adds is there when the disk starts.
uefi=""
if [ "$firmware" = uefi ]; then
	cp "${OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}" "$work/vars.fd"
	uefi="-drive if=pflash,format=raw,readonly=on,file=${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd} -drive if=pflash,format=raw,file=$work/vars.fd"
fi

xorriso -osirrox on -indev "$iso" -extract /live/vmlinuz "$work/vmlinuz" -extract /live/initrd.img "$work/initrd.img" \
	>/dev/null 2>&1
qemu-img create -q -f qcow2 "$work/disk.qcow2" 20G

kvm=""
[ -w /dev/kvm ] && kvm="-enable-kvm -cpu host"

stop() {
	[ -f "$work/qemu.pid" ] && kill "$(cat "$work/qemu.pid")" 2>/dev/null || true
	rm -rf "$work"
}
trap stop EXIT

# shellcheck disable=SC2086 # $kvm and $uefi hold several arguments
qemu-system-x86_64 $kvm $uefi -m 4G -smp 2 \
	-kernel "$work/vmlinuz" -initrd "$work/initrd.img" \
	-append "boot=live components username=karakulak hostname=karakulak console=ttyS0 systemd.journald.forward_to_console=1 karakulak.install-test" \
	-cdrom "$iso" \
	-drive file="$work/disk.qcow2",if=virtio \
	-device virtio-vga -display none \
	-nic user,model=virtio-net-pci \
	-serial file:"$out/install-$firmware-console.log" \
	-daemonize -pidfile "$work/qemu.pid"

log="$out/install-$firmware-console.log"
echo "Installing ($firmware)..."
deadline=$(($(date +%s) + 1200))
until grep -a -q "Karakulak is installed" "$log"; do
	if grep -a -q "karakulak-install.*failed" "$log" ||
		[ "$(date +%s)" -gt "$deadline" ]; then
		echo "The installation failed. End of the console log:"
		grep -a "karakulak-install" "$log" | tail -30
		exit 1
	fi
	sleep 5
done
echo "Installed"
kill "$(cat "$work/qemu.pid")"
sleep 3

# The installed system alone: no ISO, booting from the disk with GRUB.
# shellcheck disable=SC2086
qemu-system-x86_64 $kvm $uefi -m 4G -smp 2 \
	-drive file="$work/disk.qcow2",if=virtio \
	-device virtio-vga -display none \
	-nic user,model=virtio-net-pci \
	-monitor unix:"$work/monitor.sock",server,nowait \
	-daemonize -pidfile "$work/qemu.pid"

# The installed system shows the lock screen (a Firefox page) at start.
echo "Waiting for the installed system to start..."
sleep 150
echo "screendump $work/installed.ppm" | socat - UNIX-CONNECT:"$work/monitor.sock" >/dev/null
sleep 2
python3 "$(dirname "$0")/check_screen.py" "$work/installed.ppm" "$out/installed-$firmware.png"
