#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   ./ship_usb.sh path/to/uefi.img
#
# This will ERASE /dev/sda and copy the contents of the FAT image onto it.
# USB device is assumed to ALWAYS be /dev/sda (as requested).

DEV="/dev/sda"
IMG="${1:-}"

if [[ -z "${IMG}" || ! -f "${IMG}" ]]; then
  echo "Usage: $0 path/to/uefi.img"
  exit 1
fi

echo "About to ERASE and reformat ${DEV} and copy from: ${IMG}"
echo "If this is not your USB stick, abort now (Ctrl+C)."
sleep 2

# Unmount anything currently mounted from the device
sudo umount "${DEV}"* 2>/dev/null || true

echo "[1/5] Creating GPT + ESP partition on ${DEV}..."
sudo parted -s "${DEV}" mklabel gpt
sudo parted -s "${DEV}" mkpart ESP fat32 1MiB 100%
sudo parted -s "${DEV}" set 1 esp on

echo "[2/5] Formatting ${DEV}1 as FAT32..."
sudo mkfs.fat -F32 -n ROSBOOT "${DEV}1" >/dev/null

USB_MNT="$(mktemp -d /tmp/ros-usb.XXXXXX)"
IMG_MNT="$(mktemp -d /tmp/ros-img.XXXXXX)"
cleanup() {
  sudo umount "${IMG_MNT}" 2>/dev/null || true
  sudo umount "${USB_MNT}" 2>/dev/null || true
  rmdir "${IMG_MNT}" "${USB_MNT}" 2>/dev/null || true
}
trap cleanup EXIT

echo "[3/5] Mounting USB and image..."
sudo mount "${DEV}1" "${USB_MNT}"
sudo mount -o loop "${IMG}" "${IMG_MNT}"

echo "[4/5] Copying image contents to USB..."
# Copy everything (preserves directory structure and timestamps)
sudo cp -a "${IMG_MNT}/." "${USB_MNT}/"
sync

echo "[5/5] Verifying EFI boot path..."
if sudo test -f "${USB_MNT}/EFI/BOOT/BOOTX64.EFI"; then
  echo "OK: Found EFI/BOOT/BOOTX64.EFI"
else
  echo "ERROR: EFI/BOOT/BOOTX64.EFI not found on USB after copy!"
  echo "Contents of ${USB_MNT}/EFI/BOOT (if any):"
  sudo ls -la "${USB_MNT}/EFI/BOOT" 2>/dev/null || true
  exit 2
fi

echo "Done. USB is ready on ${DEV}."

