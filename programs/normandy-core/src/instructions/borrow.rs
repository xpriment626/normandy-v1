use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::{invoke_signed, get_return_data};
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::{BIP_DENOMINATOR, HOOK_IX_ON_BORROW};
use crate::cpi_interface::OnBorrowResult;
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

    /// CHECK: HookConfig PDA owned by the hook program. Passed through to hook CPI.
    pub hook_config: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_borrow(ctx: Context<Borrow>, amount: u64, reputation_proof: Vec<u8>) -> Result<()> {
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

    // Validate hook program matches pool configuration
    require!(
        ctx.accounts.hook_program.key() == pool.hook_program,
        NormandyError::InvalidHookProgram
    );

    // CPI to hook_program::on_borrow — credit decision
    let agent_key = ctx.accounts.agent.key();
    let mut ix_data = Vec::with_capacity(8 + 32 + 8 + 4 + reputation_proof.len() + 2 + 2 + 8 + 8);
    ix_data.extend_from_slice(&HOOK_IX_ON_BORROW);
    agent_key.serialize(&mut ix_data)?;
    amount.serialize(&mut ix_data)?;
    reputation_proof.serialize(&mut ix_data)?;
    pool.min_interest_bips.serialize(&mut ix_data)?;
    pool.max_interest_bips.serialize(&mut ix_data)?;
    pool.min_term_seconds.serialize(&mut ix_data)?;
    pool.max_term_seconds.serialize(&mut ix_data)?;

    let ix = Instruction {
        program_id: ctx.accounts.hook_program.key(),
        accounts: vec![
            AccountMeta::new_readonly(ctx.accounts.hook_config.key(), false),
            AccountMeta::new_readonly(pool.key(), true),
        ],
        data: ix_data,
    };

    // Use hook_ prefix to avoid shadowing the token transfer signer seeds below
    let hook_authority_key = pool.authority.key();
    let hook_pool_id_bytes = pool.pool_id.to_le_bytes();
    let hook_bump = [pool.bump];
    let hook_signer_seeds: &[&[u8]] = &[
        Pool::SEED,
        hook_authority_key.as_ref(),
        &hook_pool_id_bytes,
        &hook_bump,
    ];

    invoke_signed(
        &ix,
        &[
            ctx.accounts.hook_config.to_account_info(),
            pool.to_account_info(),
        ],
        &[hook_signer_seeds],
    )?;

    // Parse return data from hook
    let (returning_program, data) = get_return_data()
        .ok_or(NormandyError::InvalidHookReturnData)?;
    require!(
        returning_program == ctx.accounts.hook_program.key(),
        NormandyError::InvalidHookReturnData
    );

    let result = OnBorrowResult::try_from_slice(&data)
        .map_err(|_| NormandyError::InvalidHookReturnData)?;
    require!(result.approved, NormandyError::BorrowRejected);

    let interest_bips = result.interest_bips;
    let term_seconds = result.term_seconds;

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
