use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, MintTo, TokenAccount, TokenInterface, mint_to},
};

use crate::state::{StakeConfig, StakeAccount, UserAccount};

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

        #[account(
        mut,
        constraint = stake_account.owner == user.key() @ StakeError::InvalidOwner,
        seeds = [b"stake", stake_account.mint.as_ref(), config.key().as_ref()],
        bump = stake_account.bump,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, StakeConfig>,

    #[account(
        mut,
        seeds = [b"rewards", config.key().as_ref()],
        bump = config.rewards_bump,
    )]
    pub rewards_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = rewards_mint,
        associated_token::authority = user,
    )]
    pub rewards_ata: InterfaceAccount<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Claim<'info> {
    pub fn claim_rewards(&mut self) -> Result<()> {
        let clock = Clock::get()?;
        let elapsed = clock.unix_timestamp - self.stake_account.last_update;

        // Calculate reward tokens: elapsed_seconds * points_per_stake
        // Multiply by 10^decimals (6) to get proper token amount
        let rewards = (elapsed as u64)
            .checked_mul(self.config.points_per_stake as u64)
            .ok_or(error!(StakeError::Overflow))?
            .checked_mul(10u64.pow(self.rewards_mint.decimals as u32))
            .ok_or(error!(StakeError::Overflow))?;

        // Update the last_update timestamp so rewards aren't double-counted
        self.stake_account.last_update = clock.unix_timestamp;

        // Update user points for tracking
        self.user_account.points = self.user_account.points
            .checked_add(elapsed as u32)
            .ok_or(error!(StakeError::Overflow))?;

        if rewards > 0 {
            // Mint reward tokens to the user's ATA
            // The config PDA is the mint authority
            let seeds = &[b"config".as_ref(), &[self.config.bump]];
            let signer_seeds = &[&seeds[..]];

            mint_to(
                CpiContext::new_with_signer(
                    self.token_program.key(),
                    MintTo {
                        mint: self.rewards_mint.to_account_info(),
                        to: self.rewards_ata.to_account_info(),
                        authority: self.config.to_account_info(),
                    },
                    signer_seeds,
                ),
                rewards,
            )?;
        }

        Ok(())
    }
}

use crate::errors::StakeError;
