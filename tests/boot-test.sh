#!/bin/sh
# Boots the ISO in QEMU without a window, waits for the session, and takes a
# screenshot. Fails if auto-login never happens or the screen stays blank.
#
# Usage: tests/boot-test.sh karakulak.iso [output-dir]
# Needs: qemu-system-x86_64, xorriso, socat, python3
set -e

iso=$(realpath "$1")
out=$(realpath "${2:-boot-test}")
mkdir -p "$out"
work=$(mktemp -d)

# Boot the live system's kernel directly, so no boot menu is in the way and
# the console goes to a log file.
xorriso -osirrox on -indev "$iso" -extract /live/vmlinuz "$work/vmlinuz" -extract /live/initrd.img "$work/initrd.img" \
	>/dev/null 2>&1

kvm=""
[ -w /dev/kvm ] && kvm="-enable-kvm -cpu host"

# shellcheck disable=SC2086 # $kvm holds several arguments
qemu-system-x86_64 $kvm -m 4G -smp 2 \
	-kernel "$work/vmlinuz" -initrd "$work/initrd.img" \
	-append "boot=live components username=karakulak hostname=karakulak console=ttyS0 systemd.show_status=1" \
	-cdrom "$iso" \
	-device virtio-vga -display none \
	-nic user,model=virtio-net-pci \
	-serial file:"$out/console.log" \
	-monitor unix:"$work/monitor.sock",server,nowait \
	-daemonize -pidfile "$work/qemu.pid"

stop() {
	kill "$(cat "$work/qemu.pid")" 2>/dev/null || true
	rm -rf "$work"
}
trap stop EXIT

echo "Waiting for auto-login..."
deadline=$(($(date +%s) + 300))
until grep -q "karakulak (automatic login)" "$out/console.log" 2>/dev/null; do
	if [ "$(date +%s)" -gt "$deadline" ]; then
		echo "No auto-login after 5 minutes. Console log:"
		tail -50 "$out/console.log"
		exit 1
	fi
	sleep 5
done

echo "Logged in; giving the compositor and Firefox time to start..."
sleep 90
echo "screendump $work/screen.ppm" | socat - UNIX-CONNECT:"$work/monitor.sock" >/dev/null
sleep 2
python3 "$(dirname "$0")/check_screen.py" "$work/screen.ppm" "$out/screen.png"
