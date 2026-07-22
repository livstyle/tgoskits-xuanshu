# Task 2 实施进度记录

> 智能化工控虚拟化擂台赛 · 任务二：基于 IP 网络的客户机间通信  
> 主文档：[os/axvisor/doc/task2-network.md](../os/axvisor/doc/task2-network.md)  
> 交卷状态：[plans/task2-reports/SUBMISSION-STATUS.md](task2-reports/SUBMISSION-STATUS.md)

---

## 进度总览

| 里程碑 | 状态 | 说明 |
|---|---|---|
| M2.0 基线调研 | ✅ | VirtioNet/VirtioBlk 工厂原均未实现 |
| M2.1 VirtioNet MMIO + 工厂 | ✅ | `axdevice/src/virtio_net/` + builtin 注册 |
| M2.2 单口 loopback 冒烟 | ✅ | `virtio-net-loopback` PASS |
| M2.3 vsw + MAC/ACL | ✅ | `SwitchPortBackend` + `IcpcPortAcl` |
| M2.4 双 Guest UDP 互通 | ✅ | `vsw-dual-guest` PASS |
| M2.5 icpc 协议库 | ✅ | `components/icpc` 单测 PASS |
| M2.6 Guest icpc 三类消息 | ✅ | `icpc-smoke` PASS |
| M2.7 可靠性 + 故障注入 | ⬜ | |
| M2.8 交卷材料 | 🚧 | |

---

## 2026-07-21 阶段五（Guest icpc 三类消息）

### 交付

| 组件 | 路径 | 行为 |
|---|---|---|
| C wire 格式 | `scripts/task2/icpc-wire.{h,c}` | 与 `components/icpc` 24B 头 + CRC32 对齐 |
| Guest B peer | `scripts/task2/icpc-peer-server.c` | CTRL→STATE / ERROR→ACK / HEARTBEAT 回显；明文 echo 兼容 |
| Guest A client | `scripts/task2/icpc-smoke-client.c` | 三类业务 + 心跳 smoke |
| 测试 | `test-suit/axvisor/normal/icpc-smoke/` | ping 暖机后跑 `/usr/local/bin/icpc-smoke` |

### 验证

```text
icpc-smoke PASS — ICPC_CTRL_OK / ICPC_ERROR_OK / ICPC_HEARTBEAT_OK
vsw-dual-guest PASS（peer 明文 echo 兼容仍可用）
vsw-peer-initramfs PASS
```

### 命令

```bash
./scripts/task2/setup-icpc-guests.sh
cargo xtask axvisor test qemu --arch aarch64 -c icpc-smoke
```

---

## 2026-07-17 阶段四（双 Guest PASS）

Passthrough 下 idle Guest 不 VM-exit → cross-VM kick 立即 peer RX DMA + `set_pending_spi_on_cpu` 按 peer CPU 路由 SPI。详见上文历史。

---

## 下一步

1. ACK/重传/心跳可靠性 + 故障注入  
2. 交卷材料收尾  
