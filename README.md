# Normandy

Undercollateralized lending for AI agents on Solana.

## What is this

AI agents borrow against reputation — specifically, positive trading PnL — not collateral. Lenders create pools, deposit capital, and earn yield from agent borrowing activity. Credit decisions are modular: each pool points to a pluggable hook program that decides who can borrow and on what terms. The core protocol is an impartial intermediary; it enforces accounting, reserve ratios, and fee collection but makes no credit decisions itself.

Built on Solana with Anchor.

## Architecture

Two programs:

| Program | ID | Role |
|---|---|---|
| `normandy-core` | `3kXtyEqYxGTTnUtCpVNVwNwQjRZPYfGkEZo75tQtdwLs` | Pool state, lending logic, 7 instructions |
| `normandy-hook-fixed-term` | `He2SZJXMwPnyjN3dfuV8VEU2TPU58oR1HSWFkYvUgnNC` | Credit policy hook, 2 instructions |

The core program invokes hook programs via raw `invoke_signed` CPI using known instruction discriminators — it does not import the hook crate. This means anyone can deploy a custom hook (Anchor, Pinocchio, or raw) without modifying or recompiling core, as long as it implements the `on_borrow` discriminator (`0xdaf8cae6160c915b`).

The MVP hook (`normandy-hook-fixed-term`) checks that agent PnL > 0 and enforces a per-agent borrow cap.

## Lifecycle

```
lender creates pool → lender deposits → agent borrows (hook CPI checks credit) → agent repays → lender withdraws + yield
```

During `borrow`, the core program CPIs into the pool's hook program. The hook validates the `reputation_proof` payload (off-chain signed PnL attestation) and either approves or rejects. Core handles the token transfer and accounting.

## Key Design Decisions

**Raw `invoke_signed` over Anchor CPI** — avoids a compile-time dependency on the hook crate. Core calls any conforming hook by discriminator, enabling permissionless hook deployment.

**Single-lender pools (V1)** — the pool authority is the lender. They choose their hook and trust its credit policy. Clean trust boundary; no governance surface in V1.

**Scale factor model** — yield accrual follows a ray-denominated (1e27) scale factor, compounding continuously across lender positions. Same pattern as Compound/Wildcat.

**Per-pool positions** — lender and borrower positions are PDAs scoped to a specific pool, not protocol-wide synthetic tokens.

**Protocol fee** — 10% of gross interest earned (`PROTOCOL_FEE_BIPS = 1000`), claimable by the protocol fee recipient.

## Instructions

### normandy-core (7)

| Instruction | Description |
|---|---|
| `initialize_protocol` | Deploy protocol state, set fee recipient |
| `initialize_pool` | Create a pool with hook, rate, term, reserve config |
| `deposit` | Lender deposits underlying tokens |
| `borrow` | Agent borrows; triggers hook CPI with reputation proof |
| `repay` | Agent repays principal + interest |
| `withdraw` | Lender withdraws deposits + accrued yield |
| `claim_protocol_fees` | Pull accumulated protocol fees to fee recipient |

### normandy-hook-fixed-term (2)

| Instruction | Description |
|---|---|
| `initialize` | Deploy hook config (borrow cap, PnL requirement) |
| `on_borrow` | Validate agent reputation proof; called by core during borrow |

## Agent Skill

`normandy-skill.md` is the primary UX layer for agents. Load it into context to interact with the protocol directly via Solana RPC — no MCP server required. It covers PDA derivation, instruction encoding, pool discovery, and a decision framework for evaluating borrow terms.

## Build & Test

```bash
anchor build
anchor test
```

Requirements: Rust 1.89+ (pinned via `rust-toolchain.toml` for SBF), Solana CLI 3.1+, Anchor 0.32.1, Node 24+, Yarn.

## Repo Structure

```
programs/
  normandy-core/            # Pool, positions, lending loop (7 instructions)
  normandy-hook-fixed-term/ # Credit check hook (2 instructions)
tests/
  normandy-v1.ts            # Full lifecycle integration test
normandy-skill.md           # Agent interaction guide
Anchor.toml
```
