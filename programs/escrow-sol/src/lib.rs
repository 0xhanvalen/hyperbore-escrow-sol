#![allow(clippy::result_large_err)]
use anchor_lang::prelude::*;
use anchor_lang::solana_program::clock::Clock;
declare_id!("qbuMdeYxYJXBjU6C6qFKjZKjXmrU83eDQomHdrch826");

#[program]
pub mod test {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, tax: u16, fee: u8) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.judge = *ctx.accounts.owner.key;
        config.treasury = *ctx.accounts.treasury.key;
        config.tax = tax;
        config.fee = fee;
        config.bump = ctx.bumps.config;
        emit!(ConfigCreated {
            address: config.key(),
            treasury: config.treasury,
            judge: config.judge,
            tax: config.tax,
            fee: config.fee,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn update_config(ctx: Context<UpdateContext>, updates: ConfigUpdateArgs) -> Result<()> {
        let config = &mut ctx.accounts.config;
        if let Some(new_treasury) = updates.treasury {
            config.treasury = new_treasury;
        }
        if let Some(new_tax) = updates.tax {
            if new_tax > 2000 {
                return Err(error!(ErrorCode::TaxTooHigh));
            }
            config.tax = new_tax;
        }
        if let Some(new_fee) = updates.fee {
            if new_fee > 20 {
                return Err(error!(ErrorCode::FeeTooHigh));
            }
            config.fee = new_fee;
        }
        if let Some(new_judge) = updates.pending_judge {
            config.pending_judge = Some(new_judge);
            emit!(JudgeNominated {
                address: config.key(),
                pending_judge: new_judge,
                timestamp: Clock::get()?.unix_timestamp,
            })
        }
        emit!(ConfigUpdated {
            address: config.key(),
            treasury: config.treasury,
            pending_judge: config.pending_judge,
            tax: config.tax,
            fee: config.fee,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn accept_judge_seat(ctx: Context<AcceptJudgeSeatContext>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        let old_judge = config.judge;
        if let Some(new_judge) = config.pending_judge {
            config.judge = new_judge;
            config.pending_judge = None;
            emit!(JudgeAccepted {
                address: config.key(),
                old_judge,
                new_judge: config.judge,
                timestamp: Clock::get()?.unix_timestamp,
            });
        }
        
        Ok(())
    }

    pub fn create_escrow(ctx: Context<CreateEscrowContext>, args: EscrowCreationArgs) -> Result<()> {
        let escrow = &mut ctx.accounts.escrow;
        let config = &ctx.accounts.config;
        if args.amount * (config.fee as u64) < 1 {
            return Err(error!(ErrorCode::InvalidEscrowAmount));
        }
        escrow.payer = ctx.accounts.payer.key();
        escrow.payee = args.payee;
        let now = Clock::get()?.unix_timestamp;
        escrow.creation_time = now;
        escrow.deadline = now + (14 * 24 * 60 * 60);
        escrow.judge_deadline = escrow.deadline + (28 * 24 * 60 * 60);
        escrow.amount = args.amount;
        escrow.token_mint = args.token_mint;
        escrow.bump = ctx.bumps.escrow;
        emit!(EscrowCreated {
            address: escrow.key(),
            payer: escrow.payer,
            payee: escrow.payee,
            amount: escrow.amount,
            token_mint: escrow.token_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn dispute_escrow(ctx: Context<DisputeEscrowContext>) -> Result<()> {
        let escrow = &mut ctx.accounts.escrow;
        let user = &ctx.accounts.user;
        let config = &ctx.accounts.config;
        // Judges can't get involved until after the deadline
        if user.key() == config.judge {
            let now = Clock::get()?.unix_timestamp;
            if now <= escrow.deadline {
                return Err(error!(ErrorCode::UninvolvedUser));
            }
        }
        escrow.disputed = true;
        Ok(())
    }

    pub fn judge_sol_escrow(ctx: Context<JudgeSolanaContext>, decision: bool) -> Result<()> {
        let escrow = &ctx.accounts.escrow;
        let config = &ctx.accounts.config;
        let treasury_info = &mut ctx.accounts.treasury.to_account_info();
        let escrow_info = &mut ctx.accounts.escrow.to_account_info();
        // get the rent
        let rent = Rent::get()?;
        let rent_exemption = rent.minimum_balance(escrow_info.data_len());
        let amount = escrow.amount + rent_exemption;
        // % fee for requiring judgement
        let fee = amount * ((config.fee as u64) / 100);
        if !decision {
            let payer_info = &mut ctx.accounts.payer.to_account_info();
            let payer_amount = amount - fee;
            **escrow_info.try_borrow_mut_lamports()? -= amount;
            **treasury_info.try_borrow_mut_lamports()? += fee;
            **payer_info.try_borrow_mut_lamports()? += payer_amount;
        }
        if decision {
            let payee_info = &mut ctx.accounts.payee.to_account_info();
            let payee_amount = amount - fee;
            **escrow_info.try_borrow_mut_lamports()? -= amount;
            **treasury_info.try_borrow_mut_lamports()? += fee;
            **payee_info.try_borrow_mut_lamports()? += payee_amount;
        }
        Ok(())
    }

    pub fn deposit_sol_funds(ctx: Context<DepositSolanaContext>) -> Result<()> {
        let payer_info = &mut ctx.accounts.payer.to_account_info();
        let escrow = &ctx.accounts.escrow;
        let escrow_info = &mut ctx.accounts.escrow.to_account_info();
        // get the rent
        let rent = Rent::get()?;
        let rent_exemption = rent.minimum_balance(escrow_info.data_len());
        // deposit the sol
        let amount = escrow.amount + rent_exemption;
        **payer_info.try_borrow_mut_lamports()? -= amount;
        **escrow_info.try_borrow_mut_lamports()? += amount;
        Ok(())
    }

    pub fn release_sol_funds(ctx: Context<ReleaseSolanaContext>) -> Result<()> {
        let config = &ctx.accounts.config;
        let payee_info = &mut ctx.accounts.payee.to_account_info();
        let escrow_info = &mut ctx.accounts.escrow.to_account_info();
        let treasury_info = &mut ctx.accounts.treasury.to_account_info();
        // get the rent
        let rent = Rent::get()?;
        let rent_exemption = rent.minimum_balance(escrow_info.data_len());
        let remaining_lamports = escrow_info.lamports();
        let amount = remaining_lamports - rent_exemption;
        // Do the taxes for the dao
        let fee = amount * ((config.fee as u64) / 10000);
        let payee_amount = amount - fee;
        **escrow_info.try_borrow_mut_lamports()? -= amount;
        **treasury_info.try_borrow_mut_lamports()? += fee;
        **payee_info.try_borrow_mut_lamports()? += payee_amount;

        Ok(())
    }

    pub fn return_sol_funds(ctx: Context<ReturnSolanaContext>) -> Result<()> {
        let config = &ctx.accounts.config;
        let payer_info = &mut ctx.accounts.payee.to_account_info();
        let escrow_info = &mut ctx.accounts.escrow.to_account_info();
        let treasury_info = &mut ctx.accounts.treasury.to_account_info();
        // get the rent
        let rent = Rent::get()?;
        let rent_exemption = rent.minimum_balance(escrow_info.data_len());
        let remaining_lamports = escrow_info.lamports();
        let amount = remaining_lamports - rent_exemption;
        // Do the taxes for the dao
        let fee = amount * ((config.fee as u64) / 10000);
        let payer_amount = amount - fee;
        **escrow_info.try_borrow_mut_lamports()? -= amount;
        **treasury_info.try_borrow_mut_lamports()? += fee;
        **payer_info.try_borrow_mut_lamports()? += payer_amount;

        Ok(())
    }

    //todo: recover_sol_funds after deadline
}

// ============================================================================================================== //
//          -                 Account Contexts                                                                    //
//         -:                  _______  _______  _______ _________     _______ _________          _______         //
//         ::      --         (  ___  )(  ____ \(  ____ \\__   __/    (  ____ \\__   __/|\     /|(  ____ \        //
//  ::::  :.:::::::-          | (   ) || (    \/| (    \/   ) (       | (    \/   ) (   ( \   / )| (    \/        //
//    :::::.:.::-             | (___) || |      | |         | |       | |         | |    \ (_) / | (_____         //
//      ::.::::..:            |  ___  || |      | |         | |       | |         | |     ) _ (  (_____  )        //
//      :::.::.:::::          | (   ) || |      | |         | |       | |         | |    / ( ) \       ) |        //
//   -:::::: ::   ----        | )   ( || (____/\| (____/\   | |       | (____/\   | |   ( /   \ )/\____) |        //
//   -       ::               |/     \|(_______/(_______/   )_(       (_______/   )_(   |/     \|\_______)        //
//           :                                                                                                    //
// ============================================================================================================== //                                                               

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    /// CHECK: treasury wallet address for deriving token accounts
    pub treasury: AccountInfo<'info>,

    #[account(
        init,
        payer = owner,
        space = 8 + ConfigAccount::INIT_SPACE,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, ConfigAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateContext<'info> {
    #[account(mut)]
    pub judge: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = judge, // Ensures the signer matches config.judge
    )]
    pub config: Account<'info, ConfigAccount>,
}

#[derive(Accounts)]
pub struct AcceptJudgeSeatContext<'info> {
    #[account(mut)]
    pub pending_judge: Signer<'info>,

    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.pending_judge.is_some() @ ErrorCode::NoPendingJudge,
        constraint = config.pending_judge.unwrap() == pending_judge.key() @ ErrorCode::UnauthorizedJudge,
    )]
    pub config: Account<'info, ConfigAccount>,
}

#[derive(Accounts)]
pub struct CreateEscrowContext<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, ConfigAccount>,

    #[account(
        init,
        payer = payer,
        space = 8 + EscrowAccount::INIT_SPACE,
        seeds = [b"escrow", payer.key().as_ref()],
        bump
    )]
    pub escrow: Account<'info, EscrowAccount>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DisputeEscrowContext<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, ConfigAccount>,

    // #[account(mut)]
    // pub escrow: Account<'info, EscrowAccount>,
    #[account(
        seeds = [b"escrow", escrow.payer.as_ref()],
        bump = escrow.bump,
        constraint = (!escrow.disputed) @ ErrorCode::EscrowDisputed,
        constraint = (user.key() == escrow.payer || user.key() == escrow.payee || user.key() == config.judge) @ ErrorCode::UninvolvedUser,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct DepositSolanaContext<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, ConfigAccount>,

    #[account(
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = !escrow.token_mint.is_some() @ ErrorCode::EscrowNotSolana,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct ReleaseSolanaContext<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: This is payee pubkey
    #[account(mut)]
    pub payee: AccountInfo<'info>,

    /// CHECK: This is treasury pubkey
    #[account(mut)]
    pub treasury: AccountInfo<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, ConfigAccount>,

    #[account(
        mut,
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = !escrow.token_mint.is_some() @ ErrorCode::EscrowNotSolana,
        constraint = escrow.payer == payer.key() @ ErrorCode::NotPayerReleasing,
        constraint = escrow.payee == payee.key() @ ErrorCode::NotPayeeReceiving,
        constraint = !escrow.disputed @ ErrorCode::EscrowDisputed,
        close = payer,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct ReturnSolanaContext<'info> {
    #[account(mut)]
    pub payee: Signer<'info>,

    /// CHECK: This is payer pubkey
    #[account(mut)]
    pub payer: AccountInfo<'info>,

    /// CHECK: This is treasury pubkey
    #[account(mut)]
    pub treasury: AccountInfo<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, ConfigAccount>,

    #[account(
        mut,
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = !escrow.token_mint.is_some() @ ErrorCode::EscrowNotSolana,
        constraint = escrow.payer == payer.key() @ ErrorCode::NotPayerReturning,
        constraint = escrow.payee == payee.key() @ ErrorCode::NotPayeeReturning,
        constraint = !escrow.disputed @ ErrorCode::EscrowDisputed,
        close = payer,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct JudgeSolanaContext<'info> {
    #[account(mut)]
    pub judge: Signer<'info>,

    /// CHECK: This is payee pubkey
    #[account(mut)]
    pub payee: AccountInfo<'info>,

    /// CHECK: This is payee pubkey
    #[account(mut)]
    pub payer: AccountInfo<'info>,

    /// CHECK: This is treasury pubkey
    #[account(mut)]
    pub treasury: AccountInfo<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.treasury == treasury.key() @ ErrorCode::UninvolvedUser,
        constraint = config.judge == judge.key() @ ErrorCode::UninvolvedUser,
    )]
    pub config: Account<'info, ConfigAccount>,

    #[account(
        mut,
        seeds = [b"escrow", escrow.payer.as_ref()],
        bump = escrow.bump,
        constraint = !escrow.token_mint.is_some() @ ErrorCode::EscrowNotSolana,
        constraint = escrow.disputed @ ErrorCode::EscrowNotDisputed,
        constraint = escrow.payee == payee.key() @ ErrorCode::UninvolvedUser,
        constraint = escrow.payer == payer.key() @ ErrorCode::UninvolvedUser,
        close = payer,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    pub system_program: Program<'info, System>
}
// ============================================================================================================================== //
//          -                 Function Argument Definitions                                                                       //
//         -:                  _______  _______  _______    _______ _________ _______           _______ _________ _______         //
//         ::      --         (  ___  )(  ____ )(  ____ \  (  ____ \\__   __/(  ____ )|\     /|(  ____ \\__   __/(  ____ \        //
//  ::::  :.:::::::-          | (   ) || (    )|| (    \/  | (    \/   ) (   | (    )|| )   ( || (    \/   ) (   | (    \/        //
//    :::::.:.::-             | (___) || (____)|| |        | (_____    | |   | (____)|| |   | || |         | |   | (_____         //
//      ::.::::..:            |  ___  ||     __)| | ____   (_____  )   | |   |     __)| |   | || |         | |   (_____  )        //
//      :::.::.:::::          | (   ) || (\ (   | | \_  )        ) |   | |   | (\ (   | |   | || |         | |         ) |        //
//   -:::::: ::   ----        | )   ( || ) \ \__| (___) |  /\____) |   | |   | ) \ \__| (___) || (____/\   | |   /\____) |        //
//   -       ::               |/     \||/   \__/(_______)  \_______)   )_(   |/   \__/(_______)(_______/   )_(   \_______)        //
//           :                                                                                                                    //
// ============================================================================================================================== //

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ConfigUpdateArgs {
    pub treasury: Option<Pubkey>,
    pub pending_judge: Option<Pubkey>,
    pub tax: Option<u16>,
    pub fee: Option<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct EscrowCreationArgs {
    pub amount: u64,
    pub payee: Pubkey,
    pub deadline: i64,
    pub judge_deadline: i64,
    pub token_mint: Option<Pubkey>
}

// ============================================================================================================ //
//          -                 Account Definitions                                                               //
//         -:                  _______  _______  _______ _________   ______   _______  _______  _______         //
//         ::      --         (  ___  )(  ____ \(  ____ \\__   __/  (  __  \ (  ____ \(  ____ \(  ____ \        //
//  ::::  :.:::::::-          | (   ) || (    \/| (    \/   ) (     | (  \  )| (    \/| (    \/| (    \/        //
//    :::::.:.::-             | (___) || |      | |         | |     | |   ) || (__    | (__    | (_____         //
//      ::.::::..:            |  ___  || |      | |         | |     | |   | ||  __)   |  __)   (_____  )        //
//      :::.::.:::::          | (   ) || |      | |         | |     | |   ) || (      | (            ) |        //
//   -:::::: ::   ----        | )   ( || (____/\| (____/\   | |     | (__/  )| (____/\| )      /\____) |        //
//   -       ::               |/     \|(_______/(_______/   )_(     (______/ (_______/|/       \_______)        //
//           :                                                                                                  //
// ============================================================================================================ //     

#[account]
#[derive(InitSpace)]
pub struct ConfigAccount {
    pub judge: Pubkey,
    pub treasury: Pubkey,
    pub pending_judge: Option<Pubkey>,
    pub tax: u16, // BPS Fee for all transactions
    pub fee: u8, // Incentivize the DAO to rule on escrows
    pub bump: u8, // Store the bump for verification later
}

#[account]
#[derive(InitSpace)]
pub struct EscrowAccount {
    pub payer: Pubkey,              // The person depositing funds
    pub payee: Pubkey,              // The recipient who should receive funds
    pub amount: u64,                // Amount held in escrow
    pub token_mint: Option<Pubkey>, // If None, this is a SOL escrow, otherwise an SPL token
    pub disputed: bool,             // 0 = Active, 1 = Disputed
    pub deadline: i64,
    pub judge_deadline: i64,
    pub creation_time: i64,         // When escrow was created (unix timestamp)
    pub bump: u8,                   // Bump for PDA verification
}

// ======================================================================================== //
//          -                 Events                                                        //
//         -:                  _______           _______  _       _________ _______         //
//         ::      --         (  ____ \|\     /|(  ____ \( (    /|\__   __/(  ____ \        //
//  ::::  :.:::::::-          | (    \/| )   ( || (    \/|  \  ( |   ) (   | (    \/        //
//    :::::.:.::-             | (__    | |   | || (__    |   \ | |   | |   | (_____         //
//      ::.::::..:            |  __)   ( (   ) )|  __)   | (\ \) |   | |   (_____  )        //
//      :::.::.:::::          | (       \ \_/ / | (      | | \   |   | |         ) |        //
//   -:::::: ::   ----        | (____/\  \   /  | (____/\| )  \  |   | |   /\____) |        //
//   -       ::               (_______/   \_/   (_______/|/    )_)   )_(   \_______)        //
//           :                                                                              //
// ======================================================================================== // 
                                                      

#[event]
pub struct ConfigCreated {
    pub address: Pubkey,
    pub treasury: Pubkey,
    pub judge: Pubkey,
    pub tax: u16,
    pub fee: u8,
    pub timestamp: i64,  // Optional: include the current timestamp
}

#[event]
pub struct ConfigUpdated {
    pub address: Pubkey,
    pub treasury: Pubkey,
    pub pending_judge: Option<Pubkey>,
    pub tax: u16,
    pub fee: u8,
    pub timestamp: i64,
}

#[event]
pub struct JudgeNominated {
    pub address: Pubkey,
    pub pending_judge: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct JudgeAccepted {
    pub address: Pubkey,
    pub old_judge: Pubkey,
    pub new_judge: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct EscrowCreated {
    pub address: Pubkey,
    pub payer: Pubkey,             
    pub payee: Pubkey,             
    pub amount: u64,               
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,        
}

#[error_code]
pub enum ErrorCode {
    #[msg("Tax rate exceeds maximum of 2000 basis points (20%)")]
    TaxTooHigh,
    
    #[msg("Judge fee exceeds maximum of 20%")]
    FeeTooHigh,
    
    #[msg("Invalid treasury address")]
    InvalidTreasury,
    
    #[msg("Unauthorized: Only the judge can perform this action")]
    UnauthorizedConfigOwner,
    
    #[msg("Unauthorized: Only the judge can perform this action")]
    UnauthorizedConfigJudge,
    
    #[msg("Config account does not match the expected PDA")]
    InvalidConfigAccount,
    
    #[msg("Operation would result in insufficient funds")]
    InsufficientFunds,

    #[msg("Uninvolved user")]
    UninvolvedUser,

    #[msg("Operation failed - can not accept judge seat unless nominated - no current nominee")]
    NoPendingJudge,

    #[msg("Signer is not current nominee")]
    UnauthorizedJudge,

    #[msg("Escrow creation failed - amount is too small")]
    InvalidEscrowAmount,

    #[msg("Operation failed - trying to perform SOL operations on Token escrow")]
    EscrowNotSolana,

    #[msg("Operation failed - trying to release funds from wrong Payer")]
    NotPayerReleasing,

    #[msg("Operation failed - trying to return funds to wrong Payer")]
    NotPayerReturning,

    #[msg("Operation failed - trying to release funds to wrong Payee")]
    NotPayeeReceiving,

    #[msg("Operation failed - trying to return funds from wrong Payee")]
    NotPayeeReturning,

    #[msg("Operation failed - Escrow is in Dispute")]
    EscrowDisputed,

    #[msg("Operation failed - Escrow is not in Dispute")]
    EscrowNotDisputed
}