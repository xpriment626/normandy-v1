use anchor_lang::prelude::*;

/// Per (pool, lender) position.
/// PDA: ["lender", pool, lender]
#[account]
pub struct LenderPosition {
    pub pool: Pubkey,
    pub lender: Pubkey,
    /// Cumulative nominal deposit amount (for total_deposits bookkeeping)
    pub total_deposited: u64,
    /// Deposit in scaled units
    pub scaled_deposit: u64,
    /// Scale factor at most recent deposit (informational only)
    pub entry_scale_factor: u128,
    pub deposited_at: i64,
    pub bump: u8,
}

impl LenderPosition {
    pub const SEED: &'static [u8] = b"lender";
    pub const SIZE: usize = 8 + 32 + 32 + 8 + 8 + 16 + 8 + 1;
}
