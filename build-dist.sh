#!/usr/bin/env bash
# Clonaris Local Multi-Platform Build Script
set -e

echo "🔨 Building Clonaris for Linux (glibc)..."
cargo build --release
mkdir -p dist
cp target/release/clonaris dist/clonaris-linux-x86_64

echo "🔨 Building Clonaris for Linux (Musl Static)..."
cargo build --release --target x86_64-unknown-linux-musl
cp target/x86_64-unknown-linux-musl/release/clonaris dist/clonaris-linux-x86_64-musl

echo "✅ Local binaries compiled successfully in dist/:"
ls -lh dist/
