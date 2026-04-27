/**
 * Integration test for hook-sati-score: full init -> deposit -> borrow lifecycle
 * with a SAS ReputationScoreV3 attestation supplied via remaining_accounts.
 *
 * The attestation account is pre-loaded into solana-test-validator from
 * tests/fixtures/satscore-happy-account.json (see Anchor.toml's
 * [[test.validator.account]] block, regenerate with build-satscore-fixtures.ts).
 *
 * Scope: this single test proves the *transaction wiring* end-to-end —
 * core::borrow forwards remaining_accounts, hook::on_borrow reads the SAS
 * account, verifies bytes against HookConfig, and approves. Exhaustive
 * verification-logic coverage is in the Rust unit tests
 * (programs/normandy-hook-sati-score/src/instructions/on_borrow.rs).
 *
 * V1 simplification: this hook treats the borrow-signing agent pubkey as the
 * agent_mint for nonce derivation. Real production needs a separate
 * agent_mint arg on borrow so a wallet can hold an NFT distinct from itself.
 */

import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { NormandyCore } from "../target/types/normandy_core";
import { NormandyHookSatiScore } from "../target/types/normandy_hook_sati_score";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import { assert } from "chai";
import * as fs from "fs";
import * as path from "path";
import { FEE_RECIPIENT, ensureProtocolConfig } from "./utils/test-env";

interface SatiscoreMeta {
  sasProgramId: string;
  credential: string;
  schema: string;
  provider: string;
  agentMint: string;
  attestationPda: string;
  outcome: number;
  expiry: number;
}

describe("Normandy V1 — hook-sati-score Integration (Happy Path)", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const coreProgram = anchor.workspace.normandyCore as Program<NormandyCore>;
  const hookProgram = anchor.workspace
    .normandyHookSatiScore as Program<NormandyHookSatiScore>;

  // Load fixture metadata produced by tests/fixtures/build-satscore-fixtures.ts
  const meta: SatiscoreMeta = JSON.parse(
    fs.readFileSync(
      path.join(__dirname, "fixtures", "satscore-meta.json"),
      "utf-8"
    )
  );

  const credential = new PublicKey(meta.credential);
  const schema = new PublicKey(meta.schema);
  const trustedProvider = new PublicKey(meta.provider);
  const attestationPda = new PublicKey(meta.attestationPda);

  // Reproduce the same agent keypair the fixture script used. The hook computes
  // the expected nonce as keccak256(provider || agent_signer_pubkey), so the
  // signer's pubkey must equal the agent_mint in the pre-baked attestation.
  function seedFromString(label: string): Buffer {
    const buf = Buffer.alloc(32);
    buf.write(label, 0, "utf-8");
    return buf;
  }
  const agent = Keypair.fromSeed(seedFromString("normandy-test-agent-mint-v1"));
  assert.equal(
    agent.publicKey.toBase58(),
    meta.agentMint,
    "test agent keypair must derive the fixture's agent_mint pubkey"
  );

  // Actors
  const authority = provider.wallet.payer;
  const lender = Keypair.generate();
  const feeRecipient = FEE_RECIPIENT; // shared deterministic — see tests/utils/test-env.ts

  let usdcMint: PublicKey;
  const DECIMALS = 6;
  let lenderTokenAccount: PublicKey;
  let agentTokenAccount: PublicKey;

  const vaultKeypair = Keypair.generate();

  // Use POOL_ID = 2 to avoid PDA collision with the fixed-term test (POOL_ID = 1).
  const POOL_ID = new BN(2);
  const INTEREST_BIPS = 1000;
  const TERM_SECONDS = new BN(86400);
  const RESERVE_RATIO_BIPS = 1000;
  const POSITION_MODE = 0;
  const DEPOSIT_WINDOW_END = new BN(0);
  const MAX_BORROW_PER_AGENT = new BN(1_000_000_000);
  const MIN_OUTCOME = 1; // Neutral or higher; fixture is Positive (2)
  const MAX_SCORE_AGE_SECONDS = new BN(0); // disable freshness gate; fixture has no expiry

  const DEPOSIT_AMOUNT = new BN(1_000_000_000);
  const BORROW_AMOUNT = new BN(500_000_000);

  let protocolConfigPda: PublicKey;
  let poolPda: PublicKey;
  let lenderPositionPda: PublicKey;
  let borrowerPositionPda: PublicKey;
  let hookConfigPda: PublicKey;

  before(async () => {
    [protocolConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("protocol_config")],
      coreProgram.programId
    );
    [poolPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("pool"),
        authority.publicKey.toBuffer(),
        POOL_ID.toArrayLike(Buffer, "le", 8),
      ],
      coreProgram.programId
    );
    [lenderPositionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("lender"), poolPda.toBuffer(), lender.publicKey.toBuffer()],
      coreProgram.programId
    );
    [borrowerPositionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("borrower"), poolPda.toBuffer(), agent.publicKey.toBuffer()],
      coreProgram.programId
    );
    [hookConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("hook_config"), poolPda.toBuffer()],
      hookProgram.programId
    );

    const airdrop = 10 * anchor.web3.LAMPORTS_PER_SOL;
    const sigs = await Promise.all([
      provider.connection.requestAirdrop(lender.publicKey, airdrop),
      provider.connection.requestAirdrop(agent.publicKey, airdrop),
      provider.connection.requestAirdrop(feeRecipient.publicKey, airdrop),
    ]);
    for (const s of sigs) await provider.connection.confirmTransaction(s, "confirmed");

    usdcMint = await createMint(
      provider.connection,
      authority,
      authority.publicKey,
      null,
      DECIMALS
    );
    lenderTokenAccount = await createAccount(
      provider.connection,
      lender,
      usdcMint,
      lender.publicKey
    );
    agentTokenAccount = await createAccount(
      provider.connection,
      agent,
      usdcMint,
      agent.publicKey
    );
    await mintTo(
      provider.connection,
      authority,
      usdcMint,
      lenderTokenAccount,
      authority,
      DEPOSIT_AMOUNT.toNumber()
    );

    console.log("  Setup complete:");
    console.log(`    Pool PDA:        ${poolPda.toBase58()}`);
    console.log(`    HookConfig PDA:  ${hookConfigPda.toBase58()}`);
    console.log(`    Hook program:    ${hookProgram.programId.toBase58()}`);
    console.log(`    Attestation PDA: ${attestationPda.toBase58()} (preloaded)`);
  });

  it("initialize_protocol — idempotent, may already exist from fixed-term test", async () => {
    await ensureProtocolConfig(coreProgram, authority);
    const config = await coreProgram.account.protocolConfig.fetch(protocolConfigPda);
    assert.ok(config.authority.equals(authority.publicKey));
    assert.ok(config.feeRecipient.equals(feeRecipient.publicKey), "shared fee_recipient");
  });

  it("initialize_pool — wires hook-sati-score with credential/schema/providers", async () => {
    // hook-sati-score init args borsh: u64 + Pubkey + Pubkey + Vec<Pubkey> + u8 + i64
    // For 1 provider: 8 + 32 + 32 + (4 + 32) + 1 + 8 = 117 bytes
    const hookInitData = Buffer.alloc(8 + 32 + 32 + 4 + 32 + 1 + 8);
    let off = 0;
    hookInitData.writeBigUInt64LE(BigInt(MAX_BORROW_PER_AGENT.toString()), off); off += 8;
    credential.toBuffer().copy(hookInitData, off); off += 32;
    schema.toBuffer().copy(hookInitData, off); off += 32;
    hookInitData.writeUInt32LE(1, off); off += 4; // Vec<Pubkey> length = 1
    trustedProvider.toBuffer().copy(hookInitData, off); off += 32;
    hookInitData.writeUInt8(MIN_OUTCOME, off); off += 1;
    hookInitData.writeBigInt64LE(BigInt(MAX_SCORE_AGE_SECONDS.toString()), off);

    const tx = await coreProgram.methods
      .initializePool(
        POOL_ID,
        INTEREST_BIPS,
        TERM_SECONDS,
        RESERVE_RATIO_BIPS,
        POSITION_MODE,
        DEPOSIT_WINDOW_END,
        hookInitData
      )
      .accounts({
        pool: poolPda,
        vault: vaultKeypair.publicKey,
        underlyingMint: usdcMint,
        hookProgram: hookProgram.programId,
        hookConfig: hookConfigPda,
        authority: authority.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([vaultKeypair])
      .rpc();
    console.log(`    tx: ${tx}`);

    // Verify the hook actually populated HookConfig with our values via CPI.
    const hookConfig = await hookProgram.account.hookConfig.fetch(hookConfigPda);
    assert.ok(hookConfig.pool.equals(poolPda), "hookConfig.pool");
    assert.ok(
      hookConfig.maxBorrowPerAgent.eq(MAX_BORROW_PER_AGENT),
      "hookConfig.maxBorrowPerAgent"
    );
    assert.ok(hookConfig.credential.equals(credential), "hookConfig.credential");
    assert.ok(hookConfig.schema.equals(schema), "hookConfig.schema");
    assert.equal(hookConfig.allowedProviders.length, 1, "1 allowed provider");
    assert.ok(
      (hookConfig.allowedProviders[0] as PublicKey).equals(trustedProvider),
      "allowedProviders[0]"
    );
    assert.equal(hookConfig.minOutcome, MIN_OUTCOME, "hookConfig.minOutcome");
    assert.ok(
      hookConfig.maxScoreAgeSeconds.eq(MAX_SCORE_AGE_SECONDS),
      "hookConfig.maxScoreAgeSeconds"
    );
    console.log("    HookConfig populated via CPI with sati-score fields");
  });

  it("deposit — lender funds the pool", async () => {
    await coreProgram.methods
      .deposit(DEPOSIT_AMOUNT)
      .accounts({
        pool: poolPda,
        lenderPosition: lenderPositionPda,
        vault: vaultKeypair.publicKey,
        lenderTokenAccount: lenderTokenAccount,
        lender: lender.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([lender])
      .rpc();

    const vault = await getAccount(provider.connection, vaultKeypair.publicKey);
    assert.equal(Number(vault.amount), DEPOSIT_AMOUNT.toNumber());
    console.log(`    Vault funded with ${DEPOSIT_AMOUNT.toNumber()}`);
  });

  it("borrow — agent borrows with SATI ReputationScoreV3 attestation", async () => {
    // reputation_proof is unused by hook-sati-score but core's signature still
    // takes a Vec<u8> — pass empty.
    const reputationProof = Buffer.alloc(0);

    const tx = await coreProgram.methods
      .borrow(BORROW_AMOUNT, reputationProof)
      .accounts({
        pool: poolPda,
        borrowerPosition: borrowerPositionPda,
        vault: vaultKeypair.publicKey,
        agentTokenAccount: agentTokenAccount,
        agent: agent.publicKey,
        hookProgram: hookProgram.programId,
        hookConfig: hookConfigPda,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts([
        { pubkey: attestationPda, isSigner: false, isWritable: false },
      ])
      .signers([agent])
      .rpc();
    console.log(`    tx: ${tx}`);

    const agentAccount = await getAccount(provider.connection, agentTokenAccount);
    assert.equal(
      Number(agentAccount.amount),
      BORROW_AMOUNT.toNumber(),
      "agent received borrowed amount"
    );

    const position = await coreProgram.account.borrowerPosition.fetch(borrowerPositionPda);
    assert.ok(position.principal.eq(BORROW_AMOUNT), "principal recorded");
    assert.equal(position.annualInterestBips, INTEREST_BIPS, "interest recorded");
    assert.equal(position.status, 0, "position is Active (0)");

    console.log("    Borrow approved by hook-sati-score after on-chain SAS verify ✓");
  });

  after(() => {
    console.log("\n  ════════════════════════════════════════════════════════");
    console.log("  hook-sati-score happy path verified end-to-end:");
    console.log("    1. init_pool with credential/schema/providers via CPI  ✓");
    console.log("    2. deposit                                              ✓");
    console.log("    3. borrow with attestation forwarded to hook on_borrow  ✓");
    console.log("       (hook verified SAS bytes + nonce + outcome on-chain)");
    console.log("  ════════════════════════════════════════════════════════");
  });
});
