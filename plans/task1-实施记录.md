# Task 1 实施进度记录

> 智能化工控虚拟化擂台赛 · 任务一：实时性改造与验证  
> 主文档：[os/axvisor/doc/task1-realtime.md](../os/axvisor/doc/task1-realtime.md)

---

## 进度总览

| 里程碑 | 状态 | 说明 |
|---|---|---|
| M1.1 linux-smp2 配置 | ✅ | `os/axvisor/configs/vms/qemu/aarch64/linux-smp2.toml` |
| M1.2 RT 域 arceos-rt-smp1 配置 | ✅ | pCPU3 独占分区 |
| M1.3 Zephyr 基线模板 | ✅ | `zephyr-rt-baseline.toml` |
| M1.4 裸机抖动测量框架 | ✅ | `test-suit/arceos/rust` feature `rt-latency` |
| M1.5 AxVisor linux-smp2 冒烟测试 | ✅ | `test-suit/axvisor/normal/qemu/linux-smp2/` |
| M1.6 task1 脚本 | ✅ | `os/axvisor/scripts/task1/`、`scripts/task1/run-rt-baseline.sh` |
| M2.1 vCPU 优先级抢占 | ✅ | `vcpu_priorities` + `sched-cfs` |
| M2.2 vGIC/定时器优化 | 🚧 | 中断 wake 已加；arch timer 直访待续 |
| M2.3 虚拟化 vs 裸机对比报告 | 🚧 | smoke 已接入；`RT_LATENCY mode=guest` 需 bare-metal guest 链接 |
| M3.1 长稳/stress 矩阵脚本 | ✅ | `run-stress-matrix.sh` + `rt-latency-long` feature |
| M3.2 改造前后对比自动化 | ✅ | `collect-rt-latency-report.sh` |

---

## 2026-07-10 阶段一落地内容

### 配置

- **Linux 2vCPU**：`cpu_num = 2`，`phys_cpu_ids = [1, 2]`，内存 1GiB @ `0x8000_0000`
- **RT ArceOS**：`cpu_num = 1`，`phys_cpu_ids = [3]`，内存 128MiB @ `0x4000_0000`

### 测量

- 新增 `rt-latency` 测试：1ms/10ms 周期，200 样本，输出 mean/P99/max
- 注册到 `cargo xtask arceos test qemu -c rt-latency`

### 验证命令

```bash
# 裸机 RTOS 基线
./scripts/task1/run-rt-baseline.sh

# Linux 2vCPU under AxVisor
cargo xtask axvisor test qemu --arch aarch64 -c linux-smp2

# 混合分区（手动）
cd os/axvisor && ./scripts/task1/setup-qemu-aarch64.sh && ./scripts/task1/run-mixed.sh
```

### 已知限制

1. 宿主 vCPU 调度在阶段二前为 FIFO 基线；当前已启用 `sched-cfs` 与 vCPU nice
2. arch timer 客户机直访（CNTV）尚未实现
3. RT-Thread QEMU 客户机尚未配置；Zephyr 需自行编译 `zephyr.bin`

---

## 2026-07-10 阶段二落地内容

### 调度改造

- `axvmconfig::VMBaseConfig::vcpu_priorities`：per-vCPU CFS nice
- `axvm::spawn_vcpu_task`：创建 vCPU 宿主任务后应用优先级
- `axtask::set_task_priority`：支持为任意任务设置 nice
- AxVisor / AxVM 启用 `sched-cfs`（可抢占 CFS）
- `os/axvisor/src/task.rs`：管理任务绑 pCPU0
- 中断 `queue_interrupt` 路径：`wake_task` 目标 vCPU

### 配置

- `linux-smp2.toml`：`vcpu_priorities = [10, 10]`
- `arceos-rt-smp1.toml`：`vcpu_priorities = [-20]`

### 脚本

- `os/axvisor/scripts/task1/build-arceos-rt-guest.sh`：构建 memory 加载的 rt-latency 客户机

---

## 2026-07-10 阶段二续 / 阶段三落地内容

### RT 客户机虚拟化测量

- `rt-latency-guest` / `rt-latency-long` feature：`mode=guest` 与长稳采样（180k）
- 输出增加 `p999_jitter_ns`
- 修复 `build-arceos-rt-guest.sh` 路径，统一安装到 `os/axvisor/images/qemu_aarch64_arceos_rt/`
- AxVisor CI 用例：`test-suit/axvisor/normal/arceos-rt-latency/`（smoke：`VM[2] boot success`）
- Guest 镜像拉取：`cargo xtask image pull qemu-aarch64` → `images/qemu-aarch64/arceos/arceos-qemu`

### 阶段三脚本

| 脚本 | 用途 |
|---|---|
| `scripts/task1/run-rt-guest-baseline.sh` | AxVisor guest rt-latency 短测 |
| `scripts/task1/collect-rt-latency-report.sh` | 裸机 vs guest 对比报告（`plans/task1-reports/`） |
| `scripts/task1/run-stress-matrix.sh` | 30min stress 矩阵操作说明 |

### 验证命令

```bash
# 裸机
./scripts/task1/run-rt-baseline.sh

# AxVisor RT 客户机（单 VM）
cargo xtask axvisor test qemu --arch aarch64 -c arceos-rt-latency

# 裸机 vs guest 对比报告
./scripts/task1/collect-rt-latency-report.sh

# 长稳 / stress 操作指引
./scripts/task1/run-stress-matrix.sh
```

---

## 下一步

1. **M2.2 续**：`virtualization/arm_vcpu` arch timer（CNTV）直访评估与实现
2. **Bare-metal rt-latency guest**：使 `build-arceos-rt-guest.sh` 产出 memory-load 可用的内核（当前 musl PIE 路径会 page fault）
3. **阶段三长稳实测**：混合分区 + `stress-ng` 30min，归档 `plans/task1-reports/`
4. **Zephyr / RT-Thread** QEMU 客户机基线补齐（加分项）
