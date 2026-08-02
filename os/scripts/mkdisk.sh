#!/bin/sh
set -e
DISK="$1"
KERNEL="$2"
LIMINE="$3"

printf 'label: gpt\n,,U\n' | sfdisk "$DISK" 2>/dev/null > /dev/null

START=$(sfdisk -l -o Start "$DISK" 2>/dev/null | awk 'END{print $1}')
SECTORS=$(sfdisk -l -o Sectors "$DISK" 2>/dev/null | awk 'END{print $1}')

mkfs.fat -F 32 --offset="$START" "$DISK" $((SECTORS / 2)) 2>/dev/null
OFFSET=$((START * 512))

mmd -i "$DISK@@$OFFSET" ::EFI 2>/dev/null
mmd -i "$DISK@@$OFFSET" ::EFI/BOOT 2>/dev/null
mmd -i "$DISK@@$OFFSET" ::boot 2>/dev/null
mcopy -i "$DISK@@$OFFSET" "$LIMINE/BOOTX64.EFI" ::EFI/BOOT/
mcopy -i "$DISK@@$OFFSET" "$LIMINE/BOOTIA32.EFI" ::EFI/BOOT/ 2>/dev/null || true
mcopy -i "$DISK@@$OFFSET" "$KERNEL" ::boot/os
mcopy -i "$DISK@@$OFFSET" "$(dirname "$0")/../limine.conf" ::boot/
