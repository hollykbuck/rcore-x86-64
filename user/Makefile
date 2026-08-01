TARGET := x86_64-unknown-none
MODE := release
APP_DIR := src/bin
TARGET_DIR := target/$(TARGET)/$(MODE)
APPS := $(wildcard $(APP_DIR)/*.rs)
ELFS := $(patsubst $(APP_DIR)/%.rs, $(TARGET_DIR)/%, $(APPS))

OBJDUMP := rust-objdump
CP := cp

TEST ?=

# every app is an independent ELF linked at the same base address
# (BASE_ADDRESS in src/linker.ld); the kernel parses the ELF program headers
# itself, so no per-app relocation and no `.bin` blobs are needed.
# The `touch` below forces cargo to re-link `initproc`: `make run TEST=1`
# overwrites `target/.../initproc` with `usertests`, and without it cargo would
# consider the (tampered) product up to date on the next build.
build: $(APPS)
	@touch src/bin/initproc.rs
	@cargo build --release
ifeq ($(TEST), 1)
	@$(CP) $(TARGET_DIR)/usertests $(TARGET_DIR)/initproc
endif

elf: build

clean:
	@cargo clean

.PHONY: build elf clean
