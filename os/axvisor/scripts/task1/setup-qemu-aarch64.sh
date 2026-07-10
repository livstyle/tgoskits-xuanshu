#!/usr/bin/env bash
# Task 1 phase-1: prepare QEMU aarch64 configs for Linux 2vCPU + RT-domain layout.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TGOSKITS_ROOT="$(cd "${REPO_ROOT}/../.." && pwd)"
cd "${REPO_ROOT}"

info() { echo "[task1] $*"; }

info "=== Task 1 QEMU aarch64 setup ==="

mkdir -p tmp/{configs,images}

info "Pulling guest images (linux + arceos)..."
cargo axvisor image pull qemu_aarch64_linux --output-dir tmp/images
cargo axvisor image pull qemu_aarch64_arceos --output-dir tmp/images

info "Preparing board + guest VM configs..."
cp configs/board/qemu-aarch64.toml tmp/configs/
cp configs/vms/qemu/aarch64/linux-smp2.toml tmp/configs/linux-aarch64-qemu-smp2.toml
cp configs/vms/qemu/aarch64/arceos-rt-smp1.toml tmp/configs/arceos-rt-aarch64-qemu-smp1.toml
cp .github/workflows/qemu-aarch64.toml tmp/configs/qemu-aarch64-runtime.toml

sed -i 's|^kernel_path = .*|kernel_path = "../images/qemu_aarch64_linux/qemu-aarch64"|g' \
  tmp/configs/linux-aarch64-qemu-smp2.toml
sed -i 's|^image_location = "fs"|image_location = "memory"|g' \
  tmp/configs/linux-aarch64-qemu-smp2.toml

# RT guest: default to pulled arceos image until custom rt-latency guest is built.
sed -i 's|^kernel_path = .*|kernel_path = "../images/qemu_aarch64_arceos/qemu-aarch64"|g' \
  tmp/configs/arceos-rt-aarch64-qemu-smp1.toml

ROOTFS_PATH="$(pwd)/tmp/images/qemu_aarch64_linux/rootfs.img"
sed -i 's|^  # "-drive",$|  "-drive",|g' tmp/configs/qemu-aarch64-runtime.toml
sed -i 's|^  # "id=disk0,if=none,format=raw,file=|  "id=disk0,if=none,format=raw,file=|g' \
  tmp/configs/qemu-aarch64-runtime.toml
sed -i 's|file=${workspaceFolder}/tmp/rootfs.img|file='"${ROOTFS_PATH}"'|g' \
  tmp/configs/qemu-aarch64-runtime.toml
sed -i '/success_regex = \[/,/\]/c\success_regex = []' tmp/configs/qemu-aarch64-runtime.toml

cat > tmp/configs/task1-pcpu-layout.md <<'EOF'
# Task 1 pCPU layout (QEMU virt, -smp 4)

| pCPU | Role |
|------|------|
| 0 | AxVisor host |
| 1-2 | Linux guest vCPU0-1 (`linux-smp2.toml`, phys_cpu_ids = [1, 2]) |
| 3 | RT guest vCPU0 (`arceos-rt-smp1.toml`, phys_cpu_ids = [3]) |
EOF

info "Wrote tmp/configs/task1-pcpu-layout.md"
info "=== Task 1 setup complete ==="
info "Run Linux 2vCPU only:"
info "  ./scripts/task1/run-linux-smp2.sh"
info "Run mixed Linux + RT partition:"
info "  ./scripts/task1/run-mixed.sh"
info "Collect bare-metal RTOS jitter baseline:"
info "  ${TGOSKITS_ROOT}/scripts/task1/run-rt-baseline.sh"
