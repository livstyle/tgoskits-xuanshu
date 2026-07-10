# Task 1 交卷状态摘要

> 最后更新：2026-07-10  
> 矩阵报告：[`matrix-20260710T102947Z.md`](./matrix-20260710T102947Z.md)  
> stress post-opt：[`mixed-stress-round1-20260710T091425Z.md`](./mixed-stress-round1-20260710T091425Z.md)  
> stress pre/post：[`stress-pre-post-20260710T105300Z.md`](./stress-pre-post-20260710T105300Z.md)

## 已完成（可复现）

| 项 | 证据 |
|---|---|
| 混合分区拓扑 Linux+RT | `linux-smp2.toml` + `arceos-rt-smp1.toml`，`mixed-rt-stress-round1` PASS |
| 调度/抢占改造 | `sched-cfs`、`vcpu_priorities`、中断 wake |
| 定时器/GIC | timer passthrough、`rt-latency` + `paging` |
| 裸机基线 | `run-rt-baseline.sh` → `mode=bare` |
| Guest idle 短测 | `arceos-rt-latency-guest` → `RT_LATENCY_PASS` |
| Guest pre/post 对比 | `arceos-rt-latency-guest-pre-opt` vs post-opt |
| 30min stress 长稳（post-opt） | 180k 样本，`RT_LATENCY_PASS` |
| 30min stress 长稳（pre-opt） | `mixed-rt-stress-round1-pre-opt`，180k 样本，`RT_LATENCY_PASS` |
| stress pre/post 对比报告 | `stress-pre-post-20260710T105300Z.md` |
| 多 RTOS smoke | ArceOS / Zephyr / RT-Thread |

一键复现：

```bash
./scripts/task1/collect-task1-matrix-report.sh   # ~20s，idle 矩阵
# 长稳 stress ~35min：
cargo xtask axvisor test qemu --arch aarch64 -g stress -c mixed-rt-stress-round1
cargo xtask axvisor test qemu --arch aarch64 -g stress -c mixed-rt-stress-round1-pre-opt
```

## 关键数据摘录

### Guest idle（200 样本）

| 场景 | 1ms P99 (ns) | 10ms P99 (ns) |
|---|---:|---:|
| bare idle | 309312 | 467952 |
| guest idle pre-opt | 178944 | 369760 |
| guest idle post-opt | 262656 | 364720 |

### Guest + stress 长稳（180k 样本，pre vs post）

| period_ms | pre-opt P99 | post-opt P99 | 改善 |
|---:|---:|---:|---:|
| 1 | 263312 | 258320 | 1.9% |
| 10 | 327904 | 309648 | 5.6% |

| period_ms | pre-opt P999 | post-opt P999 | 改善 |
|---:|---:|---:|---:|
| 1 | 482448 | 446416 | 7.5% |
| 10 | 578400 | 527568 | 8.8% |

## 结论（当前轮次）

1. **虚拟化可复现**：guest 短测与 34min stress 长稳（pre/post）均可稳定输出 `RT_LATENCY_PASS`。
2. **idle 下 vcpu_priorities 单独效果不稳定**：200 样本短测中 pre-opt 1ms P99 反而更低，**不能单独证明 ≥50% 改善**。
3. **stress 长稳是主证据**：混合分区 + CPU 压力下完成 180k 采样；post-opt 绝对值优于 pre-opt，但 **P99 改善仅 1.9%（1ms）/ 5.6%（10ms）**，未达赛题 ≥50%。
4. **pre-opt 定义有限**：当前「改造前」仅去掉 `vcpu_priorities`；完整改造前（无 `sched-cfs`、无 timer 直访等）尚未建立，可能是改善幅度偏小的原因之一。

## 仍缺项（交卷前建议补）

| 优先级 | 项 | 说明 |
|---|---|---|
| 高 | 赛题 ≥50% 改善证据 | 需更完整改造前基线或进一步优化 |
| 中 | IRQ 响应延迟 | `irq.rs` 路径未落地 |
| 中 | 正式设计/测试文档 | QEMU vs 实板差异、改造说明 PR 材料 |
| 低 | RT-Thread 混合长稳 | smoke 已有，非 ArceOS rt-latency 可比 |

## 赛题评分对照（自评）

| 评分项 | 自评 | 说明 |
|---|---|---|
| 多核 Linux 客户机 | ✅ | linux-smp2 smoke |
| 实质改造 | ✅ | 调度/定时器/GIC/中断 |
| 改造前后数据 | 🚧 | idle + stress 均有 pre/post，但改善幅度不足 |
| 空载+stress 对比 | ✅ | 两套长稳 + 对比报告 |
| 裸机基线可复现 | ✅ | bare + Zephyr + RT-Thread |
