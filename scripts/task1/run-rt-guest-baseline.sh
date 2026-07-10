#!/usr/bin/env bash
# Run ArceOS RT-domain smoke under AxVisor (VM boot on pCPU3).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

echo "[task1] Running ArceOS RT guest smoke under AxVisor (VM[2] boot success)..."
cargo xtask axvisor test qemu --arch aarch64 -c arceos-rt-latency
echo "[task1] For RT_LATENCY guest benchmark, use mixed partition manually:"
echo "  cd os/axvisor && ./scripts/task1/build-arceos-rt-guest.sh && ./scripts/task1/run-mixed.sh"
