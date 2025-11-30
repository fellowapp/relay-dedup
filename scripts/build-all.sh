#!/bin/bash
set -e

# Build and test all platform binaries locally
# Requires: cargo, cross (cargo install cross), Docker

BINARY_NAME="relay-dedup"
DIST_DIR="dist"

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

echo "=== Building and testing all platforms ==="

# macOS ARM64 (native on M1/M2, cross from Intel)
echo ""
echo "→ aarch64-apple-darwin (macOS ARM64)"
if [[ $(uname -m) == "arm64" ]]; then
    echo "  Building..."
    cargo build --release --target aarch64-apple-darwin
    echo "  Testing..."
    cargo test --release --target aarch64-apple-darwin
else
    echo "  Skipping (not on ARM Mac)"
fi

# macOS x64 (native on Intel, cross from ARM)
echo ""
echo "→ x86_64-apple-darwin (macOS x64)"
rustup target add x86_64-apple-darwin 2>/dev/null || true
echo "  Building..."
cargo build --release --target x86_64-apple-darwin
echo "  Testing..."
cargo test --release --target x86_64-apple-darwin

# Linux x64 (via cross/Docker)
echo ""
echo "→ x86_64-unknown-linux-musl (Linux x64)"
echo "  Building..."
cross build --release --target x86_64-unknown-linux-musl
echo "  Testing (in Docker)..."
cross test --release --target x86_64-unknown-linux-musl

# Linux ARM64 (via cross/Docker)
echo ""
echo "→ aarch64-unknown-linux-musl (Linux ARM64)"
echo "  Building..."
cross build --release --target aarch64-unknown-linux-musl
echo "  Testing (in Docker via QEMU)..."
cross test --release --target aarch64-unknown-linux-musl

echo ""
echo "=== Packaging ==="

for target in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
    if [[ -f "target/$target/release/$BINARY_NAME" ]]; then
        echo "→ $BINARY_NAME-$target.tar.gz"
        tar -czvf "$DIST_DIR/$BINARY_NAME-$target.tar.gz" -C "target/$target/release" "$BINARY_NAME"
    fi
done

echo ""
echo "=== All builds passed tests ✓ ==="
ls -lh "$DIST_DIR"
