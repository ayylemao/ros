LOADER_DIR = ./loader
KERNEL_DIR = ./kernel
IMG = uefi.img

TARGET_UEFI = x86_64-unknown-uefi
TARGET_KERNEL = kernel/x86_64-ros-kernel.json
TARGET_USER = user/x86_64-ros-user.json

LOADER_DEBUG = target/$(TARGET_UEFI)/debug/loader.efi
LOADER_PDB   = target/$(TARGET_UEFI)/debug/deps/loader.pdb
KERNEL_DEBUG = target/x86_64-ros-kernel/debug/kernel
USER_DEBUG = target/x86_64-ros-user/debug/user

LOADER_RELEASE = target/$(TARGET_UEFI)/release/loader.efi
KERNEL_RELEASE = target/x86_64-ros-kernel/release/kernel
USER_RELEASE = target/x86_64-ros-user/release/user

CARGO_NIGHTLY_JSON := cargo +nightly-2025-10-03


all: run

# ------------------------------------------------------------
# BUILD
# ------------------------------------------------------------

FORCE:
	$(CARGO_NIGHTLY_JSON) build --target $(TARGET_USER) -p user
	$(CARGO_NIGHTLY_JSON) build --release --target $(TARGET_USER) -p user
	mkdir -p initrfs/usr/sbin
	mkdir -p initrfs/usr/bin
	cp target/x86_64-ros-user/release/init initrfs/usr/sbin/init
	cp target/x86_64-ros-user/release/ls initrfs/usr/bin/ls
	cp target/x86_64-ros-user/release/shell initrfs/usr/bin/shell
	cp target/x86_64-ros-user/release/cat initrfs/usr/bin/cat
	cp target/x86_64-ros-user/release/sched_demo initrfs/usr/bin/sched_demo
	cp target/x86_64-ros-user/release/task_demo initrfs/usr/bin/task_demo
	tar -cf initrfs.tar initrfs/
	cargo build --target $(TARGET_UEFI) -p loader
	cargo build --release --target $(TARGET_UEFI) -p loader
	$(CARGO_NIGHTLY_JSON) build --target $(TARGET_KERNEL) -p kernel
	$(CARGO_NIGHTLY_JSON) build --release --target $(TARGET_KERNEL) -p kernel


build: FORCE


# ------------------------------------------------------------
# FAT IMAGE CREATION
# ------------------------------------------------------------

$(IMG):
	dd if=/dev/zero of=$(IMG) bs=1M count=64
	mkfs.fat -F32 $(IMG)

# ------------------------------------------------------------
# INSTALL DEBUG BUILD INTO UEFI IMAGE
# ------------------------------------------------------------

install: build $(IMG)
	mdir -i $(IMG) ::/EFI       >/dev/null 2>&1 || mmd -i $(IMG) ::/EFI
	mdir -i $(IMG) ::/EFI/BOOT  >/dev/null 2>&1 || mmd -i $(IMG) ::/EFI/BOOT

	mcopy -i $(IMG) -o $(LOADER_DEBUG) ::/EFI/BOOT/BOOTX64.EFI
	mcopy -i $(IMG) -o $(KERNEL_DEBUG) ::/kernel.elf

# ------------------------------------------------------------
# NORMAL RUN
# ------------------------------------------------------------

run: install
	qemu-system-x86_64 \
	  -machine q35 \
	  -drive if=pflash,format=raw,readonly=on,file=/usr/share/ovmf/x64/OVMF_CODE.4m.fd \
	  -drive if=pflash,format=raw,file=OVMF_VARS.fd \
	  -drive format=raw,file=$(IMG) \
# ------------------------------------------------------------
# DEBUG MODE (LLDB or GDB)
# ------------------------------------------------------------

debug: install
	@echo "[QEMU] Starting in debug mode (waiting for debugger)..."
	qemu-system-x86_64 \
	  -machine q35 \
	  -drive if=pflash,format=raw,readonly=on,file=/usr/share/ovmf/x64/OVMF_CODE.4m.fd \
	  -drive if=pflash,format=raw,file=OVMF_VARS.fd \
	  -drive format=raw,file=$(IMG) \
	  -s -S \
	  -serial mon:stdio &

# Alternative: auto-start LLDB for you
debug-lldb: install
	@echo "[QEMU] Starting in debug mode..."
	qemu-system-x86_64 \
	  -machine q35 \
	  -drive if=pflash,format=raw,readonly=on,file=/usr/share/ovmf/x64/OVMF_CODE.4m.fd \
	  -drive if=pflash,format=raw,file=OVMF_VARS.fd \
	  -drive format=raw,file=$(IMG) \
	  -s -S \
	  -serial mon:stdio &

	sleep 1

	@echo "[LLDB] Attaching..."
	lldb \
	  -o "settings set target.process.python-os-plugin-path /usr/lib/lldb" \
	  -o "target create --arch x86_64 $(LOADER_DEBUG) --symfile $(LOADER_PDB)" \
	  -o "gdb-remote localhost:1234"
	
release-build: build

release-install: release-build $(IMG)
	mdir -i $(IMG) ::/EFI       >/dev/null 2>&1 || mmd -i $(IMG) ::/EFI
	mdir -i $(IMG) ::/EFI/BOOT  >/dev/null 2>&1 || mmd -i $(IMG) ::/EFI/BOOT

	mcopy -i $(IMG) -o $(LOADER_RELEASE) ::/EFI/BOOT/BOOTX64.EFI
	mcopy -i $(IMG) -o $(KERNEL_RELEASE) ::/kernel.elf

run-release: release-install
	qemu-system-x86_64 \
	  -machine q35 \
	  -drive if=pflash,format=raw,readonly=on,file=/usr/share/ovmf/x64/OVMF_CODE.4m.fd \
	  -drive if=pflash,format=raw,file=OVMF_VARS.fd \
	  -drive format=raw,file=$(IMG)

release: run-release
# ------------------------------------------------------------
# CLEAN
# ------------------------------------------------------------

clean:
	cargo clean
	rm -f $(IMG)