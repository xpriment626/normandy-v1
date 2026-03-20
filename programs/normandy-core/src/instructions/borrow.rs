use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::BIP_DENOMINATOR;
use crate::errors::NormandyError;
use crate::state::{BorrowerPosition, Pool};

#[derive(Accounts)]
pub struct Borrow<'info> {
    #[account(
        mut,
        has_one = vault,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        init,
        payer = agent,
        space = BorrowerPosition::SIZE,
        seeds = [BorrowerPosition::SEED, pool.key().as_ref(), agent.key().as_ref()],
        bump,
    )]
    pub borrower_position: Account<'info, BorrowerPosition>,

    #[account(mut)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub agent_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub agent: Signer<'info>,

    /// CHECK: Hook program for credit decisions. CPI target.
    pub hook_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_borrow(ctx: Context<Borrow>, amount: u64, _reputation_proof: Vec<u8>) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let clock = Clock::get()?;

    // Accrue interest
    pool.accrue_interest(clock.unix_timestamp)?;

    // Reserve ratio check
    let total_deposits = pool.total_deposits as u128;
    let reserve_ratio = pool.reserve_ratio_bips as u128;
    let required_reserves = total_deposits
        .checked_mul(reserve_ratio).ok_or(NormandyError::MathOverflow)?
        .checked_div(BIP_DENOMINATOR).ok_or(NormandyError::MathOverflow)? as u64;

    let vault_balance = ctx.accounts.vault.amount;
    let available = vault_balance
        .checked_sub(required_reserves).ok_or(NormandyError::ReserveRatioBreached)?
        .checked_sub(pool.accrued_protocol_fees).ok_or(NormandyError::ReserveRatioBreached)?;

    require!(amount <= available, NormandyError::ReserveRatioBreached);

    // CPI to hook_program::on_borrow will be wired in the hook integration step
    // For now, MVP uses pool's fixed rate/term directly

    let interest_bips = pool.min_interest_bips;
    let term_seconds = pool.min_term_seconds;

    // Create borrower position
    let position = &mut ctx.accounts.borrower_position;
    position.pool = pool.key();
    position.agent = ctx.accounts.agent.key();
    position.principal = amount;
    position.scaled_borrow = 0; // Reserved for post-MVP
    position.annual_interest_bips = interest_bips;
    position.term_seconds = term_seconds;
    position.accrued_interest = 0;
    position.borrow_scale_factor = pool.scale_factor;
    position.borrowed_at = clock.unix_timestamp;
    position.maturity_timestamp = clock.unix_timestamp
        .checked_add(term_seconds).ok_or(NormandyError::MathOverflow)?;
    position.last_accrual_timestamp = clock.unix_timestamp;
    position.status = BorrowerPosition::STATUS_ACTIVE;
    position.bump = ctx.bumps.borrower_position;

    // Update pool
    pool.total_borrows = pool.total_borrows
        .checked_add(amount).ok_or(NormandyError::MathOverflow)?;

    // Transfer from vault to agent (Pool PDA signs)
    let authority_key = pool.authority.key();
    let pool_id_bytes = pool.pool_id.to_le_bytes();
    let bump = [pool.bump];
    let signer_seeds: &[&[&[u8]]] = &[&[
        Pool::SEED,
        authority_key.as_ref(),
        &pool_id_bytes,
        &bump,
    ]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.vault.to_account_info(),
        to: ctx.accounts.agent_token_account.to_account_info(),
        authority: pool.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    token::transfer(cpi_ctx, amount)?;

    Ok(())
}
