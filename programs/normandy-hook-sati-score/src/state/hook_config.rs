use anchor_lang::prelude::*;

/// Per-pool hook configuration for the SAS-attestation-based credit hook.
///
/// Default usage is SATI's `ReputationScoreV3` schema (see SDK helpers for the
/// canonical credential + schema pubkeys), but the lender chooses both at pool
/// init — letting them pin to a specific schema version, override the credential
/// authority, or wire to a different SAS-published score schema entirely.
///
/// PDA: ["hook_config", pool]
#[account]
pub struct HookConfig {
    pub pool: Pubkey,
    /// Per-agent borrow cap.
    pub max_borrow_per_agent: u64,
    /// SAS Credential PDA the lender trusts as the attestation issuer.
    /// Default for SATI deployments: `DQHW6fAhPfGAENuwJVYfzEvUN12DakZgaaGtPPRfGei1`.
    pub credential: Pubkey,
    /// SAS Schema PDA the attestation must conform to.
    /// Default for SATI ReputationScoreV3: `7MoXgvrFhMxmB84AfAtp8LGfC4sEXUHD6JCQJpfj2jTj`.
    pub schema: Pubkey,
    /// Provider pubkeys the lender trusts. OR semantics: any one valid score approves.
    /// Bounded length up to MAX_ALLOWED_PROVIDERS so account size stays predictable.
    pub allowed_providers: Vec<Pubkey>,
    /// SATI Outcome enum threshold: 0=Negative, 1=Neutral, 2=Positive.
    pub min_outcome: u8,
    /// Freshness preference. SAS doesn't store a creation timestamp, so V1
    /// enforces this as: "if > 0, the attestation MUST have a non-zero expiry."
    /// True age-based freshness is deferred until SATI surfaces creation time
    /// or this hook starts parsing content JSON.
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
    /// + 32 (credential)
    /// + 32 (schema)
    /// + 4 + 32 * MAX_ALLOWED_PROVIDERS (Vec<Pubkey>)
    /// + 1 (min_outcome)
    /// + 8 (max_score_age_seconds)
    /// + 1 (bump)
    pub const SIZE: usize =
        8 + 32 + 8 + 32 + 32 + (4 + 32 * Self::MAX_ALLOWED_PROVIDERS) + 1 + 8 + 1;
}
