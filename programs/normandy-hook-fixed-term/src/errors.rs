use anchor_lang::prelude::*;

#[error_code]
pub enum HookError {
    #[msg("Borrow amount exceeds per-agent cap")]
    BorrowExceedsCap,

    #[msg("Invalid reputation proof data")]
    InvalidReputationProof,

    #[msg("Agent PnL is not positive — borrow rejected")]
    InsufficientReputation,

    #[msg("Failed to serialize return data")]
    SerializationError,
}
