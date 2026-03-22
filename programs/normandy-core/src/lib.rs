use anchor_lang::prelude::*;

pub mod constants;
pub mod cpi_interface;
pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("3kXtyEqYxGTTnUtCpVNVwNwQjRZPYfGkEZo75tQtdwLs");

#[program]
pub mod normandy_core {
    use super::*;

    pub fn initialize_protocol(ctx: Context<InitializeProtocol>, fee_recipient: Pubkey) -> Result<()> {
        instructions::initialize_protocol::handle_initialize_protocol(ctx, fee_recipient)
    }

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        pool_id: u64,
        interest_bips: u16,
        term_seconds: i64,
        reserve_ratio_bips: u16,
        position_mode: u8,
        deposit_window_end: i64,
    ) -> Result<()> {
        instructions::initialize_pool::handle_initialize_pool(
            ctx, pool_id, interest_bips, term_seconds,
            reserve_ratio_bips, position_mode, deposit_window_end,
        )
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit::handle_deposit(ctx, amount)
    }

    pub fn borrow(ctx: Context<Borrow>, amount: u64, reputation_proof: Vec<u8>) -> Result<()> {
        instructions::borrow::handle_borrow(ctx, amount, reputation_proof)
    }

    pub fn repay(ctx: Context<Repay>) -> Result<()> {
        instructions::repay::handle_repay(ctx)
    }

    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        instructions::withdraw::handle_withdraw(ctx)
    }

    pub fn claim_protocol_fees(ctx: Context<ClaimProtocolFees>) -> Result<()> {
        instructions::claim_protocol_fees::handle_claim_protocol_fees(ctx)
    }
}
