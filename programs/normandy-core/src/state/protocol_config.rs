use anchor_lang::prelude::*;

/// Global singleton — stores protocol-level configuration.
/// PDA: ["protocol_config"]
#[account]
pub struct ProtocolConfig {
    /// Can update fee recipient
    pub authority: Pubkey,
    /// Where protocol fees are claimed to
    pub fee_recipient: Pubkey,
    pub bump: u8,
}

impl ProtocolConfig {
    pub const SEED: &'static [u8] = b"protocol_config";
    pub const SIZE: usize = 8 + 32 + 32 + 1;
}
