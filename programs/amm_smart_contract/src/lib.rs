use anchor_lang::prelude::*;

declare_id!("BqQN1TNMcXdL9XsBbTpWtHn6TMhEq5kssmjDvFetVjoa");

#[program]
pub mod amm_smart_contract {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
