use anchor_lang::prelude::*;

#[error_code]
pub enum NormandyError {
    #[msg("Deposit window has closed")]
    DepositWindowClosed,

    #[msg("Pool is closed")]
    PoolClosed,

    #[msg("Agent already has an active borrow in this pool")]
    ActiveBorrowExists,

    #[msg("Borrow would breach reserve ratio")]
    ReserveRatioBreached,

    #[msg("Hook program rejected the borrow")]
    BorrowRejected,

    #[msg("Borrow position is not active")]
    BorrowNotActive,

    #[msg("Insufficient vault balance for withdrawal")]
    InsufficientVaultBalance,

    #[msg("Unauthorized fee claim")]
    UnauthorizedFeeClaim,

    #[msg("Math overflow")]
    MathOverflow,

    #[msg("Invalid hook return data")]
    InvalidHookReturnData,
}
