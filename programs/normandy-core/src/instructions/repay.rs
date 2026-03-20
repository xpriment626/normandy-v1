use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::*;
use crate::errors::NormandyError;
use crate::state::{BorrowerPosition, Pool};

#[derive(Accounts)]
pub struct Repay<'info> {
    #[account(
        mut,
        has_one = vault,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [BorrowerPosition::SEED, pool.key().as_ref(), agent.key().as_ref()],
        bump = borrower_position.bump,
    )]
    pub borrower_position: Account<'info, BorrowerPosition>,

    /// CHECK: The agent whose position is being repaid. Not necessarily the signer.
    pub agent: UncheckedAccount<'info>,

    #[account(mut)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub repayer_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub repayer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_repay(ctx: Context<Repay>) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let position = &mut ctx.accounts.borrower_position;
    let clock = Clock::get()?;

    require!(position.status == BorrowerPosition::STATUS_ACTIVE, NormandyError::BorrowNotActive);

    // Accrue interest on pool (sole driver of scale_factor and total_interest_earned)
    pool.accrue_interest(clock.unix_timestamp)?;

    // Compute position interest for transfer amount
    let elapsed = clock.unix_timestamp
        .checked_sub(position.last_accrual_timestamp)
        .ok_or(NormandyError::MathOverflow)? as u128;

    let position_interest = (position.principal as u128)
        .checked_mul(position.annual_interest_bips as u128).ok_or(NormandyError::MathOverflow)?
        .checked_mul(elapsed).ok_or(NormandyError::MathOverflow)?
        .checked_div(
            BIP_DENOMINATOR.checked_mul(SECONDS_PER_YEAR).ok_or(NormandyError::MathOverflow)?
        ).ok_or(NormandyError::MathOverflow)? as u64;

    position.accrued_interest = position.accrued_interest
        .checked_add(position_interest).ok_or(NormandyError::MathOverflow)?;
    position.last_accrual_timestamp = clock.unix_timestamp;

    let total_owed = position.principal
        .checked_add(position.accrued_interest).ok_or(NormandyError::MathOverflow)?;

    // Transfer total_owed from repayer to vault
    let cpi_accounts = Transfer {
        from: ctx.accounts.repayer_token_account.to_account_info(),
        to: ctx.accounts.vault.to_account_info(),
        authority: ctx.accounts.repayer.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token::transfer(cpi_ctx, total_owed)?;

    // Update position
    position.status = BorrowerPosition::STATUS_REPAID;

    // Update pool
    pool.total_borrows = pool.total_borrows
        .checked_sub(position.principal).ok_or(NormandyError::MathOverflow)?;

    Ok(())
}
