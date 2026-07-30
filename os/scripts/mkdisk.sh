#!/bin/sh
set -e
DISK="$1"
KERNEL="$2"
LIMINE="$3"

START=$(sfdisk -l -o Start "$DISK" 2>/dev/null | awk 'END{print $1}')
SECTORS=$(sfdisk -l -o Sectors "$DISK" 2>/dev/null | awk 'END{print $1}')

mkfs.fat -F 32 --offset="$START" "$DISK" $((SECTORS / 2)) 2>/dev/null

OFFSET=$((START * 512))

mmd -i "$DISK@@$OFFSET" ::boot 2>/dev/null
mcopy -i "$DISK@@$OFFSET" "$KERNEL" ::boot/os
mcopy -i "$DISK@@$OFFSET" "$(dirname "$0")/../limine.conf" ::boot/
mcopy -i "$DISK@@$OFFSET" "$LIMINE/limine-bios.sys" ::/

"$LIMINE/limine" bios-install "$DISK" 2>/dev/null
