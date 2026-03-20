use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::RAY;
use crate::state::Pool;

#[derive(Accounts)]
#[instruction(pool_id: u64)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        space = Pool::SIZE,
        seeds = [Pool::SEED, authority.key().as_ref(), &pool_id.to_le_bytes()],
        bump,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        init,
        payer = authority,
        token::mint = underlying_mint,
        token::authority = pool,
    )]
    pub vault: Account<'info, TokenAccount>,

    pub underlying_mint: Account<'info, Mint>,

    /// CHECK: Hook program to be used for credit decisions. Validated by caller.
    pub hook_program: UncheckedAccount<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize_pool(
    ctx: Context<InitializePool>,
    pool_id: u64,
    interest_bips: u16,
    term_seconds: i64,
    reserve_ratio_bips: u16,
    position_mode: u8,
    deposit_window_end: i64,
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let clock = Clock::get()?;

    pool.authority = ctx.accounts.authority.key();
    pool.underlying_mint = ctx.accounts.underlying_mint.key();
    pool.hook_program = ctx.accounts.hook_program.key();
    pool.vault = ctx.accounts.vault.key();

    pool.scale_factor = RAY;
    pool.total_interest_earned = 0;
    pool.last_accrual_timestamp = clock.unix_timestamp;

    // MVP: min == max (fixed rate/term)
    pool.min_interest_bips = interest_bips;
    pool.max_interest_bips = interest_bips;
    pool.min_term_seconds = term_seconds;
    pool.max_term_seconds = term_seconds;

    pool.reserve_ratio_bips = reserve_ratio_bips;
    pool.total_deposits = 0;
    pool.total_borrows = 0;

    pool.accrued_protocol_fees = 0;

    pool.position_mode = position_mode;
    pool.deposit_window_end = deposit_window_end;
    pool.is_closed = false;

    pool.pool_id = pool_id;
    pool.bump = ctx.bumps.pool;

    // CPI to hook_program::initialize will be wired in the hook integration step

    Ok(())
}
