#!/bin/bash -eu

# QEMU file path
QEMU=qemu-system-riscv32

# Build the binary
cargo build --release --bin rust-os --target riscv32imac-unknown-none-elf

# Start QEMU
$QEMU -machine virt -bios default -nographic -serial mon:stdio --no-reboot -kernel target/riscv32imac-unknown-none-elf/release/rust-os
