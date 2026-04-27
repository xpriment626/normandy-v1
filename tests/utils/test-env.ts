/**
 * Shared test environment for ProtocolConfig + global keypairs.
 *
 * `protocol_config` is a singleton PDA — only one instance exists per program.
 * Multiple test files all interact with the same one. To make tests order-
 * independent, we:
 *   1. Use a deterministic fee_recipient keypair (same across all test files)
 *      so whichever test creates ProtocolConfig first sets a fee_recipient
 *      the others can still sign as.
 *   2. Provide a helper that creates ProtocolConfig if absent, no-ops otherwise.
 */

import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";

function seedFromString(label: string): Buffer {
  const buf = Buffer.alloc(32);
  buf.write(label, 0, "utf-8");
  return buf;
}

/**
 * Shared fee recipient. Deterministic so multiple test files agree on the
 * same pubkey + can each construct it without coordination.
 */
export const FEE_RECIPIENT = Keypair.fromSeed(
  seedFromString("normandy-shared-fee-recipient")
);

/**
 * Idempotently ensure ProtocolConfig exists. Returns the PDA either way.
 * Call from each test file's `before` hook.
 */
export async function ensureProtocolConfig(
  coreProgram: Program<any>,
  authority: Keypair
): Promise<PublicKey> {
  const [protocolConfigPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("protocol_config")],
    coreProgram.programId
  );

  try {
    await coreProgram.methods
      .initializeProtocol(FEE_RECIPIENT.publicKey)
      .accounts({
        protocolConfig: protocolConfigPda,
        authority: authority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  } catch (e: any) {
    const msg = e?.message ?? "";
    const logs = e?.logs ?? [];
    const alreadyExists =
      msg.includes("already in use") ||
      logs.some((l: string) => l.includes("already in use"));
    if (!alreadyExists) throw e;
  }

  return protocolConfigPda;
}
