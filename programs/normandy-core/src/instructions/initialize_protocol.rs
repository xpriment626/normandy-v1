use anchor_lang::prelude::*;

use crate::state::ProtocolConfig;

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(
        init,
        payer = authority,
        space = ProtocolConfig::SIZE,
        seeds = [ProtocolConfig::SEED],
        bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_protocol(ctx: Context<InitializeProtocol>, fee_recipient: Pubkey) -> Result<()> {
    let config = &mut ctx.accounts.protocol_config;
    config.authority = ctx.accounts.authority.key();
    config.fee_recipient = fee_recipient;
    config.bump = ctx.bumps.protocol_config;
    Ok(())
}
