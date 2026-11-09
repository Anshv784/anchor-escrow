use anchor_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    #[msg("amount must be more than 0")]
    ZeroAmount,
}
