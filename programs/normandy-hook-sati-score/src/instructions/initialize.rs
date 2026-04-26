use anchor_lang::prelude::*;

use crate::errors::HookError;
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

    /// CHECK: The Pool PDA from normandy-core that signs via CPI during initialize_pool.
    pub pool: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handle_initialize(
    ctx: Context<Initialize>,
    max_borrow_per_agent: u64,
    allowed_providers: Vec<Pubkey>,
    min_outcome: u8,
    max_score_age_seconds: i64,
) -> Result<()> {
    require!(
        !allowed_providers.is_empty()
            && allowed_providers.len() <= HookConfig::MAX_ALLOWED_PROVIDERS,
        HookError::InvalidHookConfig
    );
    require!(min_outcome <= 2, HookError::InvalidHookConfig);
    require!(max_score_age_seconds > 0, HookError::InvalidHookConfig);

    let config = &mut ctx.accounts.hook_config;
    config.pool = ctx.accounts.pool.key();
    config.max_borrow_per_agent = max_borrow_per_agent;
    config.allowed_providers = allowed_providers;
    config.min_outcome = min_outcome;
    config.max_score_age_seconds = max_score_age_seconds;
    config.bump = ctx.bumps.hook_config;
    Ok(())
}
