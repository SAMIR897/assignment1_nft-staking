use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::invoke;

use crate::constants::MPL_CORE_PROGRAM_ID;
use crate::state::{StakeConfig, StakeAccount, UserAccount};
use crate::errors::StakeError;

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Validated via CPI to mpl-core. Must be owned by the user.
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Validated via CPI to mpl-core. The collection the asset belongs to.
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,

    #[account(
        init,
        payer = user,
        seeds = [b"stake", asset.key().as_ref(), config.key().as_ref()],
        bump,
        space = 8 + StakeAccount::INIT_SPACE,
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

    /// CHECK: This is the mpl-core program
    #[account(address = MPL_CORE_PROGRAM_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> Stake<'info> {
    pub fn stake(&mut self, bumps: &StakeBumps) -> Result<()> {
        let clock = Clock::get()?;

        require!(
            self.user_account.amount_staked < self.config.max_stake,
            StakeError::MaxStakeReached
        );

        self.stake_account.set_inner(StakeAccount {
            owner: self.user.key(),
            mint: self.asset.key(),
            collection: self.collection.key(),
            staked_at: clock.unix_timestamp,
            last_update: clock.unix_timestamp,
            bump: bumps.stake_account,
        });

        self.user_account.amount_staked += 1;

        // Freeze the NFT using raw CPI to mpl-core AddPluginV1
        // Discriminator for AddPluginV1 = 2
        // Plugin: FreezeDelegate { frozen: true } = type 1, data: [1]
        // init_authority: Option::None = [0]
        let mut data = vec![2]; // AddPluginV1 discriminator
        data.extend_from_slice(&[1]); // Plugin type: FreezeDelegate
        data.push(1); // frozen = true
        data.push(0); // init_authority: None

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

        Ok(())
    }

    pub fn update_collection_attribute(&self) -> Result<()> {
        let staked_count = self.user_account.amount_staked;
        let attr_key = "staked_count";
        let attr_val = staked_count.to_string();

        // UpdateCollectionPluginV1 discriminator = 7
        // Plugin type: Attributes = 6
        let mut data = vec![7]; // UpdateCollectionPluginV1 discriminator
        data.extend_from_slice(&[6]); // Plugin type: Attributes
        // Serialize attribute list: length (u32 LE) + key + value
        let num_attrs: u32 = 1;
        data.extend_from_slice(&num_attrs.to_le_bytes());
        // Key: length (u32 LE) + bytes
        let key_bytes = attr_key.as_bytes();
        data.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(key_bytes);
        // Value: length (u32 LE) + bytes
        let val_bytes = attr_val.as_bytes();
        data.extend_from_slice(&(val_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(val_bytes);

        let accounts = vec![
            AccountMeta::new(self.collection.key(), false), // collection
            AccountMeta::new(self.user.key(), true), // payer
            AccountMeta::new_readonly(self.user.key(), true), // authority
            AccountMeta::new_readonly(self.system_program.key(), false), // systemProgram
            AccountMeta::new_readonly(MPL_CORE_PROGRAM_ID, false), // logWrapper
        ];

        let ix = Instruction {
            program_id: MPL_CORE_PROGRAM_ID,
            accounts,
            data,
        };

        invoke(
            &ix,
            &[
                self.collection.to_account_info(),
                self.user.to_account_info(),
                self.system_program.to_account_info(),
                self.mpl_core_program.to_account_info(),
            ],
        )?;

        Ok(())
    }
}


