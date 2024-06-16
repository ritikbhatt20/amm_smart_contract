use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use core::cmp::max;
use solana_program::system_instruction;
use solana_program::msg;

declare_id!("BqQN1TNMcXdL9XsBbTpWtHn6TMhEq5kssmjDvFetVjoa");

#[program]
pub mod solana_amm {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let (_, bump) = Pubkey::find_program_address(&[b"amm"], ctx.program_id);
        let amm = &mut ctx.accounts.amm;
        amm.bump = bump;
        amm.reserve_a = 0;
        amm.reserve_sol = 0;
        Ok(())
    }

    pub fn add_liquidity(ctx: Context<AddLiquidity>, amount_a: u64, sol_amount: u64) -> Result<()> {
        // Adjust amount_a to match the token decimals (multiply by 10^9) for the transfer
        let amount_a_adjusted = amount_a as u128 * 1_000_000_000;

        // Transfer tokens from user to the AMM account
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_a.to_account_info(),
            to: ctx.accounts.amm_token_a.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount_a_adjusted as u64)?;

        // Transfer SOL from user to the AMM account
        let transfer_instruction = system_instruction::transfer(
            &ctx.accounts.user.key(),
            &ctx.accounts.amm.key(),
            sol_amount,
        );
        anchor_lang::solana_program::program::invoke(
            &transfer_instruction,
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.amm.to_account_info(),
            ],
        )?;

        // Update `amm` after transfers
        let amm = &mut ctx.accounts.amm;
        amm.reserve_a += amount_a as u128; // Store the raw amount without additional 9 decimals
        amm.reserve_sol += sol_amount as u128;

        emit!(LiquidityAdded {
            provider: *ctx.accounts.user.key,
            amount_a,
            amount_sol: sol_amount,
        });

        Ok(())
    }

    pub fn remove_liquidity(ctx: Context<RemoveLiquidity>, amount_a: u64) -> Result<()> {
        msg!("Removing liquidity: {} tokens", amount_a);
        let amm = ctx.accounts.amm.clone();
        let amount_a_adjusted = amount_a as u128 * 1_000_000_000;

        require!(amm.reserve_a >= amount_a as u128, AmmError::InsufficientLiquidityA);
        let sol_amount = (amount_a as u128 * amm.reserve_sol) / amm.reserve_a;
        require!(amm.reserve_sol >= sol_amount, AmmError::InsufficientLiquiditySol);

        msg!("Calculated sol_amount to withdraw: {}", sol_amount);

        let seeds = &[b"amm".as_ref(), &[ctx.accounts.amm.bump]];
        let signer_seeds = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.amm_token_a.to_account_info(),
            to: ctx.accounts.user_token_a.to_account_info(),
            authority: ctx.accounts.amm.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, amount_a_adjusted as u64)?;

        **ctx.accounts.amm.to_account_info().try_borrow_mut_lamports()? -= sol_amount as u64;
        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? += sol_amount as u64;

        let amm = &mut ctx.accounts.amm;
        amm.reserve_a -= amount_a as u128;
        amm.reserve_sol -= sol_amount;

        emit!(LiquidityRemoved {
            provider: *ctx.accounts.user.key,
            amount_a,
            amount_sol: sol_amount as u64,
        });

        msg!("Liquidity removed successfully");
        Ok(())
    }

    pub fn buy(ctx: Context<Buy>, sol_amount: u64) -> Result<()> {
        let amm = ctx.accounts.amm.clone();

        // Transfer SOL from user to the AMM account
        let transfer_instruction = system_instruction::transfer(
            &ctx.accounts.user.key(),
            &ctx.accounts.amm.key(),
            sol_amount,
        );
        anchor_lang::solana_program::program::invoke(
            &transfer_instruction,
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.amm.to_account_info(),
            ],
        )?;

        // Calculate token amount with 9 decimals precision
        let token_amount = get_amount_out(sol_amount as u128, amm.reserve_sol, amm.reserve_a);
        let token_amount_adjusted = token_amount * 1_000_000_000;

        // Transfer tokens from AMM to user
        let seeds = &[b"amm".as_ref(), &[ctx.accounts.amm.bump]];
        let signer_seeds = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.amm_token_a.to_account_info(),
            to: ctx.accounts.user_token_a.to_account_info(),
            authority: ctx.accounts.amm.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, token_amount_adjusted as u64)?;

        // Update `amm` after transfers
        let amm = &mut ctx.accounts.amm;
        amm.reserve_sol += sol_amount as u128;
        amm.reserve_a -= token_amount; // Subtract the raw amount

        emit!(TokensBought {
            buyer: *ctx.accounts.user.key,
            amount_sol: sol_amount,
            amount_a: token_amount as u64,
        });

        Ok(())
    }

    pub fn sell(ctx: Context<Sell>, amount_a: u64) -> Result<()> {
        let amm = ctx.accounts.amm.clone();

        let amount_a_adjusted = amount_a as u128 * 1_000_000_000;
        msg!("Amount A Adjusted: {}", amount_a_adjusted);

        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_a.to_account_info(),
            to: ctx.accounts.amm_token_a.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount_a_adjusted as u64)?;

        let sol_amount = get_amount_out(amount_a as u128, amm.reserve_a, amm.reserve_sol) as u64;

        **ctx.accounts.amm.to_account_info().try_borrow_mut_lamports()? -= sol_amount;
        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? += sol_amount;

        let amm = &mut ctx.accounts.amm;
        amm.reserve_a += amount_a as u128;
        amm.reserve_sol -= sol_amount as u128;

        emit!(TokensSold {
            seller: *ctx.accounts.user.key,
            amount_a,
            amount_sol: sol_amount,
        });

        Ok(())
    }

    pub fn get_price(ctx: Context<GetPrice>) -> Result<()> {
        let amm = &ctx.accounts.amm;

        // Ensure that the reserves are not zero to avoid division by zero
        if amm.reserve_a == 0 || amm.reserve_sol == 0 {
            return Err(AmmError::InsufficientLiquidity.into());
        }

        // Calculate price as reserve_sol / reserve_a
        let price = (amm.reserve_sol / amm.reserve_a) as u64;
        msg!("Price of Token A in Lamports: {:.10}", price);

        Ok(())
    }

}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 8 + 48, seeds = [b"amm"], bump)]
    pub amm: Account<'info, Amm>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    #[account(mut, seeds = [b"amm"], bump = amm.bump)]
    pub amm: Account<'info, Amm>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_a: Account<'info, TokenAccount>,
    #[account(mut)]
    pub amm_token_a: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RemoveLiquidity<'info> {
    #[account(mut, seeds = [b"amm"], bump = amm.bump)]
    pub amm: Account<'info, Amm>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_a: Account<'info, TokenAccount>,
    #[account(mut)]
    pub amm_token_a: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Buy<'info> {
    #[account(mut, seeds = [b"amm"], bump = amm.bump)]
    pub amm: Account<'info, Amm>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_a: Account<'info, TokenAccount>,
    #[account(mut)]
    pub amm_token_a: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Sell<'info> {
    #[account(mut, seeds = [b"amm"], bump = amm.bump)]
    pub amm: Account<'info, Amm>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_a: Account<'info, TokenAccount>,
    #[account(mut)]
    pub amm_token_a: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct GetPrice<'info> {
    #[account(seeds = [b"amm"], bump = amm.bump)]
    pub amm: Account<'info, Amm>,
}

#[account]
pub struct Amm {
    pub reserve_a: u128, // This stores tokens with 9 decimal places
    pub reserve_sol: u128,
    pub bump: u8,
}

fn get_amount_out(input_amount: u128, input_reserve: u128, output_reserve: u128) -> u128 {
    // Apply the constant product formula with fee on output
    let input_amount_with_fee = input_amount * 997; // Apply fee
    let numerator = input_amount_with_fee * output_reserve;
    let denominator = input_reserve * 1000 + input_amount_with_fee;

    // Adjust denominator for fee
    if denominator > 0 {
        numerator / denominator
    } else {
        0 // Handle potential division by zero scenario gracefully
    }
}


#[event]
pub struct LiquidityAdded {
    pub provider: Pubkey,
    pub amount_a: u64,
    pub amount_sol: u64,
}

#[event]
pub struct LiquidityRemoved {
    pub provider: Pubkey,
    pub amount_a: u64,
    pub amount_sol: u64,
}

#[event]
pub struct TokensBought {
    pub buyer: Pubkey,
    pub amount_sol: u64,
    pub amount_a: u64,
}

#[event]
pub struct TokensSold {
    pub seller: Pubkey,
    pub amount_a: u64,
    pub amount_sol: u64,
}

#[error_code]
pub enum AmmError {
    #[msg("Insufficient liquidity for TokenA")]
    InsufficientLiquidityA,
    #[msg("Insufficient liquidity for SOL")]
    InsufficientLiquiditySol,
    #[msg("Insufficient liquidity for price calculation")]
    InsufficientLiquidity,
}