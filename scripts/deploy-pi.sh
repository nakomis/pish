#!/bin/bash
# Build and deploy pish to the Raspberry Pi.
# Usage: ./scripts/deploy-pi.sh [PI_HOST]
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET="aarch64-unknown-linux-gnu"
BINARY="target/${TARGET}/release/pish"
SERVICE_FILE="${SCRIPT_DIR}/../pish.service"
PI_HOST="${1:-pish}"

echo "==> Building for ${TARGET}..."
"${SCRIPT_DIR}/build-pi.sh"

echo "==> Copying binary and service file to ${PI_HOST}..."
scp "${BINARY}" "${PI_HOST}:/tmp/pish"
scp "${SERVICE_FILE}" "${PI_HOST}:/tmp/pish.service"

echo "==> Installing on ${PI_HOST}..."
ssh "${PI_HOST}" bash << 'REMOTE'
set -e
sudo mv /tmp/pish /usr/local/bin/pish
sudo chmod 755 /usr/local/bin/pish
sudo mv /tmp/pish.service /etc/systemd/system/pish.service
sudo systemctl daemon-reload
sudo systemctl enable pish
sudo systemctl restart pish
sudo systemctl status pish --no-pager
REMOTE

echo "==> Done."
