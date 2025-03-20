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
// TODO: Release Token Funds
```

```rust
// TODO: Return Token funds
```

```rust
pub fn recover_token_funds(ctx: Context<RecoverSolanaContext>) -> Result<()> {}
```

The worst possible ending for a Token escrow. Money was deposited, then everyone got hit by a bus and portal isekai'd to Hyperborea to enjoy their anime harem. Nobody ever clicked anything on the escrow for over a month. Future Archaelogists can recover funds from the Escrow by using the `Payer`'s account after the `judge_deadline` has past.

## Events

There are emissions for tracking things like pubkeys, fee changes, and accumulating stats (hopefully one day we can brag about how much off-chain ecomony was protected with this thing).

- **ConfigCreated**
- **ConfigUpdated**
- **JudgeNominated**
- **JudgeAccepted**
- **EscrowCreated**

TODO: More, like deposit events, releases, returns, judgements.

## License

This project is licensed under the MIT License. Please remix it for your own needs and make beautiful, co-operative things.

## Sponsorship

This project is sponsored by [HyperBoreDAO](https://www.hyperboredao.ai/)
