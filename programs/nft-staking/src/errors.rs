use anchor_lang::prelude::*;

#[error_code]
pub enum StakeError {
    #[msg("Maximum stake limit reached")]
    MaxStakeReached,
    #[msg("Freeze period not elapsed")]
    FreezePeriodNotElapsed,
    #[msg("You are not the owner of this stake account")]
    InvalidOwner,
    #[msg("Arithmetic overflow")]
    Overflow,
}
