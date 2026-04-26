use anchor_lang::prelude::*;

/// Per-pool hook configuration for the SATI ReputationScoreV3 hook.
/// PDA: ["hook_config", pool]
#[account]
pub struct HookConfig {
    pub pool: Pubkey,
    /// Per-agent borrow cap.
    pub max_borrow_per_agent: u64,
    /// Provider pubkeys the lender trusts. OR semantics: any one valid score approves.
    /// Bounded length up to MAX_ALLOWED_PROVIDERS so account size stays predictable.
    pub allowed_providers: Vec<Pubkey>,
    /// SATI Outcome enum threshold: 0=Poor, 1=Average, 2=Good.
    pub min_outcome: u8,
    /// Reject score attestation if older than this (seconds).
    pub max_score_age_seconds: i64,
    pub bump: u8,
}

impl HookConfig {
    pub const SEED: &'static [u8] = b"hook_config";

    pub const MAX_ALLOWED_PROVIDERS: usize = 8;

    /// Account size assuming worst-case allowed_providers length.
    /// 8 (discriminator)
    /// + 32 (pool)
    /// + 8 (max_borrow_per_agent)
    /// + 4 + 32 * MAX_ALLOWED_PROVIDERS (Vec<Pubkey>)
    /// + 1 (min_outcome)
    /// + 8 (max_score_age_seconds)
    /// + 1 (bump)
    pub const SIZE: usize = 8 + 32 + 8 + (4 + 32 * Self::MAX_ALLOWED_PROVIDERS) + 1 + 8 + 1;
}
