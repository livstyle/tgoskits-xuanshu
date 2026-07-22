//! Virtio MMIO register offsets and constants (Virtio 1.0 / 1.1).

pub const MAGIC: u32 = 0x7472_6976; // "virt"
pub const VERSION: u32 = 2;
pub const DEVICE_ID_NET: u32 = 1;
pub const VENDOR_ID: u32 = 0x1af4;

pub const OFF_MAGIC: usize = 0x000;
pub const OFF_VERSION: usize = 0x004;
pub const OFF_DEVICE_ID: usize = 0x008;
pub const OFF_VENDOR_ID: usize = 0x00c;
pub const OFF_DEVICE_FEATURES: usize = 0x010;
pub const OFF_DEVICE_FEATURES_SEL: usize = 0x014;
pub const OFF_DRIVER_FEATURES: usize = 0x020;
pub const OFF_DRIVER_FEATURES_SEL: usize = 0x024;
pub const OFF_QUEUE_SEL: usize = 0x030;
pub const OFF_QUEUE_NUM_MAX: usize = 0x034;
pub const OFF_QUEUE_NUM: usize = 0x038;
pub const OFF_QUEUE_READY: usize = 0x044;
pub const OFF_QUEUE_NOTIFY: usize = 0x050;
pub const OFF_INTERRUPT_STATUS: usize = 0x060;
pub const OFF_INTERRUPT_ACK: usize = 0x064;
pub const OFF_STATUS: usize = 0x070;
pub const OFF_QUEUE_DESC_LOW: usize = 0x080;
pub const OFF_QUEUE_DESC_HIGH: usize = 0x084;
pub const OFF_QUEUE_AVAIL_LOW: usize = 0x090;
pub const OFF_QUEUE_AVAIL_HIGH: usize = 0x094;
pub const OFF_QUEUE_USED_LOW: usize = 0x0a0;
pub const OFF_QUEUE_USED_HIGH: usize = 0x0a4;
pub const OFF_CONFIG_GENERATION: usize = 0x0fc;
pub const OFF_CONFIG: usize = 0x100;

pub const VIRTIO_F_VERSION_1: u64 = 1 << 32;
pub const VIRTIO_NET_F_MAC: u64 = 1 << 5;
pub const VIRTIO_NET_F_STATUS: u64 = 1 << 16;

pub const DEVICE_FEATURES: u64 = VIRTIO_F_VERSION_1 | VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS;

pub const STATUS_DRIVER_OK: u32 = 4;
pub const STATUS_FAILED: u32 = 128;

pub const ISR_QUEUE: u32 = 1;

pub const QUEUE_RX: u32 = 0;
pub const QUEUE_TX: u32 = 1;
pub const QUEUE_COUNT: usize = 2;
pub const QUEUE_SIZE_MAX: u32 = 64;

/// Virtio-net header length for `VIRTIO_F_VERSION_1`.
///
/// With VERSION_1 the `num_buffers` field is always present, so the header is
/// 12 bytes (legacy 10-byte header only applies without VERSION_1 / MRG_RXBUF).
pub const NET_HDR_LEN: usize = 12;

pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;
