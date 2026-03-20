use anchor_lang::prelude::*;

/// Per (pool, agent) borrow position.
/// PDA: ["borrower", pool, agent]
#[account]
pub struct BorrowerPosition {
    pub pool: Pubkey,
    pub agent: Pubkey,
    /// Original borrow amount (normalized)
    pub principal: u64,
    /// Reserved for post-MVP variable rate accrual
    pub scaled_borrow: u64,
    /// This agent's rate (set by hook within pool's range)
    pub annual_interest_bips: u16,
    /// This agent's term (set by hook within pool's range)
    pub term_seconds: i64,
    /// Interest accumulated on this position
    pub accrued_interest: u64,
    /// Scale factor at time of borrow
    pub borrow_scale_factor: u128,
    pub borrowed_at: i64,
    /// borrowed_at + term_seconds
    pub maturity_timestamp: i64,
    /// Last time interest was computed for this position
    pub last_accrual_timestamp: i64,
    /// 0 = Active, 1 = Repaid
    pub status: u8,
    pub bump: u8,
}

impl BorrowerPosition {
    pub const SEED: &'static [u8] = b"borrower";
    pub const SIZE: usize = 8 + 32 + 32 + 8 + 8 + 2 + 8 + 8 + 16 + 8 + 8 + 8 + 1 + 1;

    pub const STATUS_ACTIVE: u8 = 0;
    pub const STATUS_REPAID: u8 = 1;
}
