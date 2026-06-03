use anchor_lang::prelude::*;

pub mod constants;
pub mod contexts;
pub mod state;
pub mod errors;

pub use contexts::*;
pub use errors::*;

declare_id!("FQGYYsWcChrmwbBYsZra2vwv2zsFWDKqP5gKp2J4s8io");

#[program]
pub mod nft_staking {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, points_per_stake: u8, max_stake: u8, freeze_period: u32) -> Result<()> {
        ctx.accounts.init(points_per_stake, max_stake, freeze_period, &ctx.bumps)
    }

    pub fn init_user(ctx: Context<InitUser>) -> Result<()> {
        ctx.accounts.init_user(&ctx.bumps)
    }

    pub fn stake(ctx: Context<Stake>) -> Result<()> {
        ctx.accounts.stake(&ctx.bumps)?;
        ctx.accounts.update_collection_attribute()
    }

    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        ctx.accounts.claim_rewards()
    }

    pub fn unstake(ctx: Context<Unstake>) -> Result<()> {
        ctx.accounts.unstake()?;
        ctx.accounts.update_collection_attribute()
    }
}
