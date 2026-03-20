use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::NormandyError;

/// Per-pool lending pool account.
/// PDA: ["pool", authority, pool_id.to_le_bytes()]
#[account]
pub struct Pool {
    /// Pool creator (lender or multisig)
    pub authority: Pubkey,
    /// e.g. USDC mint
    pub underlying_mint: Pubkey,
    /// Governs credit decisions for this pool
    pub hook_program: Pubkey,
    /// SPL Token account holding pool assets
    pub vault: Pubkey,

    // Scale factor
    /// Ray-denominated (1e27), starts at RAY
    pub scale_factor: u128,
    /// Aggregate lender interest from all positions
    pub total_interest_earned: u64,
    pub last_accrual_timestamp: i64,

    // Term ranges
    pub min_interest_bips: u16,
    pub max_interest_bips: u16,
    pub min_term_seconds: i64,
    pub max_term_seconds: i64,

    // Reserve ratio
    /// % of deposits that must stay liquid
    pub reserve_ratio_bips: u16,
    /// Aggregate lender deposits (normalized)
    pub total_deposits: u64,
    /// Aggregate outstanding borrows (normalized)
    pub total_borrows: u64,

    // Protocol fees
    pub accrued_protocol_fees: u64,

    // Config
    /// 0 = PDA, 1 = NFT (future)
    pub position_mode: u8,
    /// 0 = always open
    pub deposit_window_end: i64,
    pub is_closed: bool,

    pub pool_id: u64,
    pub bump: u8,
}

impl Pool {
    pub const SEED: &'static [u8] = b"pool";
    // 8 (discriminator) + 32*4 + 16 + 8 + 8 + 2*2 + 8*2 + 2 + 8*2 + 8 + 1 + 8 + 1 + 8 + 1
    pub const SIZE: usize = 8 + 32 + 32 + 32 + 32 + 16 + 8 + 8 + 2 + 2 + 8 + 8 + 2 + 8 + 8 + 8 + 1 + 8 + 1 + 8 + 1;

    /// Accrue interest on the pool — called at the top of every state-mutating instruction.
    /// This is the sole mechanism that updates total_interest_earned, accrued_protocol_fees,
    /// and scale_factor.
    pub fn accrue_interest(&mut self, now: i64) -> Result<()> {
        let elapsed = now.checked_sub(self.last_accrual_timestamp)
            .ok_or(NormandyError::MathOverflow)? as u128;

        if elapsed == 0 || self.total_borrows == 0 || self.total_deposits == 0 {
            return Ok(());
        }

        let rate = self.min_interest_bips as u128;
        let total_borrows = self.total_borrows as u128;

        // gross_interest = total_borrows * rate * elapsed / (BIP_DENOMINATOR * SECONDS_PER_YEAR)
        let gross_interest = total_borrows
            .checked_mul(rate).ok_or(NormandyError::MathOverflow)?
            .checked_mul(elapsed).ok_or(NormandyError::MathOverflow)?
            .checked_div(BIP_DENOMINATOR.checked_mul(SECONDS_PER_YEAR).ok_or(NormandyError::MathOverflow)?)
            .ok_or(NormandyError::MathOverflow)?;

        let protocol_fee_bips = PROTOCOL_FEE_BIPS as u128;
        let protocol_cut = gross_interest
            .checked_mul(protocol_fee_bips).ok_or(NormandyError::MathOverflow)?
            .checked_div(BIP_DENOMINATOR)
            .ok_or(NormandyError::MathOverflow)?;

        let lender_interest = gross_interest.checked_sub(protocol_cut)
            .ok_or(NormandyError::MathOverflow)?;

        self.total_interest_earned = self.total_interest_earned
            .checked_add(lender_interest as u64).ok_or(NormandyError::MathOverflow)?;
        self.accrued_protocol_fees = self.accrued_protocol_fees
            .checked_add(protocol_cut as u64).ok_or(NormandyError::MathOverflow)?;

        // scale_factor = RAY + (total_interest_earned * RAY / total_deposits)
        let total_deposits = self.total_deposits as u128;
        self.scale_factor = RAY.checked_add(
            (self.total_interest_earned as u128)
                .checked_mul(RAY).ok_or(NormandyError::MathOverflow)?
                .checked_div(total_deposits).ok_or(NormandyError::MathOverflow)?
        ).ok_or(NormandyError::MathOverflow)?;

        self.last_accrual_timestamp = now;

        Ok(())
    }
}
