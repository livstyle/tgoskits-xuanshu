#!/usr/bin/env bash
# Build ArceOS rt-latency guest image for AxVisor memory-loaded RT VM.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AXVISOR_ROOT="${REPO_ROOT}/os/axvisor"
IMAGE_DIR="${AXVISOR_ROOT}/images/qemu_aarch64_arceos_rt"
OUTPUT_NAME="qemu-aarch64-rt-latency-bench"

info() { echo "[task1] $*"; }

info "Building ArceOS rt-latency guest (aarch64)..."
cd "${REPO_ROOT}"
cargo xtask arceos build --arch aarch64 -g rust --features rt-latency

BIN_SRC="${REPO_ROOT}/tmp/axbuild/arceos/aarch64/rust/rt-latency/arceos-test-suit"
if [[ ! -f "${BIN_SRC}" ]]; then
  BIN_SRC="${REPO_ROOT}/tmp/axbuild/arceos/aarch64/rust/rt-latency/arceos-test-suit.bin"
fi
if [[ ! -f "${BIN_SRC}" ]]; then
  echo "rt-latency guest binary not found under tmp/axbuild/arceos/aarch64/rust/rt-latency/" >&2
  exit 1
fi

mkdir -p "${IMAGE_DIR}"
install -m 0644 "${BIN_SRC}" "${IMAGE_DIR}/${OUTPUT_NAME}"
info "Installed ${IMAGE_DIR}/${OUTPUT_NAME}"
info "Use with configs/vms/qemu/aarch64/arceos-rt-smp1.toml (image_location = memory)"
