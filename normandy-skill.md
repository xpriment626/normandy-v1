# Normandy Agent Skill

> Operational guide for AI agents interacting with the Normandy undercollateralized lending protocol on Solana.

## Table of Contents

1. [Protocol Overview](#1-protocol-overview)
2. [Network & Program IDs](#2-network--program-ids)
3. [Account Model & PDA Derivation](#3-account-model--pda-derivation)
4. [Pool Discovery](#4-pool-discovery)
5. [Instruction Set](#5-instruction-set)
6. [Decision Framework](#6-decision-framework)
7. [Key Constants](#7-key-constants)

---

## 1. Protocol Overview

Normandy is an undercollateralized lending protocol where AI agents borrow against reputation, not collateral.

- **Humans are lenders.** They create pools, deposit capital, earn yield.
- **Agents are borrowers.** They borrow from pools by proving reputation (e.g., positive trading PnL).
- **Credit decisions are modular.** Each pool points to a hook program that decides who can borrow and on what terms. The hook is a separate on-chain program invoked via CPI during borrow.
- **The protocol is an impartial intermediary.** It enforces accounting, reserve ratios, and fee collection. It does not make credit decisions itself (Wildcat model).

No MCP server required. Agents construct and submit transactions directly via Solana RPC.

---

## 2. Network & Program IDs

| Property | Value |
|----------|-------|
| Network | Solana devnet (testing), mainnet-beta (production) |
| normandy-core | `3kXtyEqYxGTTnUtCpVNVwNwQjRZPYfGkEZo75tQtdwLs` |
| normandy-hook-fixed-term | `He2SZJXMwPnyjN3dfuV8VEU2TPU58oR1HSWFkYvUgnNC` |

```typescript
import { PublicKey } from "@solana/web3.js";

const NORMANDY_CORE_PROGRAM_ID = new PublicKey("3kXtyEqYxGTTnUtCpVNVwNwQjRZPYfGkEZo75tQtdwLs");
const NORMANDY_HOOK_FIXED_TERM_PROGRAM_ID = new PublicKey("He2SZJXMwPnyjN3dfuV8VEU2TPU58oR1HSWFkYvUgnNC");
```

---

## 3. Account Model & PDA Derivation

| Account | Seeds | Program |
|---------|-------|---------|
| ProtocolConfig | `["protocol_config"]` | normandy-core |
| Pool | `["pool", authority, pool_id_le_bytes]` | normandy-core |
| LenderPosition | `["lender", pool, lender]` | normandy-core |
| BorrowerPosition | `["borrower", pool, agent]` | normandy-core |
| HookConfig | `["hook_config", pool]` | normandy-hook-fixed-term |
| Vault | SPL Token account, authority = Pool PDA | (token program) |

### PDA Derivation Examples

```typescript
import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";

// Pool PDA
const [poolPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("pool"), authority.toBuffer(), new BN(poolId).toArrayLike(Buffer, "le", 8)],
  NORMANDY_CORE_PROGRAM_ID
);

// LenderPosition PDA
const [lenderPositionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("lender"), poolPda.toBuffer(), lender.toBuffer()],
  NORMANDY_CORE_PROGRAM_ID
);

// BorrowerPosition PDA
const [borrowerPositionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("borrower"), poolPda.toBuffer(), agent.toBuffer()],
  NORMANDY_CORE_PROGRAM_ID
);

// ProtocolConfig PDA
const [protocolConfigPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("protocol_config")],
  NORMANDY_CORE_PROGRAM_ID
);

// HookConfig PDA
const [hookConfigPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("hook_config"), poolPda.toBuffer()],
  NORMANDY_HOOK_FIXED_TERM_PROGRAM_ID
);
```

### Account Data Layouts

**Pool** (213 bytes including 8-byte discriminator):
| Field | Type | Description |
|-------|------|-------------|
| authority | Pubkey | Pool creator |
| underlying_mint | Pubkey | Token mint (e.g., USDC) |
| hook_program | Pubkey | Credit-decision hook program |
| vault | Pubkey | SPL token account holding pool assets |
| scale_factor | u128 | Ray-denominated (1e27), starts at RAY |
| total_interest_earned | u64 | Aggregate lender interest |
| last_accrual_timestamp | i64 | Last time interest was accrued |
| min_interest_bips | u16 | Min annual rate in basis points |
| max_interest_bips | u16 | Max annual rate in basis points |
| min_term_seconds | i64 | Min loan term |
| max_term_seconds | i64 | Max loan term |
| reserve_ratio_bips | u16 | % of deposits that must stay liquid |
| total_deposits | u64 | Aggregate lender deposits (nominal) |
| total_borrows | u64 | Aggregate outstanding borrows (nominal) |
| accrued_protocol_fees | u64 | Unclaimed protocol fees |
| position_mode | u8 | 0 = PDA positions |
| deposit_window_end | i64 | 0 = always open |
| is_closed | bool | Whether pool accepts new activity |
| pool_id | u64 | Unique ID within authority namespace |
| bump | u8 | PDA bump seed |

**BorrowerPosition** (148 bytes including discriminator):
| Field | Type | Description |
|-------|------|-------------|
| pool | Pubkey | Pool this position belongs to |
| agent | Pubkey | Borrowing agent |
| principal | u64 | Original borrow amount |
| scaled_borrow | u64 | Reserved for post-MVP |
| annual_interest_bips | u16 | Rate set by hook |
| term_seconds | i64 | Term set by hook |
| accrued_interest | u64 | Interest accumulated |
| borrow_scale_factor | u128 | Scale factor at borrow time |
| borrowed_at | i64 | Borrow timestamp |
| maturity_timestamp | i64 | borrowed_at + term_seconds |
| last_accrual_timestamp | i64 | Last interest computation |
| status | u8 | 0 = Active, 1 = Repaid |
| bump | u8 | PDA bump seed |

**LenderPosition** (105 bytes including discriminator):
| Field | Type | Description |
|-------|------|-------------|
| pool | Pubkey | Pool this position belongs to |
| lender | Pubkey | Depositor |
| total_deposited | u64 | Cumulative nominal deposit |
| scaled_deposit | u64 | Deposit in scaled units |
| entry_scale_factor | u128 | Scale factor at most recent deposit |
| deposited_at | i64 | Timestamp of last deposit |
| bump | u8 | PDA bump seed |

**HookConfig** (50 bytes including discriminator):
| Field | Type | Description |
|-------|------|-------------|
| pool | Pubkey | Associated pool |
| max_borrow_per_agent | u64 | Per-agent borrow cap |
| require_pnl_positive | bool | Whether positive PnL is required |
| bump | u8 | PDA bump seed |

---

## 4. Pool Discovery

Use `getProgramAccounts` to find available pools. The Pool account discriminator is the first 8 bytes of account data (Anchor discriminator = first 8 bytes of SHA256("account:Pool")).

```typescript
import { Connection, PublicKey } from "@solana/web3.js";
import { BorshCoder, Idl } from "@coral-xyz/anchor";

const connection = new Connection("https://api.devnet.solana.com");

// Fetch all Pool accounts
const pools = await connection.getProgramAccounts(NORMANDY_CORE_PROGRAM_ID, {
  filters: [
    { dataSize: 213 }, // Pool::SIZE
    // Optional: filter by underlying_mint at offset 40 (8 disc + 32 authority)
    // { memcmp: { offset: 40, bytes: usdcMint.toBase58() } },
  ],
});
```

### Evaluating a Pool

For each pool, decode the account data and check:

1. **is_closed** -- skip closed pools
2. **underlying_mint** -- matches the token you want to borrow
3. **min_interest_bips / max_interest_bips** -- the annual rate (MVP: these are equal, fixed rate)
4. **min_term_seconds / max_term_seconds** -- the loan term (MVP: these are equal, fixed term)
5. **Available liquidity** -- can the pool fund your borrow?

### Computing Available Liquidity

```typescript
// Read the vault's token balance
const vaultInfo = await connection.getTokenAccountBalance(pool.vault);
const vaultBalance = Number(vaultInfo.value.amount);

// Required reserves
const requiredReserves = Math.floor(
  (pool.totalDeposits * pool.reserveRatioBips) / 10000
);

// Available to borrow
const available = vaultBalance - requiredReserves - pool.accruedProtocolFees;
```

### Reading HookConfig for Borrow Limits

```typescript
const [hookConfigPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("hook_config"), poolPda.toBuffer()],
  NORMANDY_HOOK_FIXED_TERM_PROGRAM_ID
);

const hookConfigAccount = await program.account.hookConfig.fetch(hookConfigPda);
// hookConfigAccount.maxBorrowPerAgent — per-agent cap
// hookConfigAccount.requirePnlPositive — whether positive PnL proof is needed
```

---

## 5. Instruction Set

All examples use `@coral-xyz/anchor`. Load the IDL and create the program instance:

```typescript
import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorProvider } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, getAssociatedTokenAddress } from "@solana/spl-token";
import BN from "bn.js";

// Load IDL (generate from anchor build or fetch from chain)
import idl from "./target/idl/normandy_core.json";

const provider = AnchorProvider.env();
const program = new Program(idl, NORMANDY_CORE_PROGRAM_ID, provider);
```

---

### 5a. initialize_protocol

One-time setup by protocol deployer. Creates the global ProtocolConfig singleton.

**Arguments:**
| Name | Type | Description |
|------|------|-------------|
| fee_recipient | Pubkey | Where protocol fees are claimed to |

**Accounts:**
| Account | Type | Signer | Mutable | Notes |
|---------|------|--------|---------|-------|
| protocol_config | ProtocolConfig | No | Yes | PDA, init. Seeds: `["protocol_config"]` |
| authority | Signer | Yes | Yes | Deployer, pays rent |
| system_program | Program | No | No | |

```typescript
const [protocolConfigPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("protocol_config")],
  NORMANDY_CORE_PROGRAM_ID
);

await program.methods
  .initializeProtocol(feeRecipientPubkey)
  .accounts({
    protocolConfig: protocolConfigPda,
    authority: deployer.publicKey,
    systemProgram: SystemProgram.programId,
  })
  .signers([deployer])
  .rpc();
```

---

### 5b. initialize_pool

Lender creates a new lending pool. Also CPI-creates the HookConfig on the hook program.

**Arguments:**
| Name | Type | Description |
|------|------|-------------|
| pool_id | u64 | Unique pool identifier (within authority namespace) |
| interest_bips | u16 | Annual interest rate in basis points (MVP: fixed, sets both min and max) |
| term_seconds | i64 | Loan term in seconds (MVP: fixed, sets both min and max) |
| reserve_ratio_bips | u16 | % of deposits that must stay liquid (in bips) |
| position_mode | u8 | 0 = PDA-based positions |
| deposit_window_end | i64 | Unix timestamp after which deposits stop (0 = always open) |
| max_borrow_per_agent | u64 | Per-agent borrow cap (passed to hook via CPI) |
| require_pnl_positive | bool | Whether hook requires positive PnL (passed to hook via CPI) |

**Accounts:**
| Account | Type | Signer | Mutable | Notes |
|---------|------|--------|---------|-------|
| pool | Pool | No | Yes | PDA, init. Seeds: `["pool", authority, pool_id_le_bytes]` |
| vault | TokenAccount | No | Yes | Init, token authority = pool PDA |
| underlying_mint | Mint | No | No | Token mint to lend |
| hook_program | UncheckedAccount | No | No | Credit-decision hook program |
| hook_config | UncheckedAccount | No | Yes | HookConfig PDA created by hook via CPI |
| authority | Signer | Yes | Yes | Pool creator, pays rent |
| token_program | Program | No | No | SPL Token |
| system_program | Program | No | No | |
| rent | Sysvar | No | No | |

```typescript
const poolId = new BN(1);
const [poolPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("pool"), authority.publicKey.toBuffer(), poolId.toArrayLike(Buffer, "le", 8)],
  NORMANDY_CORE_PROGRAM_ID
);

const [hookConfigPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("hook_config"), poolPda.toBuffer()],
  NORMANDY_HOOK_FIXED_TERM_PROGRAM_ID
);

// Generate a keypair for the vault token account
const vault = Keypair.generate();

await program.methods
  .initializePool(
    poolId,                   // pool_id
    500,                      // interest_bips (5% annual)
    new BN(2592000),          // term_seconds (30 days)
    1000,                     // reserve_ratio_bips (10%)
    0,                        // position_mode (PDA)
    new BN(0),                // deposit_window_end (always open)
    new BN(1_000_000_000),    // max_borrow_per_agent (1000 USDC at 6 decimals)
    true,                     // require_pnl_positive
  )
  .accounts({
    pool: poolPda,
    vault: vault.publicKey,
    underlyingMint: usdcMint,
    hookProgram: NORMANDY_HOOK_FIXED_TERM_PROGRAM_ID,
    hookConfig: hookConfigPda,
    authority: authority.publicKey,
    tokenProgram: TOKEN_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
    rent: anchor.web3.SYSVAR_RENT_PUBKEY,
  })
  .signers([authority, vault])
  .rpc();
```

---

### 5c. deposit

Lender adds capital to a pool. Creates a LenderPosition on first deposit (init_if_needed). Supports multiple deposits to the same pool.

**Arguments:**
| Name | Type | Description |
|------|------|-------------|
| amount | u64 | Token amount to deposit (in mint's native units) |

**Accounts:**
| Account | Type | Signer | Mutable | Notes |
|---------|------|--------|---------|-------|
| pool | Pool | No | Yes | Must match vault via has_one |
| lender_position | LenderPosition | No | Yes | PDA, init_if_needed. Seeds: `["lender", pool, lender]` |
| vault | TokenAccount | No | Yes | Pool's vault |
| lender_token_account | TokenAccount | No | Yes | Lender's token account for underlying_mint |
| lender | Signer | Yes | Yes | Depositor, pays rent on first deposit |
| token_program | Program | No | No | SPL Token |
| system_program | Program | No | No | |

**Constraints:**
- Pool must not be closed
- If deposit_window_end > 0, current time must be before it

```typescript
const [lenderPositionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("lender"), poolPda.toBuffer(), lender.publicKey.toBuffer()],
  NORMANDY_CORE_PROGRAM_ID
);

const lenderTokenAccount = await getAssociatedTokenAddress(usdcMint, lender.publicKey);

await program.methods
  .deposit(new BN(5_000_000_000)) // 5000 USDC
  .accounts({
    pool: poolPda,
    lenderPosition: lenderPositionPda,
    vault: vaultPubkey,
    lenderTokenAccount: lenderTokenAccount,
    lender: lender.publicKey,
    tokenProgram: TOKEN_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
  })
  .signers([lender])
  .rpc();
```

---

### 5d. borrow

Agent borrows from a pool. Requires a reputation proof. The pool CPI-calls the hook program to get a credit decision. Creates a BorrowerPosition (one per pool per agent).

**Arguments:**
| Name | Type | Description |
|------|------|-------------|
| amount | u64 | Token amount to borrow |
| reputation_proof | Vec\<u8\> | Opaque bytes passed to the hook. For fixed-term hook: 16 bytes (i64 PnL + i64 timestamp) |

**Accounts:**
| Account | Type | Signer | Mutable | Notes |
|---------|------|--------|---------|-------|
| pool | Pool | No | Yes | Must match vault via has_one |
| borrower_position | BorrowerPosition | No | Yes | PDA, init. Seeds: `["borrower", pool, agent]` |
| vault | TokenAccount | No | Yes | Pool's vault |
| agent_token_account | TokenAccount | No | Yes | Agent's token account to receive borrowed funds |
| agent | Signer | Yes | Yes | Borrowing agent, pays rent |
| hook_program | UncheckedAccount | No | No | Must match pool.hook_program |
| hook_config | UncheckedAccount | No | No | HookConfig PDA for this pool |
| token_program | Program | No | No | SPL Token |
| system_program | Program | No | No | |

**Constraints:**
- Amount must not exceed available liquidity (vault balance - required reserves - accrued fees)
- Hook program must match pool.hook_program
- Hook must approve the borrow (returns OnBorrowResult with approved = true)
- One active position per agent per pool

### Constructing the Reputation Proof

The fixed-term hook expects 16 bytes: `i64 pnl` (little-endian) followed by `i64 timestamp` (little-endian).

```typescript
// Construct reputation proof for the fixed-term hook
const proofBuffer = Buffer.alloc(16);
proofBuffer.writeBigInt64LE(BigInt(pnl), 0);       // Agent's realized PnL (must be > 0 if require_pnl_positive)
proofBuffer.writeBigInt64LE(BigInt(timestamp), 8);  // Proof timestamp
const reputationProof = Array.from(proofBuffer);
```

### Full Borrow Example

```typescript
const [borrowerPositionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("borrower"), poolPda.toBuffer(), agent.publicKey.toBuffer()],
  NORMANDY_CORE_PROGRAM_ID
);

const [hookConfigPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("hook_config"), poolPda.toBuffer()],
  NORMANDY_HOOK_FIXED_TERM_PROGRAM_ID
);

const agentTokenAccount = await getAssociatedTokenAddress(usdcMint, agent.publicKey);

// Build reputation proof
const proofBuffer = Buffer.alloc(16);
proofBuffer.writeBigInt64LE(BigInt(50_000_000), 0);  // +50 USDC PnL
proofBuffer.writeBigInt64LE(BigInt(Math.floor(Date.now() / 1000)), 8);
const reputationProof = Array.from(proofBuffer);

await program.methods
  .borrow(new BN(1_000_000_000), Buffer.from(reputationProof)) // 1000 USDC
  .accounts({
    pool: poolPda,
    borrowerPosition: borrowerPositionPda,
    vault: vaultPubkey,
    agentTokenAccount: agentTokenAccount,
    agent: agent.publicKey,
    hookProgram: NORMANDY_HOOK_FIXED_TERM_PROGRAM_ID,
    hookConfig: hookConfigPda,
    tokenProgram: TOKEN_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
  })
  .signers([agent])
  .rpc();
```

---

### 5e. repay

Repay a borrow in full. Computes accrued interest at repay time. Anyone can repay on behalf of an agent -- the `repayer` is the signer, the `agent` is the position owner.

**Arguments:** None (repays full principal + accrued interest)

**Accounts:**
| Account | Type | Signer | Mutable | Notes |
|---------|------|--------|---------|-------|
| pool | Pool | No | Yes | Must match vault via has_one |
| borrower_position | BorrowerPosition | No | Yes | PDA. Seeds: `["borrower", pool, agent]` |
| agent | UncheckedAccount | No | No | The agent whose position is being repaid (not necessarily signer) |
| vault | TokenAccount | No | Yes | Pool's vault |
| repayer_token_account | TokenAccount | No | Yes | Token account funding the repayment |
| repayer | Signer | Yes | Yes | Whoever is paying |
| token_program | Program | No | No | SPL Token |

**Constraints:**
- Position must be active (status = 0)
- Repays principal + all accrued interest in a single transfer

**Interest Calculation:**
```
interest = principal * annual_interest_bips * elapsed_seconds / (10000 * 31536000)
total_owed = principal + accrued_interest (previously accumulated) + interest (new since last accrual)
```

```typescript
const [borrowerPositionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("borrower"), poolPda.toBuffer(), agent.publicKey.toBuffer()],
  NORMANDY_CORE_PROGRAM_ID
);

const repayerTokenAccount = await getAssociatedTokenAddress(usdcMint, repayer.publicKey);

await program.methods
  .repay()
  .accounts({
    pool: poolPda,
    borrowerPosition: borrowerPositionPda,
    agent: agent.publicKey,
    vault: vaultPubkey,
    repayerTokenAccount: repayerTokenAccount,
    repayer: repayer.publicKey,
    tokenProgram: TOKEN_PROGRAM_ID,
  })
  .signers([repayer])
  .rpc();
```

---

### 5f. withdraw

Lender withdraws their full position (deposit + earned yield). The position account is closed and rent is returned to the lender.

**Arguments:** None (withdraws entire scaled position)

**Accounts:**
| Account | Type | Signer | Mutable | Notes |
|---------|------|--------|---------|-------|
| pool | Pool | No | Yes | Must match vault via has_one |
| lender_position | LenderPosition | No | Yes | PDA, closed after withdraw. Seeds: `["lender", pool, lender]` |
| vault | TokenAccount | No | Yes | Pool's vault |
| lender_token_account | TokenAccount | No | Yes | Lender's token account to receive funds |
| lender | Signer | Yes | Yes | Position owner |
| token_program | Program | No | No | SPL Token |

**Payout Calculation:**
```
payout = scaled_deposit * scale_factor / RAY
```
The scale_factor grows as interest accrues, so the lender receives deposit + proportional share of interest (minus protocol fee).

```typescript
const [lenderPositionPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("lender"), poolPda.toBuffer(), lender.publicKey.toBuffer()],
  NORMANDY_CORE_PROGRAM_ID
);

const lenderTokenAccount = await getAssociatedTokenAddress(usdcMint, lender.publicKey);

await program.methods
  .withdraw()
  .accounts({
    pool: poolPda,
    lenderPosition: lenderPositionPda,
    vault: vaultPubkey,
    lenderTokenAccount: lenderTokenAccount,
    lender: lender.publicKey,
    tokenProgram: TOKEN_PROGRAM_ID,
  })
  .signers([lender])
  .rpc();
```

---

### 5g. claim_protocol_fees

Protocol fee recipient claims accrued fees from a pool.

**Arguments:** None

**Accounts:**
| Account | Type | Signer | Mutable | Notes |
|---------|------|--------|---------|-------|
| protocol_config | ProtocolConfig | No | No | PDA. Seeds: `["protocol_config"]` |
| pool | Pool | No | Yes | Must match vault via has_one |
| vault | TokenAccount | No | Yes | Pool's vault |
| fee_recipient_token_account | TokenAccount | No | Yes | Fee recipient's token account |
| fee_recipient | Signer | Yes | Yes | Must match protocol_config.fee_recipient |
| token_program | Program | No | No | SPL Token |

```typescript
const [protocolConfigPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("protocol_config")],
  NORMANDY_CORE_PROGRAM_ID
);

const feeRecipientTokenAccount = await getAssociatedTokenAddress(usdcMint, feeRecipient.publicKey);

await program.methods
  .claimProtocolFees()
  .accounts({
    protocolConfig: protocolConfigPda,
    pool: poolPda,
    vault: vaultPubkey,
    feeRecipientTokenAccount: feeRecipientTokenAccount,
    feeRecipient: feeRecipient.publicKey,
    tokenProgram: TOKEN_PROGRAM_ID,
  })
  .signers([feeRecipient])
  .rpc();
```

---

## 6. Decision Framework

### For a Borrower Agent

**When to borrow:**
- You need capital for trading, operations, or strategy execution
- You have a positive PnL track record (required by most hooks)
- You can repay principal + interest before maturity

**How to choose a pool:**
1. Filter pools by `underlying_mint` (the token you need)
2. Skip pools where `is_closed = true`
3. Compute available liquidity: `vault_balance - (total_deposits * reserve_ratio_bips / 10000) - accrued_protocol_fees`
4. Check pool's `min_interest_bips` -- lower is cheaper
5. Check pool's `min_term_seconds` -- longer gives more time to repay
6. Read the HookConfig for `max_borrow_per_agent` -- confirms your desired amount is within the cap

**Risk considerations:**
- You **must** repay by maturity. There is no automatic liquidation -- failure to repay damages your on-chain reputation.
- Interest accrues continuously. The total owed at repayment = `principal + (principal * rate * elapsed / (10000 * 31536000))`
- One active position per pool. You cannot borrow again from the same pool until repaid.

### For a Lender (Human or Agent)

**When to create a pool:**
- You want to earn yield by lending to AI agents
- You have capital in an SPL token (e.g., USDC) that you want to put to work
- You accept undercollateralized credit risk in exchange for higher yield

**How to configure a pool:**
- `interest_bips`: Higher rate = more yield but fewer borrowers. 500 bips = 5% APR.
- `term_seconds`: Shorter terms reduce exposure. 2592000 = 30 days.
- `reserve_ratio_bips`: Higher ratio = more liquid reserves, less capital deployed. 1000 = 10%.
- `max_borrow_per_agent`: Caps per-agent exposure. Size based on your risk tolerance.
- `require_pnl_positive`: Keep `true` to only lend to profitable agents.
- `deposit_window_end`: Set to 0 for always-open deposits, or a timestamp to close deposits at a specific time.

**Risk considerations:**
- Undercollateralized lending means credit risk. If an agent defaults, the loss falls on depositors.
- Hook quality matters. The hook decides who borrows -- a permissive hook increases default risk.
- Reserve ratio protects liquidity for withdrawals but reduces capital efficiency.

---

## 7. Key Constants

```typescript
const RAY = BigInt("1000000000000000000000000000"); // 1e27 — scale factor base
const SECONDS_PER_YEAR = 31_536_000;
const BIP_DENOMINATOR = 10_000;
const PROTOCOL_FEE_BIPS = 1_000; // 10% of interest goes to protocol
```

### Interest Math

```
gross_interest = total_borrows * rate_bips * elapsed_seconds / (BIP_DENOMINATOR * SECONDS_PER_YEAR)
protocol_fee = gross_interest * PROTOCOL_FEE_BIPS / BIP_DENOMINATOR
lender_interest = gross_interest - protocol_fee
scale_factor = RAY + (total_interest_earned * RAY / total_deposits)
```

### Hook Discriminators

If building raw transactions without Anchor:
```typescript
// SHA256("global:initialize")[..8]
const HOOK_IX_INITIALIZE = [0xaf, 0xaf, 0x6d, 0x1f, 0x0d, 0x98, 0x9b, 0xed];

// SHA256("global:on_borrow")[..8]
const HOOK_IX_ON_BORROW = [0xda, 0xf8, 0xca, 0xe6, 0x16, 0x0c, 0x91, 0x5b];
```

### Error Codes

| Error | Meaning |
|-------|---------|
| DepositWindowClosed | Deposit window has ended |
| PoolClosed | Pool is closed |
| ReserveRatioBreached | Borrow would violate reserve requirements |
| BorrowRejected | Hook program denied the borrow |
| BorrowNotActive | Position is already repaid |
| InsufficientVaultBalance | Vault cannot cover the withdrawal |
| UnauthorizedFeeClaim | Signer is not the fee recipient |
| InvalidHookProgram | Hook program doesn't match pool config |
| InvalidHookReturnData | Hook returned malformed data |
