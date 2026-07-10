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
| M2.2 vGIC/定时器优化 | 🚧 | 中断 wake 已加；vGIC/直访待续 |
| M2.3 虚拟化 vs 裸机对比报告 | 🚧 | 需 `build-arceos-rt-guest.sh` + 混合运行 |

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

1. 宿主 vCPU 调度仍为 FIFO，尚未做实时化改造（保留作「改造前」基线）
2. RT 客户机当前使用 pulled ArceOS 镜像，自定义 `rt-latency` 客户机镜像待阶段二集成
3. RT-Thread QEMU 客户机尚未配置，Zephyr 需自行编译 `zephyr.bin`

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

## 下一步（阶段二续 / 阶段三）
