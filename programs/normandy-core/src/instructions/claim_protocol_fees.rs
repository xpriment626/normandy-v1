use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::errors::NormandyError;
use crate::state::{Pool, ProtocolConfig};

#[derive(Accounts)]
pub struct ClaimProtocolFees<'info> {
    #[account(
        seeds = [ProtocolConfig::SEED],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        has_one = vault,
    )]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub fee_recipient_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = fee_recipient.key() == protocol_config.fee_recipient @ NormandyError::UnauthorizedFeeClaim,
    )]
    pub fee_recipient: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_claim_protocol_fees(ctx: Context<ClaimProtocolFees>) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let clock = Clock::get()?;

    // Accrue interest
    pool.accrue_interest(clock.unix_timestamp)?;

    let fees = pool.accrued_protocol_fees;
    if fees == 0 {
        return Ok(());
    }

    // Transfer fees from vault to fee recipient (Pool PDA signs)
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
        to: ctx.accounts.fee_recipient_token_account.to_account_info(),
        authority: pool.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    token::transfer(cpi_ctx, fees)?;

    pool.accrued_protocol_fees = 0;

    Ok(())
}
