use anchor_lang::prelude::*;

/// Per-pool hook configuration.
/// PDA: ["hook_config", pool]
#[account]
pub struct HookConfig {
    pub pool: Pubkey,
    /// Per-agent borrow cap
    pub max_borrow_per_agent: u64,
    /// MVP: always true
    pub require_pnl_positive: bool,
    pub bump: u8,
}

impl HookConfig {
    pub const SEED: &'static [u8] = b"hook_config";
    pub const SIZE: usize = 8 + 32 + 8 + 1 + 1;
}
