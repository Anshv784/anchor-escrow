use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
        TransferChecked,
    },
};

use crate::state::Escrow;

#[derive(Accounts)]
pub struct Take<'info> {
    #[account(mut)]
    pub taker: Signer<'info>,

    #[account(mut)]
    pub maker: SystemAccount<'info>,

    pub mint_a: Box<InterfaceAccount<'info, Mint>>,
    pub mint_b: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_a,
        associated_token::authority = taker,
        associated_token::token_program = token_program
    )]
    pub taker_ata_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = taker,
        associated_token::token_program = token_program
    )]
    pub taker_ata_b: Box<InterfaceAccount<'info, TokenAccount>>,

    // maker might not have a mint_b account yet, taker pays for it in that case
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_b,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_b: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        close = maker,
        has_one = maker,
        has_one = mint_a,
        has_one = mint_b,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Box<Account<'info, Escrow>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

impl<'info> Take<'info> {
    pub fn pay_maker(&mut self) -> Result<()> {
        let cpi_ctx = CpiContext::new(
            self.token_program.key(),
            TransferChecked {
                from: self.taker_ata_b.to_account_info(),
                mint: self.mint_b.to_account_info(),
                to: self.maker_ata_b.to_account_info(),
                authority: self.taker.to_account_info(),
            },
        );
        transfer_checked(cpi_ctx, self.escrow.receive, self.mint_b.decimals)
    }

    pub fn withdraw_and_close(&mut self) -> Result<()> {
        let maker_key = self.maker.key();
        let seed_bytes = self.escrow.seed.to_le_bytes();
        let seeds = &[
            b"escrow",
            maker_key.as_ref(),
            seed_bytes.as_ref(),
            &[self.escrow.bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.key(),
            TransferChecked {
                from: self.vault.to_account_info(),
                mint: self.mint_a.to_account_info(),
                to: self.taker_ata_a.to_account_info(),
                authority: self.escrow.to_account_info(),
            },
            signer_seeds,
        );
        transfer_checked(cpi_ctx, self.vault.amount, self.mint_a.decimals)?;

        // vault rent goes back to the maker since they paid for it
        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.key(),
            CloseAccount {
                account: self.vault.to_account_info(),
                destination: self.maker.to_account_info(),
                authority: self.escrow.to_account_info(),
            },
            signer_seeds,
        );
        close_account(cpi_ctx)
    }
}
