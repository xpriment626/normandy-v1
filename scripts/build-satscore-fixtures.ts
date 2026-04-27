/**
 * Generates static fixtures for the hook-sati-score integration test.
 *
 * Produces two files:
 *   tests/fixtures/satscore-happy-account.json  — solana-test-validator --account JSON,
 *                                                  loaded via [[test.validator.account]] in
 *                                                  Anchor.toml. Represents one valid SAS
 *                                                  attestation account on the localnet ledger.
 *   tests/fixtures/satscore-meta.json           — pubkeys + bytes the integration test
 *                                                  needs to construct its calls.
 *
 * Why static fixtures (vs. dynamic SAS instructions): on localnet we don't have SAS
 * deployed and don't have SATI's authority key. The validator accepts arbitrary
 * accounts via --account JSON regardless of whether the listed `owner` is a deployed
 * program — so we can construct an attestation byte-for-byte and load it as inert
 * state. Our hook then reads it the same way it would read a real SAS-created one.
 *
 * Run with:
 *   cd normandy-v1 && yarn ts-node scripts/build-satscore-fixtures.ts
 *
 * Re-run only when the SAS or SATI byte layout assumptions change. Output is committed.
 */

import { Keypair, PublicKey } from "@solana/web3.js";
import { keccak_256 } from "@noble/hashes/sha3";
import * as fs from "fs";
import * as path from "path";

// ─────────────────────────────────────────────────────────────────
// Constants — must match programs/normandy-hook-sati-score/src/constants.rs
// ─────────────────────────────────────────────────────────────────
const SAS_PROGRAM_ID = new PublicKey("22zoJMtdu4tQc2PzL74ZUT7FrwgB1Udec8DdW4yw4BdG");
const ATTESTATION_SEED = Buffer.from("attestation");
const SATI_LAYOUT_VERSION = 1;

// SAS account layout offsets
const SAS_OFF_DISCRIMINATOR = 0;
const SAS_OFF_NONCE = 1;
const SAS_OFF_CREDENTIAL = 33;
const SAS_OFF_SCHEMA = 65;
const SAS_OFF_DATA_LEN = 97;
const SAS_OFF_DATA = 101;
const SAS_TAIL_SIZE = 32 + 8 + 32; // signer + expiry + token_account
const SAS_ATTESTATION_DISCRIMINATOR = 2;

// ─────────────────────────────────────────────────────────────────
// Deterministic keypairs — same pubkeys across runs
// ─────────────────────────────────────────────────────────────────
function seedFromString(label: string): Buffer {
  const buf = Buffer.alloc(32);
  buf.write(label, 0, "utf-8");
  return buf;
}

const credentialKp = Keypair.fromSeed(seedFromString("normandy-test-credential-v1"));
const schemaKp = Keypair.fromSeed(seedFromString("normandy-test-schema-v1"));
const providerKp = Keypair.fromSeed(seedFromString("normandy-test-provider-v1"));
const agentMintKp = Keypair.fromSeed(seedFromString("normandy-test-agent-mint-v1"));

// ─────────────────────────────────────────────────────────────────
// SATI ReputationScoreV3 nonce derivation
// ─────────────────────────────────────────────────────────────────
function computeReputationNonce(provider: PublicKey, agentMint: PublicKey): Buffer {
  const buf = new Uint8Array(64);
  buf.set(provider.toBytes(), 0);
  buf.set(agentMint.toBytes(), 32);
  return Buffer.from(keccak_256(buf));
}

// ─────────────────────────────────────────────────────────────────
// Build the SATI ReputationScoreV3 payload bytes
// ─────────────────────────────────────────────────────────────────
function buildSatv3Payload(args: {
  taskRef: Buffer;
  agentMint: PublicKey;
  counterparty: PublicKey;
  outcome: number; // 0=Negative, 1=Neutral, 2=Positive
  content: Buffer;
}): Buffer {
  const buf = Buffer.alloc(135 + args.content.length);
  let off = 0;
  buf.writeUInt8(SATI_LAYOUT_VERSION, off); off += 1;
  args.taskRef.copy(buf, off); off += 32;
  Buffer.from(args.agentMint.toBytes()).copy(buf, off); off += 32;
  Buffer.from(args.counterparty.toBytes()).copy(buf, off); off += 32;
  buf.writeUInt8(args.outcome, off); off += 1;
  // data_hash: 32 zero bytes (CounterpartySigned mode)
  off += 32;
  buf.writeUInt8(1, off); off += 1; // content_type = JSON
  buf.writeUInt32LE(args.content.length, off); off += 4;
  args.content.copy(buf, off);
  return buf;
}

// ─────────────────────────────────────────────────────────────────
// Build the full SAS Attestation account data buffer
// ─────────────────────────────────────────────────────────────────
function buildSasAttestationBytes(args: {
  nonce: Buffer;
  credential: PublicKey;
  schema: PublicKey;
  payload: Buffer;
  expiry: bigint; // i64; 0 = never
}): Buffer {
  const buf = Buffer.alloc(SAS_OFF_DATA + args.payload.length + SAS_TAIL_SIZE);
  buf.writeUInt8(SAS_ATTESTATION_DISCRIMINATOR, SAS_OFF_DISCRIMINATOR);
  args.nonce.copy(buf, SAS_OFF_NONCE);
  Buffer.from(args.credential.toBytes()).copy(buf, SAS_OFF_CREDENTIAL);
  Buffer.from(args.schema.toBytes()).copy(buf, SAS_OFF_SCHEMA);
  buf.writeUInt32LE(args.payload.length, SAS_OFF_DATA_LEN);
  args.payload.copy(buf, SAS_OFF_DATA);
  // signer (32 zero bytes)
  // expiry (8 bytes LE) at offset SAS_OFF_DATA + payload.length + 32
  const expiryOffset = SAS_OFF_DATA + args.payload.length + 32;
  buf.writeBigInt64LE(args.expiry, expiryOffset);
  // token_account (32 zero bytes)
  return buf;
}

// ─────────────────────────────────────────────────────────────────
// Compose the happy-path fixture
// ─────────────────────────────────────────────────────────────────
const credential = credentialKp.publicKey;
const schema = schemaKp.publicKey;
const provider = providerKp.publicKey;
const agentMint = agentMintKp.publicKey;

const nonce = computeReputationNonce(provider, agentMint);

const [attestationPda] = PublicKey.findProgramAddressSync(
  [ATTESTATION_SEED, credential.toBuffer(), schema.toBuffer(), nonce],
  SAS_PROGRAM_ID
);

const payload = buildSatv3Payload({
  taskRef: nonce,
  agentMint,
  counterparty: provider,
  outcome: 2, // Positive
  content: Buffer.from(JSON.stringify({ score: 85, methodology: "test" })),
});

const attestationData = buildSasAttestationBytes({
  nonce,
  credential,
  schema,
  payload,
  expiry: BigInt(0), // never expires
});

// ─────────────────────────────────────────────────────────────────
// Emit fixture files
// ─────────────────────────────────────────────────────────────────
// solana-test-validator --account format
const accountJson = {
  pubkey: attestationPda.toBase58(),
  account: {
    lamports: 5_000_000, // 0.005 SOL — covers rent for ~280 byte account
    data: [attestationData.toString("base64"), "base64"],
    owner: SAS_PROGRAM_ID.toBase58(),
    executable: false,
    rentEpoch: 0,
    space: attestationData.length,
  },
};

// Test-side metadata — read by the integration test
const meta = {
  description: "Static SAS attestation fixture for hook-sati-score happy path",
  generatedAt: new Date().toISOString(),
  sasProgramId: SAS_PROGRAM_ID.toBase58(),
  credential: credential.toBase58(),
  schema: schema.toBase58(),
  provider: provider.toBase58(),
  agentMint: agentMint.toBase58(),
  attestationPda: attestationPda.toBase58(),
  nonceHex: nonce.toString("hex"),
  outcome: 2,
  expiry: 0,
  attestationByteSize: attestationData.length,
};

const fixturesDir = path.join(__dirname, "..", "tests", "fixtures");
fs.writeFileSync(
  path.join(fixturesDir, "satscore-happy-account.json"),
  JSON.stringify(accountJson, null, 2)
);
fs.writeFileSync(
  path.join(fixturesDir, "satscore-meta.json"),
  JSON.stringify(meta, null, 2)
);

console.log("✓ Wrote fixtures:");
console.log(`  satscore-happy-account.json (${attestationData.length} bytes payload)`);
console.log(`  satscore-meta.json`);
console.log("");
console.log("Test addresses:");
console.log(`  credential       ${credential.toBase58()}`);
console.log(`  schema           ${schema.toBase58()}`);
console.log(`  provider         ${provider.toBase58()}`);
console.log(`  agent_mint       ${agentMint.toBase58()}`);
console.log(`  attestation PDA  ${attestationPda.toBase58()}`);
