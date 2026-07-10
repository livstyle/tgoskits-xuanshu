#!/usr/bin/env bash
# Build ArceOS rt-latency guest image for AxVisor memory-loaded RT VM.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AXVISOR_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TGOSKITS_ROOT="$(cd "${AXVISOR_ROOT}/../.." && pwd)"
IMAGE_DIR="${AXVISOR_ROOT}/images/qemu_aarch64_arceos_rt"
OUTPUT_NAME="qemu-aarch64-rt-latency-bench"
TARGET="aarch64-unknown-none-softfloat"
PACKAGE="arceos-test-suit"

info() { echo "[task1] $*"; }

info "Building ArceOS rt-latency guest (${TARGET})..."
cd "${TGOSKITS_ROOT}"
cargo xtask arceos build --arch aarch64 \
  -p "${PACKAGE}" \
  -c test-suit/arceos/rust/build-aarch64-rt-latency-guest.toml

BIN_SRC="${TGOSKITS_ROOT}/target/aarch64-unknown-linux-musl/release/${PACKAGE}.bin"
if [[ ! -f "${BIN_SRC}" ]]; then
  BIN_SRC="${TGOSKITS_ROOT}/target/${TARGET}/release/${PACKAGE}.bin"
fi
if [[ ! -f "${BIN_SRC}" ]]; then
  BIN_SRC="${TGOSKITS_ROOT}/target/aarch64-unknown-linux-musl/release/${PACKAGE}"
fi
if [[ ! -f "${BIN_SRC}" ]]; then
  BIN_SRC="${TGOSKITS_ROOT}/target/${TARGET}/release/${PACKAGE}"
fi
if [[ ! -f "${BIN_SRC}" ]]; then
  echo "rt-latency guest binary not found under target/${TARGET}/release/" >&2
  exit 1
fi

mkdir -p "${IMAGE_DIR}"
install -m 0644 "${BIN_SRC}" "${IMAGE_DIR}/${OUTPUT_NAME}"

# Keep manual setup (`tmp/configs` + `tmp/images`) in sync when present.
TMP_IMAGE_DIR="${AXVISOR_ROOT}/tmp/images/qemu_aarch64_arceos_rt"
mkdir -p "${TMP_IMAGE_DIR}"
install -m 0644 "${BIN_SRC}" "${TMP_IMAGE_DIR}/${OUTPUT_NAME}"

info "Installed ${IMAGE_DIR}/${OUTPUT_NAME}"
info "Also synced ${TMP_IMAGE_DIR}/${OUTPUT_NAME}"
info "Use with configs/vms/qemu/aarch64/arceos-rt-smp1.toml (image_location = memory)"
