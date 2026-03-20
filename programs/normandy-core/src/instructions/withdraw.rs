use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::RAY;
use crate::errors::NormandyError;
use crate::state::{LenderPosition, Pool};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        mut,
        has_one = vault,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [LenderPosition::SEED, pool.key().as_ref(), lender.key().as_ref()],
        bump = lender_position.bump,
        close = lender,
    )]
    pub lender_position: Account<'info, LenderPosition>,

    #[account(mut)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub lender_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub lender: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_withdraw(ctx: Context<Withdraw>) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let position = &ctx.accounts.lender_position;
    let clock = Clock::get()?;

    // Accrue interest
    pool.accrue_interest(clock.unix_timestamp)?;

    // Compute payout
    let payout = (position.scaled_deposit as u128)
        .checked_mul(pool.scale_factor).ok_or(NormandyError::MathOverflow)?
        .checked_div(RAY).ok_or(NormandyError::MathOverflow)? as u64;

    // Check vault has enough (minus protocol fees)
    let available = ctx.accounts.vault.amount
        .checked_sub(pool.accrued_protocol_fees).ok_or(NormandyError::InsufficientVaultBalance)?;
    require!(payout <= available, NormandyError::InsufficientVaultBalance);

    // Transfer from vault to lender (Pool PDA signs)
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
        to: ctx.accounts.lender_token_account.to_account_info(),
        authority: pool.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    token::transfer(cpi_ctx, payout)?;

    // Update pool — subtract original deposit amount
    pool.total_deposits = pool.total_deposits
        .checked_sub(position.total_deposited).ok_or(NormandyError::MathOverflow)?;

    // Position account is closed by the `close = lender` constraint

    Ok(())
}
