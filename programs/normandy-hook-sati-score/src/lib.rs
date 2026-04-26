use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("5JGmhFpyEMapoDy7WkN3HeaASCeRdya2rC3utXPi5DwL");

#[program]
pub mod normandy_hook_sati_score {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        max_borrow_per_agent: u64,
        allowed_providers: Vec<Pubkey>,
        min_outcome: u8,
        max_score_age_seconds: i64,
    ) -> Result<()> {
        instructions::initialize::handle_initialize(
            ctx,
            max_borrow_per_agent,
            allowed_providers,
            min_outcome,
            max_score_age_seconds,
        )
    }

    pub fn on_borrow<'info>(
        ctx: Context<'_, '_, '_, 'info, OnBorrow<'info>>,
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
