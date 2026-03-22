/// 1e27 — ray-denominated scale factor base
pub const RAY: u128 = 1_000_000_000_000_000_000_000_000_000;

/// 365 days in seconds
pub const SECONDS_PER_YEAR: u128 = 31_536_000;

/// Basis point denominator (10_000 = 100%)
pub const BIP_DENOMINATOR: u128 = 10_000;

/// Protocol fee: 10% of interest earned
pub const PROTOCOL_FEE_BIPS: u16 = 1000;

/// Hook instruction discriminators — SHA256("global:<name>")[..8]
/// These are the Anchor sighash values for the hook program's instructions.
/// Any conforming hook (Anchor, Pinocchio, raw) must use these discriminators.

/// SHA256("global:initialize")[..8]
pub const HOOK_IX_INITIALIZE: [u8; 8] = [0xaf, 0xaf, 0x6d, 0x1f, 0x0d, 0x98, 0x9b, 0xed];

/// SHA256("global:on_borrow")[..8]
pub const HOOK_IX_ON_BORROW: [u8; 8] = [0xda, 0xf8, 0xca, 0xe6, 0x16, 0x0c, 0x91, 0x5b];
