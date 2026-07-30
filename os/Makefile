TARGET := x86_64-unknown-none
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os

LIMINE_DIR := $(abspath ../bootloader/limine-v12)
DISK_IMG := disk.img

OBJDUMP := rust-objdump
OBJCOPY := rust-objcopy

build: env $(KERNEL_ELF)

env:
	(rustup target list | grep "$(TARGET) (installed)") || rustup target add $(TARGET)
	(command -v rust-objcopy >/dev/null && command -v rust-objdump >/dev/null) || cargo install cargo-binutils
	rustup component add rust-src
	rustup component add llvm-tools-preview

$(KERNEL_ELF): kernel

kernel:
	@cargo build --release

disk: build
	dd if=/dev/zero of=$(DISK_IMG) bs=1M count=64 2>/dev/null
	printf ',,0x0C,*\n' | sfdisk $(DISK_IMG) 2>/dev/null > /dev/null
	./scripts/mkdisk.sh $(DISK_IMG) $(KERNEL_ELF) $(LIMINE_DIR)

run: disk
	@qemu-system-x86_64 -hda $(DISK_IMG) -m 256M -no-reboot -nographic -device isa-debug-exit
	@rm -f $(DISK_IMG)

clean:
	@cargo clean
	@rm -f $(DISK_IMG) os.iso
	@rm -rf iso_root

disasm: kernel
	@$(OBJDUMP) -d $(KERNEL_ELF) | less

.PHONY: build env kernel disk run clean disasm
