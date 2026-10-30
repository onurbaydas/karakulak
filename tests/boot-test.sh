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
	-append "boot=live components username=karakulak hostname=karakulak console=ttyS0 systemd.journald.forward_to_console=1" \
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

# The journal is forwarded to the serial console, so the log shows the
# auto-login on tty1 and the compositor's own messages.
wait_for() {
	echo "Waiting for: $2"
	deadline=$(($(date +%s) + $1))
	until grep -a -q "$3" "$out/console.log" 2>/dev/null; do
		if [ "$(date +%s)" -gt "$deadline" ]; then
			echo "Gave up. End of the console log:"
			tail -60 "$out/console.log"
			exit 1
		fi
		sleep 5
	done
}

wait_for 300 "auto-login" "New session .* of user karakulak"
wait_for 120 "the compositor to light up a screen" "screen connected"

echo "Giving Firefox time to start..."
sleep 60
echo "screendump $work/screen.ppm" | socat - UNIX-CONNECT:"$work/monitor.sock" >/dev/null
sleep 2
python3 "$(dirname "$0")/check_screen.py" "$work/screen.ppm" "$out/screen.png"
