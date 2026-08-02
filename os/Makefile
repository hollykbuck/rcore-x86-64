TARGET := x86_64-unknown-none
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os

LIMINE_VERSION := 12.5.2
LIMINE_DIR := $(abspath ../bootloader/limine-$(LIMINE_VERSION))
LIMINE_STAMP := $(abspath ../bootloader/.limine-$(LIMINE_VERSION))
DISK_IMG := disk.img

OBJDUMP := rust-objdump
OBJCOPY := rust-objcopy

build: env $(KERNEL_ELF)

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
	@cd ../user && make build
	@cargo build --release

disk: build
	dd if=/dev/zero of=$(DISK_IMG) bs=1M count=64 2>/dev/null
	./scripts/mkdisk.sh $(DISK_IMG) $(KERNEL_ELF) $(LIMINE_DIR)

run: disk
	@qemu-system-x86_64 -hda $(DISK_IMG) -m 256M -no-reboot -nographic \
		-bios /usr/share/ovmf/OVMF.fd -device isa-debug-exit
	@rm -f $(DISK_IMG)

clean:
	@cargo clean
	@rm -f $(DISK_IMG) os.iso
	@rm -rf iso_root

distclean: clean
	@rm -rf $(LIMINE_DIR) $(LIMINE_STAMP)

disasm: kernel
	@$(OBJDUMP) -d $(KERNEL_ELF) | less

.PHONY: build env kernel disk run clean distclean disasm
