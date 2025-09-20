#!/bin/bash -eu

# file paths
QEMU=qemu-system-riscv32
OBJCOPY=llvm-objcopy

# Build the user program
cargo build --release -p user --bin user --target riscv32imac-unknown-none-elf

# Embed the user program in the binary.
$OBJCOPY --set-section-flags .bss=alloc,contents -O binary target/riscv32imac-unknown-none-elf/release/user target/riscv32imac-unknown-none-elf/release/user.bin
$OBJCOPY -Ibinary -Oelf32-littleriscv target/riscv32imac-unknown-none-elf/release/user.bin target/riscv32imac-unknown-none-elf/release/user.bin.o

# Build the kernel
cargo build --release --bin rust-os --target riscv32imac-unknown-none-elf

# Start QEMU
$QEMU -machine virt -bios default -nographic -serial mon:stdio --no-reboot -kernel target/riscv32imac-unknown-none-elf/release/rust-os
