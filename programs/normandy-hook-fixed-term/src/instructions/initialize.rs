use anchor_lang::prelude::*;

use crate::state::HookConfig;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = HookConfig::SIZE,
        seeds = [HookConfig::SEED, pool.key().as_ref()],
        bump,
    )]
    pub hook_config: Account<'info, HookConfig>,

    /// CHECK: The Pool PDA from normandy-core that signs via CPI.
    pub pool: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handle_initialize(
    ctx: Context<Initialize>,
    max_borrow_per_agent: u64,
    require_pnl_positive: bool,
) -> Result<()> {
    let config = &mut ctx.accounts.hook_config;
    config.pool = ctx.accounts.pool.key();
    config.max_borrow_per_agent = max_borrow_per_agent;
    config.require_pnl_positive = require_pnl_positive;
    config.bump = ctx.bumps.hook_config;
    Ok(())
}
