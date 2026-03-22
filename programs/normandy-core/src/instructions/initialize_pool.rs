use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::{RAY, HOOK_IX_INITIALIZE};
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

    /// CHECK: HookConfig PDA to be created by the hook program via CPI.
    /// Seeds: ["hook_config", pool.key()] — validated by the hook program, not core.
    #[account(mut)]
    pub hook_config: UncheckedAccount<'info>,

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
    max_borrow_per_agent: u64,
    require_pnl_positive: bool,
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

    // CPI to hook_program::initialize — create HookConfig for this pool
    let mut ix_data = Vec::with_capacity(8 + 8 + 1);
    ix_data.extend_from_slice(&HOOK_IX_INITIALIZE);
    max_borrow_per_agent.serialize(&mut ix_data)?;
    require_pnl_positive.serialize(&mut ix_data)?;

    let ix = Instruction {
        program_id: ctx.accounts.hook_program.key(),
        accounts: vec![
            AccountMeta::new(ctx.accounts.hook_config.key(), false),
            AccountMeta::new_readonly(pool.key(), true),
            AccountMeta::new(ctx.accounts.authority.key(), true),
            AccountMeta::new_readonly(ctx.accounts.system_program.key(), false),
        ],
        data: ix_data,
    };

    let authority_key = ctx.accounts.authority.key();
    let pool_id_bytes = pool_id.to_le_bytes();
    let bump = [pool.bump];
    let signer_seeds: &[&[u8]] = &[
        Pool::SEED,
        authority_key.as_ref(),
        &pool_id_bytes,
        &bump,
    ];

    invoke_signed(
        &ix,
        &[
            ctx.accounts.hook_config.to_account_info(),
            pool.to_account_info(),
            ctx.accounts.authority.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[signer_seeds],
    )?;

    Ok(())
}
