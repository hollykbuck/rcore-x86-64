TARGET := x86_64-unknown-none
MODE := release
APP_DIR := src/bin
TARGET_DIR := target/$(TARGET)/$(MODE)
APPS := $(wildcard $(APP_DIR)/*.rs)
ELFS := $(patsubst $(APP_DIR)/%.rs, $(TARGET_DIR)/%, $(APPS))

OBJDUMP := rust-objdump

# ch4: every app is an independent ELF linked at the same base address
# (BASE_ADDRESS in src/linker.ld); the kernel parses the ELF program headers
# itself, so no per-app relocation and no `.bin` blobs are needed.
build: $(APPS)
	@cargo build --release

elf: build

clean:
	@cargo clean

.PHONY: build elf clean
