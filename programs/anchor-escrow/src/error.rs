use anchor_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    #[msg("amount must be more than 0")]
    ZeroAmount,
    #[msg("expiry has to be in the future")]
    InvalidExpiry,
    #[msg("this escrow has expired")]
    Expired,
    #[msg("only the maker can refund before expiry")]
    NotExpired,
    #[msg("this escrow is reserved for a different taker")]
    WrongTaker,
}
