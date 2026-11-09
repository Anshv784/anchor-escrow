use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("HMTmWS4y1aou6W4khczCKu9xcCY7youifgxNSnRTxjmP");

#[program]
pub mod anchor_escrow {
    use super::*;

    pub fn make(
        ctx: Context<Make>,
        seed: u64,
        deposit: u64,
        receive: u64,
        expires_at: i64,
        taker: Option<Pubkey>,
    ) -> Result<()> {
        ctx.accounts
            .init_escrow(seed, receive, expires_at, taker, &ctx.bumps)?;
        ctx.accounts.deposit(deposit)
    }

    pub fn take(ctx: Context<Take>) -> Result<()> {
        ctx.accounts.check_conditions()?;
        ctx.accounts.pay_maker()?;
        ctx.accounts.withdraw_and_close()
    }

    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        ctx.accounts.refund_and_close()
    }

    pub fn update(ctx: Context<Update>, receive: u64, expires_at: i64) -> Result<()> {
        ctx.accounts.update(receive, expires_at)
    }
}
