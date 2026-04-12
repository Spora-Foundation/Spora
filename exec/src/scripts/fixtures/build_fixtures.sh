#!/bin/bash
# Build script for RISC-V ELF fixtures
#
# Prerequisites:
# - Rust nightly toolchain with riscv64imac-unknown-none-elf target
# - Or: riscv64-unknown-elf-gcc toolchain
#
# Install Rust target:
#   rustup target add riscv64imac-unknown-none-elf

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Building RISC-V ELF fixtures..."

# Check for Rust target
if rustup target list --installed | grep -q "riscv64imac-unknown-none-elf"; then
    echo "Using Rust riscv64imac-unknown-none-elf target"
    USE_RUST=1
else
    echo "Warning: riscv64imac-unknown-none-elf target not installed"
    echo "Install with: rustup target add riscv64imac-unknown-none-elf"
    echo ""
    echo "Alternatively, install RISC-V GCC toolchain:"
    echo "  brew install riscv-tools  # macOS"
    echo "  apt-get install gcc-riscv64-linux-gnu  # Ubuntu"
    exit 1
fi

# Build always_success
if [ $USE_RUST -eq 1 ]; then
    echo "Building always_success.elf..."
    rustc --edition 2021 \
        --target riscv64imac-unknown-none-elf \
        -C opt-level=3 \
        -o always_success.elf \
        always_success.rs
fi

# Build load_input_since
echo "Building load_input_since.elf..."
rustc --edition 2021 \
    --target riscv64imac-unknown-none-elf \
    -C opt-level=3 \
    -o load_input_since.elf \
    load_input_since.rs

# Build load_header_timestamp
echo "Building load_header_timestamp.elf..."
rustc --edition 2021 \
    --target riscv64imac-unknown-none-elf \
    -C opt-level=3 \
    -o load_header_timestamp.elf \
    load_header_timestamp.rs

# Build load_dep_cell_data
echo "Building load_dep_cell_data.elf..."
rustc --edition 2021 \
    --target riscv64imac-unknown-none-elf \
    -C opt-level=3 \
    -o load_dep_cell_data.elf \
    load_dep_cell_data.rs

# Build timelock_absolute
echo "Building timelock_absolute.elf..."
rustc --edition 2021 \
    --target riscv64imac-unknown-none-elf \
    -C opt-level=3 \
    -o timelock_absolute.elf \
    timelock_absolute.rs

# Build timelock_relative
echo "Building timelock_relative.elf..."
rustc --edition 2021 \
    --target riscv64imac-unknown-none-elf \
    -C opt-level=3 \
    -o timelock_relative.elf \
    timelock_relative.rs

# Build htlc_minimal
echo "Building htlc_minimal.elf..."
rustc --edition 2021 \
    --target riscv64imac-unknown-none-elf \
    -C opt-level=3 \
    -o htlc_minimal.elf \
    htlc_minimal.rs

# Build htlc
echo "Building htlc.elf..."
rustc --edition 2021 \
    --target riscv64imac-unknown-none-elf \
    -C opt-level=3 \
    -o htlc.elf \
    htlc.rs

echo ""
echo "Build complete!"
echo ""
HASH_TOOL=""
if command -v blake3sum >/dev/null 2>&1; then
    HASH_TOOL="blake3sum"
elif command -v b3sum >/dev/null 2>&1; then
    HASH_TOOL="b3sum"
fi

print_hash() {
    local label="$1"
    local file="$2"
    if [ -n "$HASH_TOOL" ]; then
        echo "  ${label}: $(${HASH_TOOL} "${file}" | cut -d' ' -f1)"
    else
        echo "  ${label}: <install blake3sum or b3sum to print hash>"
    fi
}

echo "Code hashes:"
print_hash "always_success" "always_success.elf"
print_hash "load_input_since" "load_input_since.elf"
print_hash "load_header_timestamp" "load_header_timestamp.elf"
print_hash "load_dep_cell_data" "load_dep_cell_data.elf"
print_hash "timelock_absolute" "timelock_absolute.elf"
print_hash "timelock_relative" "timelock_relative.elf"
print_hash "htlc_minimal" "htlc_minimal.elf"
print_hash "htlc" "htlc.elf"
