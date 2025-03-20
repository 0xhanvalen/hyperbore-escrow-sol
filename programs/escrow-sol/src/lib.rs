#![allow(clippy::result_large_err)]
use anchor_lang::prelude::*;
use anchor_lang::solana_program::clock::Clock;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};

declare_id!("qbuMdeYxYJXBjU6C6qFKjZKjXmrU83eDQomHdrch826");

pub const AUTHORIZED_LAUNCHER: Pubkey = pubkey!("9FEDyP1t345xFKVrJPN2TgQvQEJGz8KXE2xPV6TVXYY6");

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
        if ((args.amount * (config.tax as u64))/ 10000) < 1 {
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
        escrow.tax = config.tax;
        escrow.fee = config.fee;
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
        emit!(EscrowDisputed {
            address: escrow.key(),
            payer: escrow.payer,
            payee: escrow.payee,
            disputed_by: user.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });
        escrow.disputed = true;
        Ok(())
    }

    pub fn judge_sol_escrow(ctx: Context<JudgeSolanaContext>, decision: bool) -> Result<()> {
        let escrow = &ctx.accounts.escrow;
        let treasury_info = &mut ctx.accounts.treasury.to_account_info();
        let escrow_info = &mut ctx.accounts.escrow.to_account_info();
        // get the rent
        let rent = Rent::get()?;
        let rent_exemption = rent.minimum_balance(escrow_info.data_len());
        let amount = escrow.amount + rent_exemption;
        // % fee for requiring judgement
        let fee = amount * ((escrow.fee as u64) / 100);
        if !decision {
            let payer_info = &mut ctx.accounts.payer.to_account_info();
            let payer_amount = amount - fee;
            **escrow_info.try_borrow_mut_lamports()? -= amount;
            **treasury_info.try_borrow_mut_lamports()? += fee;
            **payer_info.try_borrow_mut_lamports()? += payer_amount;
            emit!(EscrowJudged {
                address: escrow.key(),
                winner: escrow.payer,
                amount_awarded: payer_amount,
                fee_collected: fee,
                token_mint: escrow.token_mint,
                timestamp: Clock::get()?.unix_timestamp,
            });
        }
        if decision {
            let payee_info = &mut ctx.accounts.payee.to_account_info();
            let payee_amount = amount - fee;
            **escrow_info.try_borrow_mut_lamports()? -= amount;
            **treasury_info.try_borrow_mut_lamports()? += fee;
            **payee_info.try_borrow_mut_lamports()? += payee_amount;
            emit!(EscrowJudged {
                address: escrow.key(),
                winner: escrow.payer,
                amount_awarded: payee_amount,
                fee_collected: fee,
                token_mint: escrow.token_mint,
                timestamp: Clock::get()?.unix_timestamp,
            });
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
        emit!(EscrowDeposited {
            address: escrow.key(),
            amount: amount,
            token_mint: escrow.token_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn release_sol_funds(ctx: Context<ReleaseSolanaContext>) -> Result<()> {
        let escrow = &ctx.accounts.escrow;
        let payee_info = &mut ctx.accounts.payee.to_account_info();
        let escrow_info = &mut ctx.accounts.escrow.to_account_info();
        let treasury_info = &mut ctx.accounts.treasury.to_account_info();
        // get the rent
        let rent = Rent::get()?;
        let rent_exemption = rent.minimum_balance(escrow_info.data_len());
        let remaining_lamports = escrow_info.lamports();
        let amount = remaining_lamports - rent_exemption;
        // Do the taxes for the dao
        let fee = amount * ((escrow.fee as u64) / 10000);
        let payee_amount = amount - fee;
        **escrow_info.try_borrow_mut_lamports()? -= amount;
        **treasury_info.try_borrow_mut_lamports()? += fee;
        **payee_info.try_borrow_mut_lamports()? += payee_amount;
        emit!(EscrowReleased {
            address: escrow.key(),
            amount: payee_amount,
            tax_paid: fee,
            token_mint: escrow.token_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn return_sol_funds(ctx: Context<ReturnSolanaContext>) -> Result<()> {
        let escrow = &ctx.accounts.escrow;
        let payer_info = &mut ctx.accounts.payee.to_account_info();
        let escrow_info = &mut ctx.accounts.escrow.to_account_info();
        let treasury_info = &mut ctx.accounts.treasury.to_account_info();
        // get the rent
        let rent = Rent::get()?;
        let rent_exemption = rent.minimum_balance(escrow_info.data_len());
        let remaining_lamports = escrow_info.lamports();
        let amount = remaining_lamports - rent_exemption;
        // Do the taxes for the dao
        let fee = amount * ((escrow.tax as u64) / 10000);
        let payer_amount = amount - fee;
        **escrow_info.try_borrow_mut_lamports()? -= amount;
        **treasury_info.try_borrow_mut_lamports()? += fee;
        **payer_info.try_borrow_mut_lamports()? += payer_amount;
        emit!(EscrowReturned {
            address: escrow.key(),
            amount: payer_amount,
            tax_paid: fee,
            token_mint: escrow.token_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn recover_sol_funds(ctx: Context<RecoverSolanaContext>) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let escrow = &mut ctx.accounts.escrow;
        if now <= escrow.judge_deadline {
            return Err(error!(ErrorCode::RecoverTooEarly));
        }
        let escrow_info = &escrow.to_account_info();
        let payer_info = &mut ctx.accounts.payer.to_account_info();
        let rent = Rent::get()?;
        let rent_exemption = rent.minimum_balance(escrow_info.data_len());
        let remaining_lamports = escrow_info.lamports();
        let amount = remaining_lamports - rent_exemption;
        **escrow_info.try_borrow_mut_lamports()? -= amount;
        **payer_info.try_borrow_mut_lamports()? += amount;
        emit!(EscrowRecovered {
            address: escrow.key(),
            amount: amount,
            token_mint: escrow.token_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn deposit_token_funds(ctx: Context<DepositTokenContext>) -> Result<()> {
        let escrow = &mut ctx.accounts.escrow;
        if let Some(_token_mint_pubkey) = escrow.token_mint {
            let payer = &mut ctx.accounts.payer;
            let payer_token_account = &ctx.accounts.payer_token_account;
            let escrow_token_account = &ctx.accounts.escrow_token_account;
            transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                from: payer_token_account.to_account_info(),
                to: escrow_token_account.to_account_info(),
                authority: payer.to_account_info(),
                },
            ),
            escrow.amount,
            )?;
            emit!(EscrowDeposited {
            address: escrow.key(),
            amount: escrow.amount,
            token_mint: escrow.token_mint,
            timestamp: Clock::get()?.unix_timestamp,
            });
        } else {
            return Err(error!(ErrorCode::EscrowNotToken))
        }
        Ok(())
    }

    pub fn judge_token_escrow(ctx: Context<JudgeTokenContext>, decision: bool) -> Result<()> {
        let escrow = &ctx.accounts.escrow;
        let treasury_token_account = &mut ctx.accounts.treasury_token_account;
        let payer_token_account = &mut ctx.accounts.payer_token_account;
        let payee_token_account = &mut ctx.accounts.payee_token_account;
        let escrow_token_account = &mut ctx.accounts.escrow_token_account;
        if let Some(_token_mint_pubkey) = escrow.token_mint {
            // calculate fee
            let init_amount = escrow.amount;
            // convert to decimals
            let fee = (init_amount * (escrow.fee as u64)) / 100; // percentage fee for judgement
            let amount = init_amount - fee;
            
            transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                         from: escrow_token_account.to_account_info(),
                         to: treasury_token_account.to_account_info(),
                         authority: escrow.to_account_info()
                    },
                ),
                fee,
            )?;
            // handle decision
            if decision {
                transfer(
                    CpiContext::new(
                        ctx.accounts.token_program.to_account_info(),
                        Transfer {
                            from: escrow_token_account.to_account_info(),
                            to: payee_token_account.to_account_info(),
                            authority: escrow.to_account_info(),
                        },
                    ),
                    amount,
                )?;
                emit!(EscrowJudged {
                    address: escrow.key(),
                    winner: escrow.payee,
                    amount_awarded: amount,
                    fee_collected: fee,
                    token_mint: escrow.token_mint,
                    timestamp: Clock::get()?.unix_timestamp,
                });
            } else {
                transfer(
                    CpiContext::new(
                        ctx.accounts.token_program.to_account_info(),
                        Transfer {
                            from: escrow_token_account.to_account_info(),
                            to: payer_token_account.to_account_info(),
                            authority: escrow.to_account_info(),
                        },
                    ),
                    amount,
                )?;
                emit!(EscrowJudged {
                    address: escrow.key(),
                    winner: escrow.payer,
                    amount_awarded: amount,
                    fee_collected: fee,
                    token_mint: escrow.token_mint,
                    timestamp: Clock::get()?.unix_timestamp,
                });
            }
        } else {
            return Err(error!(ErrorCode::EscrowNotToken))
        };
        Ok(())
    }

    pub fn release_token_escrow(ctx: Context<ReleaseTokenContext>) -> Result<()> {
        let escrow = &ctx.accounts.escrow;
        // calculate the tax
        let init_amount = escrow.amount;
        let fee = (init_amount * (escrow.tax as u64)) / 10000; // basis point tax for being available for judgement
        let amount = init_amount - fee;
        let escrow_token_account = &mut ctx.accounts.escrow_token_account;
        let payee_token_account = &mut ctx.accounts.payee_token_account;
        let treasury_token_account = &mut ctx.accounts.treasury_token_account;
        transfer(CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: escrow_token_account.to_account_info(),
                to: treasury_token_account.to_account_info(),
                authority: escrow.to_account_info(),
            },
        ),
        fee,
        )?;
        transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: escrow_token_account.to_account_info(),
                    to: payee_token_account.to_account_info(),
                    authority: escrow.to_account_info(),
                },
            ),
            amount,
        )?;
        emit!(EscrowReleased {
            address: escrow.key(),
            amount: amount,
            tax_paid: fee,
            token_mint: escrow.token_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }
    pub fn return_token_escrow(ctx: Context<ReturnTokenContext>) -> Result<()> {
        let escrow = &ctx.accounts.escrow;
        // calculate the tax
        let init_amount = escrow.amount;
        let fee = (init_amount * (escrow.tax as u64)) / 10000; // basis point tax for being available for judgement
        let amount = init_amount - fee;
        let escrow_token_account = &mut ctx.accounts.escrow_token_account;
        let payer_token_account = &mut ctx.accounts.payer_token_account;
        let treasury_token_account = &mut ctx.accounts.treasury_token_account;
        transfer(CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: escrow_token_account.to_account_info(),
                to: treasury_token_account.to_account_info(),
                authority: escrow.to_account_info(),
            },
        ),
        fee,
        )?;
        transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: escrow_token_account.to_account_info(),
                    to: payer_token_account.to_account_info(),
                    authority: escrow.to_account_info(),
                },
            ),
            amount,
        )?;
        emit!(EscrowReturned {
            address: escrow.key(),
            amount: amount,
            tax_paid: fee,
            token_mint: escrow.token_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn recover_token_funds(ctx: Context<RecoverTokenContext>) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let escrow = &mut ctx.accounts.escrow;
        if now <= escrow.judge_deadline {
            return Err(error!(ErrorCode::RecoverTooEarly));
        }
        if let Some(_token_mint_pubkey) = escrow.token_mint {
            let payer_token_account = &mut ctx.accounts.payer_token_account;
            let escrow_token_account = &mut ctx.accounts.escrow_token_account;
            transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: escrow_token_account.to_account_info(),
                        to: payer_token_account.to_account_info(),
                        authority: escrow.to_account_info(),
                    },
                ),
                escrow.amount
            )?;
            emit!(EscrowRecovered {
                address: escrow.key(),
                amount: escrow.amount,
                token_mint: escrow.token_mint,
                timestamp: Clock::get()?.unix_timestamp,
            });
        }
        Ok(())
    }
}

//  ========================================================================================================  //
//  Account Contexts                                                                                          //
//    ▄████████  ▄████████  ▄████████     ███           ▄████████     ███     ▀████    ▐████▀    ▄████████    //
//    ███    ███ ███    ███ ███    ███ ▀█████████▄      ███    ███ ▀█████████▄   ███▌   ████▀    ███    ███   //
//    ███    ███ ███    █▀  ███    █▀     ▀███▀▀██      ███    █▀     ▀███▀▀██    ███  ▐███      ███    █▀    //
//    ███    ███ ███        ███            ███   ▀      ███            ███   ▀    ▀███▄███▀      ███          //
//  ▀███████████ ███        ███            ███          ███            ███        ████▀██▄     ▀███████████   //
//    ███    ███ ███    █▄  ███    █▄      ███          ███    █▄      ███       ▐███  ▀███             ███   //
//    ███    ███ ███    ███ ███    ███     ███          ███    ███     ███      ▄███     ███▄     ▄█    ███   //
//    ███    █▀  ████████▀  ████████▀     ▄████▀        ████████▀     ▄████▀   ████       ███▄  ▄████████▀    //
//  ========================================================================================================  //                                                               


                                                                                                      

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        mut,
        constraint = owner.key() == AUTHORIZED_LAUNCHER,
    )]
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

    #[account(
        mut,
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
        mut,
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = !escrow.token_mint.is_some() @ ErrorCode::EscrowNotSolana,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct DepositTokenContext<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, ConfigAccount>,

    #[account(
        mut,
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = escrow.token_mint.is_some() @ ErrorCode::EscrowNotToken,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    #[account(
        mut,
        constraint = escrow.token_mint == Some(mint_account.key()) @ ErrorCode::WrongToken,
    )]
    pub mint_account: Account<'info, Mint>,

    #[account(
        mut,
        constraint = payer_token_account.mint == mint_account.key(),
        constraint = payer_token_account.owner == payer.key(),
    )]
    pub payer_token_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_account,
        associated_token::authority = escrow,
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct RecoverTokenContext<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = escrow.token_mint.is_some() @ ErrorCode::EscrowNotToken,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    #[account(
        mut,
        constraint = escrow.token_mint == Some(mint_account.key()) @ ErrorCode::WrongToken,
    )]
    pub mint_account: Account<'info, Mint>,

    #[account(
        mut,
        constraint = payer_token_account.mint == mint_account.key(),
        constraint = payer_token_account.owner == payer.key(),
    )]
    pub payer_token_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_account,
        associated_token::authority = escrow,
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct ReleaseTokenContext<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: This is payee pubkey
    #[account(mut)]
    pub payee: AccountInfo<'info>,

    /// CHECK: This is treasury pubkey
    #[account(mut,
        constraint = config.treasury == treasury.key()
    )]
    pub treasury: AccountInfo<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump
    )]
    pub config: Account<'info, ConfigAccount>,

    #[account(
        mut,
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = escrow.token_mint.is_some() @ ErrorCode::EscrowNotToken,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    #[account(
        mut,
        constraint = escrow.token_mint == Some(mint_account.key()) @ ErrorCode::WrongToken,
    )]
    pub mint_account: Account<'info, Mint>,

    #[account(
        mut,
        constraint = payee_token_account.mint == mint_account.key(),
        constraint = payee_token_account.owner == payee.key(),
    )]
    pub payee_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = treasury_token_account.mint == mint_account.key(),
        constraint = treasury_token_account.owner == treasury.key(),
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_account,
        associated_token::authority = escrow,
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct ReturnTokenContext<'info> {
    #[account(mut)]
    pub payee: Signer<'info>,

    /// CHECK: This is payer pubkey
    #[account(
        mut,
        constraint = escrow.payer == payer.key() @ ErrorCode::UninvolvedUser,
    )]
    pub payer: AccountInfo<'info>,

    /// CHECK: This is treasury pubkey
    #[account(
        mut,
        constraint = config.treasury == treasury.key() @ ErrorCode::UninvolvedUser,
    )]
    pub treasury: AccountInfo<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.treasury == treasury.key() @ ErrorCode::UninvolvedUser,
    )]
    pub config: Account<'info, ConfigAccount>,

    #[account(
        mut,
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = escrow.token_mint.is_some() @ ErrorCode::EscrowNotToken,
        constraint = escrow.payer == payer.key() @ ErrorCode::UninvolvedUser,
        constraint = escrow.payee == payee.key() @ ErrorCode::UninvolvedUser,
        close = payer,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    #[account(
        mut,
        constraint = escrow.token_mint == Some(mint_account.key()) @ ErrorCode::WrongToken,
    )]
    pub mint_account: Account<'info, Mint>,

    #[account(
        mut,
        constraint = payer_token_account.mint == mint_account.key(),
        constraint = payer_token_account.owner == payee.key(),
    )]
    pub payer_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = treasury_token_account.mint == mint_account.key(),
        constraint = treasury_token_account.owner == treasury.key(),
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint_account,
        associated_token::authority = escrow,
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct RecoverSolanaContext<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"escrow", payer.key().as_ref()],
        bump = escrow.bump,
        constraint = !escrow.token_mint.is_some() @ ErrorCode::EscrowNotSolana,
        constraint = escrow.payer == payer.key() @ ErrorCode::NotPayerRecovering,
        close = payer,
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
    #[account(mut,
        constraint = config.treasury == treasury.key()
    )]
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
    #[account(
        mut, 
        constraint = config.treasury == treasury.key()
    )]
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

#[derive(Accounts)]
pub struct JudgeTokenContext<'info> {
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
        constraint = escrow.token_mint.is_some() @ ErrorCode::EscrowNotToken,
        constraint = escrow.disputed @ ErrorCode::EscrowNotDisputed,
        constraint = escrow.payee == payee.key() @ ErrorCode::UninvolvedUser,
        constraint = escrow.payer == payer.key() @ ErrorCode::UninvolvedUser,
        close = payer,
    )]
    pub escrow: Account<'info, EscrowAccount>,

    #[account(
        mut,
        constraint = escrow.token_mint == Some(mint_account.key()) @ ErrorCode::WrongToken,
    )]
    pub mint_account: Account<'info, Mint>,

    #[account(
        mut,
        constraint = payer_token_account.mint == mint_account.key(),
        constraint = payer_token_account.owner == payer.key(),
    )]
    pub payer_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = payee_token_account.mint == mint_account.key(),
        constraint = payee_token_account.owner == payee.key(),
    )]
    pub payee_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = treasury_token_account.mint == mint_account.key(),
        constraint = treasury_token_account.owner == treasury.key(),
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    #[account(
        associated_token::mint = mint_account,
        associated_token::authority = escrow,
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>
}
// ================================================================================================================================  //
// Arg Structs - Function Argument Definitions                                                                                       //
//   ▄████████    ▄████████    ▄██████▄          ▄████████     ███        ▄████████ ███    █▄   ▄████████     ███        ▄████████   // 
//   ███    ███   ███    ███   ███    ███        ███    ███ ▀█████████▄   ███    ███ ███    ███ ███    ███ ▀█████████▄   ███    ███  // 
//   ███    ███   ███    ███   ███    █▀         ███    █▀     ▀███▀▀██   ███    ███ ███    ███ ███    █▀     ▀███▀▀██   ███    █▀   // 
//   ███    ███  ▄███▄▄▄▄██▀  ▄███               ███            ███   ▀  ▄███▄▄▄▄██▀ ███    ███ ███            ███   ▀   ███         // 
// ▀███████████ ▀▀███▀▀▀▀▀   ▀▀███ ████▄       ▀███████████     ███     ▀▀███▀▀▀▀▀   ███    ███ ███            ███     ▀███████████  // 
//   ███    ███ ▀███████████   ███    ███               ███     ███     ▀███████████ ███    ███ ███    █▄      ███              ███  // 
//   ███    ███   ███    ███   ███    ███         ▄█    ███     ███       ███    ███ ███    ███ ███    ███     ███        ▄█    ███  // 
//   ███    █▀    ███    ███   ████████▀        ▄████████▀     ▄████▀     ███    ███ ████████▀  ████████▀     ▄████▀    ▄████████▀   // 
//                ███    ███                                              ███    ███                                                 // 
// ================================================================================================================================  //

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

// ========================================================================================================== //
// Account Definitions                                                                                        //
//   ▄████████  ▄████████  ▄████████     ███          ████████▄     ▄████████    ▄████████    ▄████████       //
//   ███    ███ ███    ███ ███    ███ ▀█████████▄      ███   ▀███   ███    ███   ███    ███   ███    ███      //
//   ███    ███ ███    █▀  ███    █▀     ▀███▀▀██      ███    ███   ███    █▀    ███    █▀    ███    █▀       //
//   ███    ███ ███        ███            ███   ▀      ███    ███  ▄███▄▄▄      ▄███▄▄▄       ███             //
// ▀███████████ ███        ███            ███          ███    ███ ▀▀███▀▀▀     ▀▀███▀▀▀     ▀███████████      //
//   ███    ███ ███    █▄  ███    █▄      ███          ███    ███   ███    █▄    ███                 ███      //
//   ███    ███ ███    ███ ███    ███     ███          ███   ▄███   ███    ███   ███           ▄█    ███      //
//   ███    █▀  ████████▀  ████████▀     ▄████▀        ████████▀    ██████████   ███         ▄████████▀       //
// ========================================================================================================== //     


                                                                                                    

#[account]
#[derive(InitSpace)]
pub struct ConfigAccount {
    pub judge: Pubkey,
    pub treasury: Pubkey,
    pub pending_judge: Option<Pubkey>,
    pub tax: u16, // BPS Fee for all future transactions
    pub fee: u8, // Incentivize the DAO to rule on escrows
    pub bump: u8, // Store the bump for verification later
}

#[account]
#[derive(InitSpace)]
pub struct EscrowAccount {
    pub payer: Pubkey,              // The person depositing funds
    pub payee: Pubkey,              // The recipient who should receive funds
    pub amount: u64,                // Amount held in escrow
    pub tax: u16,                   // the tax at time of escrow creation, ie the tax amount Payer and Payee agreed to when escrow was created. BPS.
    pub fee: u8,                    // the fee at time of escrow creation, ie the fee amount Payer and Payee agreed to when escrow was created. Percentage.
    pub token_mint: Option<Pubkey>, // If None, this is a SOL escrow, otherwise an SPL token
    pub disputed: bool,             
    pub deadline: i64,              // judge has to wait til after this time to raise a dispute
    pub judge_deadline: i64,
    pub creation_time: i64,         // When escrow was created (unix timestamp)
    pub bump: u8,                   // Bump for PDA verification
}

// ========================================================================= //
// Events                                                                    //
//   ▄████████   ▄█    █▄     ▄████████ ███▄▄▄▄       ███        ▄████████   //
//   ███    ███ ███    ███   ███    ███ ███▀▀▀██▄ ▀█████████▄   ███    ███   //
//   ███    █▀  ███    ███   ███    █▀  ███   ███    ▀███▀▀██   ███    █▀    //
//  ▄███▄▄▄     ███    ███  ▄███▄▄▄     ███   ███     ███   ▀   ███          //
// ▀▀███▀▀▀     ███    ███ ▀▀███▀▀▀     ███   ███     ███     ▀███████████   //
//   ███    █▄  ███    ███   ███    █▄  ███   ███     ███              ███   //
//   ███    ███ ███    ███   ███    ███ ███   ███     ███        ▄█    ███   //
//   ██████████  ▀██████▀    ██████████  ▀█   █▀     ▄████▀    ▄████████▀    //
// ========================================================================  // 
                                                      
#[event]
pub struct ConfigCreated {
    pub address: Pubkey,
    pub treasury: Pubkey,
    pub judge: Pubkey,
    pub tax: u16,
    pub fee: u8,
    pub timestamp: i64,
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

#[event]
pub struct EscrowDeposited {
    pub address: Pubkey,
    pub amount: u64,
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,
}

#[event]
pub struct EscrowReleased {
    pub address: Pubkey,
    pub amount: u64,
    pub tax_paid: u64,
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,
}

#[event]
pub struct EscrowReturned {
    pub address: Pubkey,
    pub amount: u64,
    pub tax_paid: u64,
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,
}

#[event]
pub struct EscrowRecovered {
    pub address: Pubkey,
    pub amount: u64,
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,
}

#[event]
pub struct EscrowDisputed {
    pub address: Pubkey,
    pub payer: Pubkey,
    pub payee: Pubkey,
    pub disputed_by: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct EscrowJudged {
    pub address: Pubkey,
    pub winner: Pubkey,
    pub amount_awarded: u64,
    pub fee_collected: u64,
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,
}

//  ==========================================================================  //
//  Error Codes / Errors                                                        //  
//   ▄████████    ▄████████    ▄████████  ▄██████▄     ▄████████    ▄████████   //
//   ███    ███   ███    ███   ███    ███ ███    ███   ███    ███   ███    ███  // 
//   ███    █▀    ███    ███   ███    ███ ███    ███   ███    ███   ███    █▀   // 
//  ▄███▄▄▄      ▄███▄▄▄▄██▀  ▄███▄▄▄▄██▀ ███    ███  ▄███▄▄▄▄██▀   ███         // 
// ▀▀███▀▀▀     ▀▀███▀▀▀▀▀   ▀▀███▀▀▀▀▀   ███    ███ ▀▀███▀▀▀▀▀   ▀███████████  // 
//   ███    █▄  ▀███████████ ▀███████████ ███    ███ ▀███████████          ███  // 
//   ███    ███   ███    ███   ███    ███ ███    ███   ███    ███    ▄█    ███  // 
//   ██████████   ███    ███   ███    ███  ▀██████▀    ███    ███  ▄████████▀   // 
//                ███    ███   ███    ███              ███    ███               //  
//  ==========================================================================  //

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

    #[msg("Operation Failed - Escrow not in recoverable state, wait for judge deadline to pass.")]
    RecoverTooEarly,

    #[msg("Escrow creation failed - amount is too small")]
    InvalidEscrowAmount,

    #[msg("Operation failed - trying to perform SOL operations on Token escrow")]
    EscrowNotSolana,

    #[msg("Operation failed - trying to perform Token operations on SOL escrow")]
    EscrowNotToken,

    #[msg("Operation failed - trying to use the wrong token")]
    WrongToken,

    #[msg("Operation failed - trying to release funds from wrong Payer")]
    NotPayerReleasing,

    #[msg("Operation failed - trying to recover funds when not Payer")]
    NotPayerRecovering,

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