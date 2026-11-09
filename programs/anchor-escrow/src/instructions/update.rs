use anchor_lang::prelude::*;

use crate::error::EscrowError;
use crate::state::Escrow;

// lets the maker change the price or the deadline without refunding and making again
#[derive(Accounts)]
pub struct Update<'info> {
    pub maker: Signer<'info>,

    #[account(
        mut,
        has_one = maker,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Account<'info, Escrow>,
}

impl<'info> Update<'info> {
    pub fn update(&mut self, receive: u64, expires_at: i64) -> Result<()> {
        require!(receive > 0, EscrowError::ZeroAmount);
        let now = Clock::get()?.unix_timestamp;
        require!(expires_at > now, EscrowError::InvalidExpiry);

        self.escrow.receive = receive;
        self.escrow.expires_at = expires_at;
        Ok(())
    }
}
