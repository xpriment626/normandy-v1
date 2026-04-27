use anchor_lang::prelude::*;

#[derive(PartialEq, Eq)]
#[error_code]
pub enum HookError {
    #[msg("Borrow amount exceeds per-agent cap")]
    BorrowExceedsCap,

    #[msg("No remaining account passed all reputation checks")]
    NoValidReputationScore,

    #[msg("Score account owner is not the SAS program")]
    InvalidScoreAccount,

    #[msg("Reputation score exceeds max age or past SAS expiry")]
    ReputationScoreStale,

    #[msg("Reputation score outcome is below the configured threshold")]
    ReputationScoreBelowThreshold,

    #[msg("Reputation score account layout is invalid or unsupported version")]
    InvalidScoreLayout,

    #[msg("Score account does not match any allowed provider")]
    UntrustedProvider,

    #[msg("HookConfig args are invalid (empty/oversized providers, bad outcome, non-positive age)")]
    InvalidHookConfig,

    #[msg("Failed to serialize return data")]
    SerializationError,
}
