#!/bin/bash
# Cross-compile pish for Raspberry Pi (aarch64).
set -e

TARGET="aarch64-unknown-linux-gnu"

echo "==> Building pish for ${TARGET}..."
cargo zigbuild --release --target "${TARGET}"
echo "==> Binary: target/${TARGET}/release/pish"
