use anchor_lang::prelude::*;

use crate::errors::HookError;
use crate::state::HookConfig;

/// Reputation proof deserialized from opaque bytes passed through from normandy-core.
#[derive(AnchorDeserialize)]
pub struct ReputationProof {
    pub pnl: i64,
    pub timestamp: i64,
}

/// Return data from on_borrow — read by normandy-core via sol_get_return_data.
#[derive(AnchorSerialize)]
pub struct OnBorrowResult {
    pub approved: bool,
    pub interest_bips: u16,
    pub term_seconds: i64,
}

#[derive(Accounts)]
pub struct OnBorrow<'info> {
    #[account(
        seeds = [HookConfig::SEED, pool.key().as_ref()],
        bump = hook_config.bump,
    )]
    pub hook_config: Account<'info, HookConfig>,

    /// CHECK: The Pool PDA from normandy-core that signs via CPI.
    pub pool: Signer<'info>,
}

pub fn handle_on_borrow(
    ctx: Context<OnBorrow>,
    _agent: Pubkey,
    amount: u64,
    reputation_proof: Vec<u8>,
    min_interest_bips: u16,
    max_interest_bips: u16,
    min_term_seconds: i64,
    max_term_seconds: i64,
) -> Result<()> {
    let config = &ctx.accounts.hook_config;

    // Check per-agent borrow cap
    require!(amount <= config.max_borrow_per_agent, HookError::BorrowExceedsCap);

    // Deserialize and check reputation proof
    if config.require_pnl_positive {
        let proof = ReputationProof::try_from_slice(&reputation_proof)
            .map_err(|_| HookError::InvalidReputationProof)?;
        require!(proof.pnl > 0, HookError::InsufficientReputation);
    }

    // MVP: return fixed rate/term (min == max)
    let result = OnBorrowResult {
        approved: true,
        interest_bips: min_interest_bips,
        term_seconds: min_term_seconds,
    };

    // Set return data for normandy-core to read
    let data = result.try_to_vec().map_err(|_| HookError::SerializationError)?;
    anchor_lang::solana_program::program::set_return_data(&data);

    // Suppress unused variable warnings — these are used post-MVP for variable rates
    let _ = max_interest_bips;
    let _ = max_term_seconds;

    Ok(())
}
