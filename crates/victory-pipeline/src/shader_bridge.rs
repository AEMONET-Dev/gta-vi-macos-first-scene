//! Version 2 of the local shader translation bridge. A session owns a new
//! zero-filled file. Each slot has exactly one producer and one consumer at
//! a time. Publishing a state with Release transfers its payload to the peer,
//! which observes that state with Acquire. A timed-out client abandons its
//! slot until the session ends; a late reply can never satisfy a new request.
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64};

pub const BRIDGE_MAGIC: u64 = 0x5649_4354_4D54_4C32;
pub const BRIDGE_VERSION: u32 = 2;
pub const SLOTS: usize = 8;
pub const SLOT_SIZE: usize = 4 << 20;
pub const HEADER_SIZE: usize = 4096;
pub const MAPPING_SIZE: usize = HEADER_SIZE + SLOTS * SLOT_SIZE;
pub const CAPACITY: usize = SLOT_SIZE - size_of::<Slot>();

#[repr(C, align(64))]
pub struct BridgeHeader {
    pub magic: AtomicU64,
    pub version: AtomicU32,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum BridgeOpcode {
    Nop = 0,
    TranslateShader = 1,
    CompileDxilToMetalLib = 2,
    Ping = 3,
    /// Build fingerprint followed by Xbox container; response is VIctory's
    /// complete cache entry (DXIL and root binding metadata, or a refusal).
    TranslateHostShader = 4,
}

pub const EMPTY: u32 = 0;
pub const WRITING: u32 = 1;
pub const REQUEST: u32 = 2;
pub const PROCESSING: u32 = 3;
pub const RESPONSE: u32 = 4;

#[repr(C, align(64))]
pub struct Slot {
    pub state: AtomicU32,
    pub opcode: AtomicU32,
    pub option: AtomicU32,
    pub length: AtomicU32,
    pub second_length: AtomicU32,
    pub status: AtomicI32,
    pub cache_hit: AtomicU32,
}

/// # Safety
/// The caller holds a live, page-aligned mapping of MAPPING_SIZE bytes and
/// must use the state handshake before reading or modifying payload bytes.
pub unsafe fn slot(base: *mut u8, index: usize) -> (*const Slot, *mut u8) {
    assert!(index < SLOTS);
    // SAFETY: each slot lies completely within the mapping.
    let start = unsafe { base.add(HEADER_SIZE + index * SLOT_SIZE) };
    (start.cast(), unsafe { start.add(size_of::<Slot>()) })
}
