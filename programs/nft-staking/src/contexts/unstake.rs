use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::{invoke, invoke_signed};
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, MintTo, TokenAccount, TokenInterface, mint_to},
};

use crate::constants::MPL_CORE_PROGRAM_ID;
use crate::state::{StakeConfig, StakeAccount, UserAccount};
use crate::errors::StakeError;

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Validated via CPI to mpl-core
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Validated via CPI to mpl-core
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,

    #[account(
        mut,
        close = user,
        seeds = [b"stake", asset.key().as_ref(), config.key().as_ref()],
        bump = stake_account.bump,
        constraint = stake_account.owner == user.key(),
        constraint = stake_account.mint == asset.key(),
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

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

    /// CHECK: This is the mpl-core program
    #[account(address = MPL_CORE_PROGRAM_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Unstake<'info> {
    pub fn unstake(&mut self) -> Result<()> {
        let clock = Clock::get()?;

        // Enforce the freeze period
        let time_staked = clock.unix_timestamp - self.stake_account.staked_at;
        require!(
            time_staked >= self.config.freeze_period as i64,
            StakeError::FreezePeriodNotElapsed
        );

        // Claim any remaining rewards before unstaking
        self.claim_remaining_rewards(&clock)?;



        // Thaw: UpdatePluginV1 to set frozen = false
        // The owner (user) is the plugin authority since init_authority was None
        {
            let mut data = vec![6]; // UpdatePluginV1 discriminator
            data.extend_from_slice(&[1]); // Plugin type: FreezeDelegate
            data.push(0); // frozen = false

            let accounts = vec![
                AccountMeta::new(self.asset.key(), false),
                AccountMeta::new(self.collection.key(), false),
                AccountMeta::new(self.user.key(), true),
                AccountMeta::new_readonly(self.user.key(), true),
                AccountMeta::new_readonly(self.system_program.key(), false),
                AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false),
            ];

            let ix = Instruction {
                program_id: MPL_CORE_PROGRAM_ID,
                accounts,
                data,
            };

            invoke(
                &ix,
                &[
                    self.asset.to_account_info(),
                    self.collection.to_account_info(),
                    self.user.to_account_info(),
                    self.system_program.to_account_info(),
                    self.mpl_core_program.to_account_info(),
                ],
            )?;
        }

        // Remove FreezeDelegate plugin: RemovePluginV1
        {
            let mut data = vec![4]; // RemovePluginV1 discriminator
            data.extend_from_slice(&[1]); // Plugin type: FreezeDelegate

            let accounts = vec![
                AccountMeta::new(self.asset.key(), false),
                AccountMeta::new(self.collection.key(), false),
                AccountMeta::new(self.user.key(), true),
                AccountMeta::new_readonly(self.user.key(), true),
                AccountMeta::new_readonly(self.system_program.key(), false),
                AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false),
            ];

            let ix = Instruction {
                program_id: MPL_CORE_PROGRAM_ID,
                accounts,
                data,
            };

            invoke(
                &ix,
                &[
                    self.asset.to_account_info(),
                    self.collection.to_account_info(),
                    self.user.to_account_info(),
                    self.system_program.to_account_info(),
                    self.mpl_core_program.to_account_info(),
                ],
            )?;
        }

        self.user_account.amount_staked -= 1;

        Ok(())
    }

    fn claim_remaining_rewards(&mut self, clock: &Clock) -> Result<()> {
        let elapsed = clock.unix_timestamp - self.stake_account.last_update;

        let rewards = (elapsed as u64)
            .checked_mul(self.config.points_per_stake as u64)
            .ok_or(error!(StakeError::Overflow))?
            .checked_mul(10u64.pow(self.rewards_mint.decimals as u32))
            .ok_or(error!(StakeError::Overflow))?;

        self.user_account.points = self.user_account.points
            .checked_add(elapsed as u32)
            .ok_or(error!(StakeError::Overflow))?;

        if rewards > 0 {
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

    pub fn update_collection_attribute(&self) -> Result<()> {
        let staked_count = self.user_account.amount_staked;
        let attr_key = "staked_count";
        let attr_val = staked_count.to_string();

        let mut data = vec![7]; // UpdateCollectionPluginV1 discriminator
        data.extend_from_slice(&[6]); // Plugin type: Attributes
        let num_attrs: u32 = 1;
        data.extend_from_slice(&num_attrs.to_le_bytes());
        let key_bytes = attr_key.as_bytes();
        data.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(key_bytes);
        let val_bytes = attr_val.as_bytes();
        data.extend_from_slice(&(val_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(val_bytes);

        let accounts = vec![
            AccountMeta::new(self.collection.key(), false),
            AccountMeta::new(self.user.key(), true),
            AccountMeta::new_readonly(self.user.key(), true),
            AccountMeta::new_readonly(self.system_program.key(), false),
            AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false),
        ];

        let ix = Instruction {
            program_id: MPL_CORE_PROGRAM_ID,
            accounts,
            data,
        };

        invoke_signed(
            &ix,
            &[
                self.collection.to_account_info(),
                self.user.to_account_info(),
                self.system_program.to_account_info(),
                self.mpl_core_program.to_account_info(),
            ],
            &[],
        )?;

        Ok(())
    }
}


