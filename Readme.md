# Delion | Hyperbore Escrow on Solana

Holy good god damn is writing for Solana a headache. WIP.

## Current Features

```rust
pub fn initialize(ctx: Context<Initialize>, tax: u16, fee: u8) -> Result<()> {}
```

`initialize` creates a Configuration Account that stores cross-escrow details like the current judge, the treasury address, and fee information.

```rust
pub fn update_config(ctx: Context<UpdateContext>, updates: ConfigUpdateArgs) -> Result<()> {}
```

`update_config` is callable by the currently designated `judge` - updates args include optional new treasury (pubkey), new judge (pubkey), new tax (u16, basis points for a tax on each created escrow contract), new fee (u8, percentage points for a tax on each `judged` escrow).

```rust
pub fn accept_judge_seat(ctx: Context<AcceptJudgeSeatContext>) -> Result<()> {}
```

To prove that new judges are real (and the pubkey for such an important role hasn't been fatfingered), new judges must be designated via `update_config` and consequently `accept` the seat by calling this function.

```rust
pub fn create_escrow(ctx: Context<CreateEscrowContext>, args: EscrowCreationArgs) -> Result<()> {}
```

Creates a new `Escrow` account that stores intra-escrow details like the payer pubkey, payee pubkey, how much to store, and an optional `args.token_mint` pubkey for indicating a Token escrow (ie USDC). When called, the caller is automatically designated as the `payer`, a role that has special provisions per `Escrow`. Deadlines are deduced automatically, giving 2 weeks for the `Escrow` to close naturally and another 2 weeks for the `Judge` (`config.judge`) to judge the escrow.

TODO: After 4 weeks, funds should be recoverable by payer for zero fees (because the `Judge` didn't do their job).
TODO: Allow different times, range 1 week per phase to 4 weeks per phase. Phase lengths should be identical.

```rust
pub fn dispute_escrow(ctx: Context<DisputeEscrowContext>) -> Result<()> {}
```

Instantly disputes an escrow, essentially asking the `Judge` to resolve an off-chain dispute. Needs some tender love and care still. Callable by the Judge (`config.judge`) if the escrow is over two weeks old.
_Should be_ uncallable by payer and payee until the escrow is two weeks old. Flips a bool. Chill lil fn.

```rust
pub fn judge_sol_escrow(ctx: Context<JudgeSolanaContext>, decision: bool) -> Result<()> {}
```

`Escrows` are separated into `SOL` and `Token` types. This function is for `SOL` types - if an escrow is disputed, (`escrow.disputed = true`), the `Judge` (`config.judge`) is able to call this function, supplying a boolean, to release the funds to either the `Payer` (depositor of funds, `escrow.payer`) using `false`, or the `Payee` (intended recipient of funds, `escrow.payee`) using `true`.
Just answer the question: "Should Payee be paid?" and this will make sense.

TODO: `judge_token_escrow`

```rust
pub fn deposit_sol_funds(ctx: Context<DepositSolanaContext>) -> Result<()> {}
```

`Payer` (`escrow.payer`) must deposit their funds after creating their escrow. The amount they pay must match the amount they indicated in `create_escrow`, plus a little something for the rent. So weird how programs are charged rent.

```rust
pub fn release_sol_funds(ctx: Context<ReleaseSolanaContext>) -> Result<()> {}
```

The happy ending for a SOL escrow. Everything happens correctly off-chain, and the `Payer` (`escrow.payer`) releases funds to the `Payee` (`escrow.payee`) in the allotted time. The `Escrow` account is closed, HyperboreDAO collects some basis point fee from the total protected, and the `Payer` gets their rent back.

```rust
pub fn return_sol_funds(ctx: Context<ReturnSolanaContext>) -> Result<()> {}
```

The slightly less happy but still chill ending for a SOL escrow. The `Payee` (`escrow.payee`) realizes that, for whatever reason, they don't deserve to get paid, and voluntarily **return** the SOL to the `Payer`. The `Escrow` account is closed, HyperboreDAO collects some basis point fee from the total protected, and the `Payer` gets their rent back.

```rust
pub fn recover_sol_funds(ctx: Context<RecoverSolanaContext>) -> Result<()> {}
```

The worst possible ending for a SOL escrow. Money was deposited, then everyone got hit by a bus and portal isekai'd to Hyperborea to enjoy their anime harem. Nobody ever clicked anything on the escrow for over a month. Future Archaelogists can recover funds from the Escrow by using the `Payer`'s account after the `judge_deadline` has past.

```rust
pub fn deposit_token_funds(ctx: Context<DepositTokenContext>) -> Result<()> {}
```

Same as `deposit_sol_funds` but cooler. Works with standard tokens like USDC or Fartcoin or whatever.

```rust
pub fn judge_token_escrow(ctx: Context<JudgeTokenContext>, decision: bool) -> Result<()> {}
```

Same as `judge_sol_escrow` but cooler. Works with standard tokens like USDC or Fartcoin or whatever.

```rust
pub fn release_token_escrow(ctx: Context<ReleaseTokenContext>) -> Result<()> {}
```

Like the sol version, this is the happy path.

```rust
pub fn return_token_escrow(ctx: Context<ReturnTokenContext>) -> Result<()> {}
```

Slightly less happy path.

```rust
pub fn recover_token_funds(ctx: Context<RecoverSolanaContext>) -> Result<()> {}
```

The worst possible ending for a Token escrow. Money was deposited, then everyone got hit by a bus and portal isekai'd to Hyperborea to enjoy their anime harem. Nobody ever clicked anything on the escrow for over a month. Future Archaelogists can recover funds from the Escrow by using the `Payer`'s account after the `judge_deadline` has past.

## Events

There are emissions for tracking things like pubkeys, fee changes, and accumulating stats (hopefully one day we can brag about how much off-chain ecomony was protected with this thing).

### Config Created

```rust
pub struct ConfigCreated {
    pub address: Pubkey, // the pubkey of the new Config PDA
    pub treasury: Pubkey, // the pubkey of the treasury that receives taxes
    pub judge: Pubkey, // the pubkey of the person/multisig that can judge escrows
    pub tax: u16, // basis point fee per escrow
    pub fee: u8, // percent fee for judgements
    pub timestamp: i64, // when the shit happened lol fym document the timestamp
}
```

### Config Updated

```rust
#[event]
pub struct ConfigUpdated {
    pub address: Pubkey, // the pubkey of the config pda being updated
    pub treasury: Pubkey, // the treasury
    pub pending_judge: Option<Pubkey>, // if this is set, it means the judge role is changing
    pub tax: u16, // basis point fee per escrow
    pub fee: u8, // percent fee for judgements
    pub timestamp: i64,
}
```

#### Judge Stuff happens here too

```rust
#[event]
pub struct JudgeNominated {
    pub address: Pubkey, // the pubkey of the config pda in question
    pub pending_judge: Pubkey, // if this is set, there is definitely being a new judge
    pub timestamp: i64,
}

#[event]
pub struct JudgeAccepted {
    pub address: Pubkey, // the pubkey of the config where this happened
    pub old_judge: Pubkey, // cya bozo
    pub new_judge: Pubkey, // welcome king
    pub timestamp: i64,
}
```

#### Escrow Created

```rust
pub struct EscrowCreated {
    pub address: Pubkey, // pubkey of the new escrow (need this for every other function later)
    pub payer: Pubkey, // the pubkey of the Payer role for the escrow. this is the person or multisig putting up coin
    pub payee: Pubkey, // the pubkey of the Payee role for the escrow. this is the person or multisig expecting coin
    pub amount: u64, // how many coins, in the smallest unit possible. Ie 5,000,000 for 5 USDC, 5,000,000,000 for 5 SOL.
    pub token_mint: Option<Pubkey>, // if this is set, it's the pubkey of the mint account for the token. If not, this is a SOL contract.
    pub timestamp: i64,
}
```

#### Escrow Deposited

```rust
pub struct EscrowDeposited {
    pub address: Pubkey, // pubkey of the escrow where the deposit happened
    pub amount: u64, // how many coins got entered
    pub token_mint: Option<Pubkey>, // if this is set, it's the pubkey of the mint account for the token. If not, this is a SOL contract.
    pub timestamp: i64,
}
```

#### Escrow Released

```rust
// (accrete these signals for dashboarding, like "Over $10bn protected" but live)
pub struct EscrowReleased { // released means that Payer gave the money to Payee.
    pub address: Pubkey, // pubkey of the escrow that just completed (it's gone forever now)
    pub amount: u64, // how much money got paid out
    pub tax_paid: u64, // how much tax was paid out
    pub token_mint: Option<Pubkey>, // needed for computing dashboarding
    pub timestamp: i64,
}
```

#### Escrow Returned

```rust
// (accrete these signals for dashboarding, like "Over $10bn protected" but live)
// values identical to EscrowReleased, but
pub struct EscrowReturned { // Returned means Payee gave the money *back* to Payer.
    pub address: Pubkey,
    pub amount: u64,
    pub tax_paid: u64,
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,
}
```

#### Escrow Recovered

```rust
// (accrete these signals for dashboarding, like "Over $10bn protected" but live)
// values identical to EscrowReleased, but
pub struct EscrowRecovered { // recovered means Payer took their money back. Time locked function.
    pub address: Pubkey,
    pub amount: u64, // notice no tax_paid field. DAO is not paid for not judging on escrows.
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,
}
```

#### Escrow Disputed

```rust
// the deal has broken, time to do hard work.
// put these signals on dings if you are doing escrow services.
pub struct EscrowDisputed {
    pub address: Pubkey, // the pubkey of the escrow that needs judgement
    pub payer: Pubkey,
    pub payee: Pubkey,
    pub disputed_by: Pubkey, // the person who disputed. could be payer, payee, or judge.
    pub timestamp: i64,
}
```

#### Escrow Judged

```rust
pub struct EscrowJudged {
    pub address: Pubkey, // the escrow that was judged
    pub winner: Pubkey, // the person who got paid
    pub amount_awarded: u64, // how much they got
    pub fee_collected: u64, // how much the judge took
    pub token_mint: Option<Pubkey>,
    pub timestamp: i64,
}
```

## License

This project is licensed under the MIT License. Please remix it for your own needs and make beautiful, co-operative things.

## Sponsorship

This project is sponsored by [HyperBoreDAO](https://www.hyperboredao.ai/)
