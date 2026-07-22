# Task 2 交卷状态摘要

> 最后更新：2026-07-21  
> 主文档：[`os/axvisor/doc/task2-network.md`](../../os/axvisor/doc/task2-network.md)  
> 实施记录：[`../task2-实施记录.md`](../task2-实施记录.md)

## 已完成（可复现）

| 项 | 证据 |
|---|---|
| VirtioNet + vsw + ACL | `virtio-net-loopback` / `vsw-dual-guest` **PASS** |
| icpc 协议库 | `cargo test -p icpc` PASS |
| Guest 三类 icpc 消息 | `icpc-smoke` **PASS**（CTRL/STATE/ERROR/ACK/HEARTBEAT） |
| Peer initramfs | `vsw-peer-initramfs` **PASS** |
| cross-VM RX + SPI affinity | `set_pending_spi_on_cpu` + kick 路径 DMA |

```bash
./scripts/task2/setup-icpc-guests.sh
cargo test -p icpc
cargo xtask axvisor test qemu --arch aarch64 -c icpc-smoke
cargo xtask axvisor test qemu --arch aarch64 -c vsw-dual-guest
cargo xtask axvisor test qemu --arch aarch64 -c virtio-net-loopback
cargo xtask axvisor test qemu --arch aarch64 -c vsw-peer-initramfs
```

## 仍缺项

| 优先级 | 项 |
|---|---|
| 中 | ACK/重传/心跳可靠性 + 故障注入 |
| 中 | `icpc-bench` / 交卷拓扑与抓包 |
| 低 | ACL 端到端拒绝用例 |

## 评分对照（自评）

| 评分项 | 自评 |
|---|---|
| IP 链路建立且配置清楚 | ✅ |
| 应用层协议字段完整 | ✅ icpc 库 + C wire 对齐 |
| 三类业务消息可用 | ✅ icpc-smoke |
| 可靠性/超时/重连 | ⬜ |
| 自动化测试数据充分 | ✅ 4 项 axvisor 测试 PASS |
| 网络隔离与访问控制 | 🚧 ACL 已挂；缺拒绝用例 |
