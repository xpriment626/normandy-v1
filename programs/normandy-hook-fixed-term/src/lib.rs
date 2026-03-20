use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("He2SZJXMwPnyjN3dfuV8VEU2TPU58oR1HSWFkYvUgnNC");

#[program]
pub mod normandy_hook_fixed_term {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        max_borrow_per_agent: u64,
        require_pnl_positive: bool,
    ) -> Result<()> {
        instructions::initialize::handle_initialize(ctx, max_borrow_per_agent, require_pnl_positive)
    }

    pub fn on_borrow(
        ctx: Context<OnBorrow>,
        agent: Pubkey,
        amount: u64,
        reputation_proof: Vec<u8>,
        min_interest_bips: u16,
        max_interest_bips: u16,
        min_term_seconds: i64,
        max_term_seconds: i64,
    ) -> Result<()> {
        instructions::on_borrow::handle_on_borrow(
            ctx, agent, amount, reputation_proof,
            min_interest_bips, max_interest_bips,
            min_term_seconds, max_term_seconds,
        )
    }
}
