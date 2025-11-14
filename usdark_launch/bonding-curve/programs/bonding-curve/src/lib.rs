use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("YourProgramIDHere");  // Replace after `anchor deploy`

#[program]
pub mod bonding_curve {
    use super::*;

    pub fn initialize_curve(ctx: Context<InitializeCurve>, total_supply: u64, decimals: u8, target_sol: u64) -> Result<()> {
        let curve = &mut ctx.accounts.curve;
        curve.mint = ctx.accounts.mint.key();
        curve.total_supply = total_supply;
        curve.decimals = decimals;
        curve.target_sol = target_sol * 1_000_000_000;  // Lamports
        curve.sol_reserves = 0;
        curve.token_reserves = total_supply;  // All tokens start in PDA
        curve.minted_tokens = 0;
        curve.authority = ctx.accounts.user.key();  // Creator as admin (for migration)
        Ok(())
    }

    pub fn buy_tokens(ctx: Context<BuyTokens>, sol_amount: u64) -> Result<()> {
        let curve = &mut ctx.accounts.curve;
        require!(curve.sol_reserves + sol_amount <= curve.target_sol, ErrorCode::TargetExceeded);

        // Linear curve: tokens_out = (sol_in / target_sol) * remaining_tokens
        let remaining_tokens = curve.token_reserves - curve.minted_tokens;
        let tokens_out = ((sol_amount as u128 * remaining_tokens as u128) / curve.target_sol as u128) as u64;

        require!(tokens_out > 0, ErrorCode::NoTokensOut);

        // Transfer SOL to PDA
        let cpi_accounts = anchor_lang::system_program::Transfer {
            from: ctx.accounts.buyer.to_account_info(),
            to: ctx.accounts.curve.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.system_program.to_account_info(), cpi_accounts);
        anchor_lang::system_program::transfer(cpi_ctx, sol_amount)?;

        curve.sol_reserves += sol_amount;

        // Transfer tokens from PDA to buyer
        let seeds = &[b"curve".as_ref(), &[ctx.bumps.curve]];
        let signer = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.curve_token_account.to_account_info(),
            to: ctx.accounts.buyer_token_account.to_account_info(),
            authority: ctx.accounts.curve.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), cpi_accounts, signer);
        token::transfer(cpi_ctx, tokens_out)?;

        curve.minted_tokens += tokens_out;
        curve.token_reserves -= tokens_out;  // Update reserves

        // Optional: 1% fee to creator
        let fee = sol_amount / 100;
        if fee > 0 {
            let fee_cpi = anchor_lang::system_program::Transfer {
                from: ctx.accounts.curve.to_account_info(),
                to: ctx.accounts.creator.to_account_info(),
            };
            anchor_lang::system_program::transfer(CpiContext::new(ctx.accounts.system_program.to_account_info(), fee_cpi), fee)?;
            curve.sol_reserves -= fee;
        }

        // Check graduation
        if curve.sol_reserves >= curve.target_sol {
            // Emit event or CPI to migrate (stub: call Raydium create_pool)
            msg!("Graduated! Migrate to Raydium.");
        }

        Ok(())
    }

    // Sell similar: tokens_in -> sol_out, reverse calc
    pub fn sell_tokens(ctx: Context<SellTokens>, tokens_amount: u64) -> Result<()> { /* Impl similar to buy */ Ok(()) }
}

#[derive(Accounts)]
pub struct InitializeCurve<'info> {
    #[account(init, payer = user, space = 8 + 32 + 8*6)]
    pub curve: Account<'info, Curve>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub curve_token_account: Account<'info, TokenAccount>,  // PDA's ATA for tokens
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(mut, seeds = [b"curve", mint.key().as_ref()], bump)]
    pub curve: Account<'info, Curve>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    #[account(mut)]  // PDA's token account
    pub curve_token_account: Account<'info, TokenAccount>,
    #[account(mut)]  // Buyer's ATA
    pub buyer_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub buyer: Signer<'info>,
    /// CHECK: Creator for fee
    #[account(mut)]
    pub creator: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct Curve {
    pub mint: Pubkey,
    pub total_supply: u64,
    pub decimals: u8,
    pub target_sol: u64,
    pub sol_reserves: u64,
    pub token_reserves: u64,
    pub minted_tokens: u64,
    pub authority: Pubkey,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Target SOL exceeded")]
    TargetExceeded,
    #[msg("No tokens out")]
    NoTokensOut,
}