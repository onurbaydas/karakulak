#!/bin/sh
# Try Karakulak in a virtual machine (QEMU).
#
#   ./vm.sh              newest ISO built by GitHub Actions (needs `gh` login)
#   ./vm.sh file.iso     a local ISO
#   ./vm.sh --fresh ...  start with an empty virtual disk
#
# A 20 GB virtual disk (~/.cache/karakulak-vm/disk.qcow2) is attached for
# trying the installer. Set KARAKULAK_VM_GL=0 if the window stays black
# (no 3D acceleration on the host).
set -e

dir="$HOME/.cache/karakulak-vm"
mkdir -p "$dir"

if [ "$1" = "--fresh" ]; then
	rm -f "$dir/disk.qcow2"
	shift
fi

iso="$1"
if [ -z "$iso" ]; then
	run=$(gh run list --workflow Build --branch main --status success --limit 1 --json databaseId --jq '.[0].databaseId')
	iso="$dir/karakulak-$run.iso"
	if [ ! -f "$iso" ]; then
		echo "Downloading the ISO from build $run..."
		rm -rf "$dir/download"
		gh run download "$run" --name karakulak-iso --dir "$dir/download"
		mv "$dir"/download/*.iso "$iso"
		rm -rf "$dir/download"
	fi
fi

[ -f "$dir/disk.qcow2" ] || qemu-img create -q -f qcow2 "$dir/disk.qcow2" 20G

if [ "${KARAKULAK_VM_GL:-1}" = 1 ]; then
	graphics="-device virtio-vga-gl -display gtk,gl=on"
else
	graphics="-device virtio-vga -display gtk"
fi

echo "Starting $iso"
# shellcheck disable=SC2086 # $graphics holds several arguments
exec qemu-system-x86_64 \
	-enable-kvm -machine q35 -cpu host -smp 4 -m 4G \
	-cdrom "$iso" -boot order=dc \
	-drive file="$dir/disk.qcow2",if=virtio \
	-nic user,model=virtio-net-pci \
	-device qemu-xhci -device usb-tablet \
	-audiodev pipewire,id=snd -device intel-hda -device hda-duplex,audiodev=snd \
	$graphics
