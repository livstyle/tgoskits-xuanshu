//! Industrial Control Plane Communication (icpc) protocol core.
//!
//! Wire format (little-endian, 24-byte header + payload):
//!
//! ```text
//! ver | type | flags | rsvd | seq(u32) | timestamp_ns(u64)
//! payload_len(u16) | err_code(u16) | crc32(u32) | payload...
//! ```

#![cfg_attr(not(test), no_std)]

mod crc32;
mod header;
mod message;

pub use crc32::crc32;
pub use header::{HEADER_LEN, Header, ProtocolError};
pub use message::{Message, MessageType, decode, encode};
