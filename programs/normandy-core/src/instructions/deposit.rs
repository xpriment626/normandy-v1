use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::RAY;
use crate::errors::NormandyError;
use crate::state::{LenderPosition, Pool};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(
        mut,
        has_one = vault,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        init_if_needed,
        payer = lender,
        space = LenderPosition::SIZE,
        seeds = [LenderPosition::SEED, pool.key().as_ref(), lender.key().as_ref()],
        bump,
    )]
    pub lender_position: Account<'info, LenderPosition>,

    #[account(mut)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub lender_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub lender: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let clock = Clock::get()?;

    // Check deposit window
    if pool.deposit_window_end != 0 && clock.unix_timestamp > pool.deposit_window_end {
        return Err(NormandyError::DepositWindowClosed.into());
    }

    // Check pool not closed
    require!(!pool.is_closed, NormandyError::PoolClosed);

    // Accrue interest
    pool.accrue_interest(clock.unix_timestamp)?;

    // Compute scaled amount
    let scaled_amount = (amount as u128)
        .checked_mul(RAY).ok_or(NormandyError::MathOverflow)?
        .checked_div(pool.scale_factor).ok_or(NormandyError::MathOverflow)? as u64;

    // Transfer tokens from lender to vault
    let cpi_accounts = Transfer {
        from: ctx.accounts.lender_token_account.to_account_info(),
        to: ctx.accounts.vault.to_account_info(),
        authority: ctx.accounts.lender.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token::transfer(cpi_ctx, amount)?;

    // Update lender position
    let position = &mut ctx.accounts.lender_position;
    if position.pool == Pubkey::default() {
        // First deposit — initialize position fields
        position.pool = pool.key();
        position.lender = ctx.accounts.lender.key();
        position.bump = ctx.bumps.lender_position;
    }
    position.scaled_deposit = position.scaled_deposit
        .checked_add(scaled_amount).ok_or(NormandyError::MathOverflow)?;
    position.total_deposited = position.total_deposited
        .checked_add(amount).ok_or(NormandyError::MathOverflow)?;
    position.entry_scale_factor = pool.scale_factor;
    position.deposited_at = clock.unix_timestamp;

    // Update pool
    pool.total_deposits = pool.total_deposits
        .checked_add(amount).ok_or(NormandyError::MathOverflow)?;

    Ok(())
}
