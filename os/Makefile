TARGET := x86_64-unknown-none
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os
FS_IMG := ../user/target/$(TARGET)/$(MODE)/fs.img

LIMINE_VERSION := 12.5.2
LIMINE_DIR := $(abspath ../bootloader/limine-$(LIMINE_VERSION))
LIMINE_STAMP := $(abspath ../bootloader/.limine-$(LIMINE_VERSION))
DISK_IMG := disk.img

TEST ?=

OBJDUMP := rust-objdump
OBJCOPY := rust-objcopy

build: env fs-img

env:
	(rustup target list | grep "$(TARGET) (installed)") || rustup target add $(TARGET)
	(command -v rust-objcopy >/dev/null && command -v rust-objdump >/dev/null) || cargo install cargo-binutils
	rustup component add rust-src
	rustup component add llvm-tools-preview
	if [ ! -f "$(LIMINE_STAMP)" ]; then \
		echo "  Downloading Limine $(LIMINE_VERSION)..."; \
		curl -L --fail -o /tmp/limine-binary.tar.gz \
			"https://github.com/limine-bootloader/limine/releases/download/v$(LIMINE_VERSION)/limine-binary.tar.gz" && \
		mkdir -p "$(LIMINE_DIR)" && \
		tar xzf /tmp/limine-binary.tar.gz -C "$(LIMINE_DIR)" --strip-components=1 && \
		rm -f /tmp/limine-binary.tar.gz && \
		$(MAKE) -C "$(LIMINE_DIR)" && \
		touch "$(LIMINE_STAMP)"; \
	fi

$(KERNEL_ELF): kernel

kernel:
	@cd ../user && make build TEST=$(TEST)
	@cargo build --release

# pack the user ELFs into fs.img via the host tool easy-fs-fuse
fs-img: kernel
	@rm -f $(FS_IMG)
	@cd ../easy-fs-fuse && cargo run --release -- -s ../user/src/bin/ -t ../user/target/$(TARGET)/$(MODE)/

disk: build
	dd if=/dev/zero of=$(DISK_IMG) bs=1M count=64 2>/dev/null
	./scripts/mkdisk.sh $(DISK_IMG) $(KERNEL_ELF) $(LIMINE_DIR)

run: disk
	# -icount: make QEMU's TCG virtual clock advance with guest instructions,
	# so the APIC timer actually preempts the compute-bound apps.
	# -rtc clock=host: keep the CMOS RTC (the get_time timebase) wall-accurate.
	# -machine q35: provides the PCIe ECAM (MMIO config space @ 0xB0000000)
	# used by the virtio-blk-pci driver. The FS image is attached as a
	# second virtio-blk disk.
	@qemu-system-x86_64 -machine q35 -hda $(DISK_IMG) -m 256M -no-reboot -nographic \
		-rtc clock=host -icount shift=auto -bios /usr/share/ovmf/OVMF.fd -device isa-debug-exit \
		-global virtio-blk-pci.disable-legacy=on \
		-drive file=$(FS_IMG),format=raw,if=virtio
	@rm -f $(DISK_IMG)

clean:
	@cargo clean
	@rm -f $(DISK_IMG) os.iso
	@rm -rf iso_root
	@rm -f $(FS_IMG)

distclean: clean
	@rm -rf $(LIMINE_DIR) $(LIMINE_STAMP)

disasm: kernel
	@$(OBJDUMP) -d $(KERNEL_ELF) | less

.PHONY: build env kernel fs-img disk run clean distclean disasm
