#!/bin/bash -eu

# file paths
QEMU=qemu-system-riscv32
OBJCOPY=llvm-objcopy
SCRATCH_DIR="$(mktemp -d /tmp/rust-os.XXXXXX)"
clean_scratch() {
    rm --recursive --one-file-system --preserve-root=all "$SCRATCH_DIR"
}
trap clean_scratch EXIT

# Build the user program
cargo build --release -p shell --bin shell --target riscv32imac-unknown-none-elf
# Convert it to raw binary data for including in the build
$OBJCOPY --set-section-flags .bss=alloc,contents -O binary target/riscv32imac-unknown-none-elf/release/shell target/riscv32imac-unknown-none-elf/release/shell.bin

# Build the kernel
cargo build --release --bin rust-os --target riscv32imac-unknown-none-elf

FS_PATH="$SCRATCH_DIR/fs.bin"
# FS size: 1MB
dd if=/dev/zero of="$FS_PATH" bs=1M count=1
# Use 128 byte inodes so I don't have to worry about extra data yet.
mkfs.ext2 -I 128 -E root_owner="$(id -u):$(id -g)" "$FS_PATH"

FS_MOUNT="$SCRATCH_DIR/fs-mnt"
echo "Mounting FS at $FS_MOUNT..."
mkdir "$FS_MOUNT"
fuse2fs -o rw,uid=$(id -u),gid=$(id -g),allow_other "$FS_PATH" "$FS_MOUNT"
echo "Lorem ipsum dolor sit amet, consectetur adipiscing elit. In ut magna consequat, cursus velit aliquam, scelerisque odio. Ut lorem eros, feugiat quis bibendum vitae, malesuada ac orci. Praesent eget quam non nunc fringilla cursus imperdiet non tellus. Aenean dictum lobortis turpis, non interdum leo rhoncus sed. Cras in tellus auctor, faucibus tortor ut, maximus metus. Praesent placerat ut magna non tristique. Pellentesque at nunc quis dui tempor vulputate. Vestibulum vitae massa orci. Mauris et tellus quis risus sagittis placerat. Integer lorem leo, feugiat sed molestie non, viverra a tellus." > "$FS_MOUNT/lorem-ipsum.txt"
fusermount -u "$FS_MOUNT" 

# Start QEMU
$QEMU -machine virt -bios default -nographic -serial mon:stdio --no-reboot \
    -drive id=drive0,file="$FS_PATH",format=raw,if=none \
    -device virtio-blk-device,drive=drive0,bus=virtio-mmio-bus.0 \
    -device virtio-rng-device,bus=virtio-mmio-bus.1 \
    -kernel target/riscv32imac-unknown-none-elf/release/rust-os
