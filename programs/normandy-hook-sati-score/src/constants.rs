use anchor_lang::prelude::*;

/// Solana Attestation Service constants.
///
/// Source: github.com/solana-foundation/solana-attestation-service
/// (program/src/lib.rs, program/src/state/attestation.rs, program/src/constants.rs)
pub mod sas {
    use super::*;

    /// SAS program ID. Same on mainnet and devnet.
    pub const PROGRAM_ID: Pubkey = pubkey!("22zoJMtdu4tQc2PzL74ZUT7FrwgB1Udec8DdW4yw4BdG");

    /// PDA seed prefix for attestation accounts.
    /// Full seeds: ["attestation", credential, schema, nonce]
    pub const ATTESTATION_SEED: &[u8] = b"attestation";

    /// Discriminator byte at offset 0 distinguishing Attestation accounts.
    /// (0 = Credential, 1 = Schema, 2 = Attestation)
    pub const ATTESTATION_DISCRIMINATOR: u8 = 2;

    /// Account layout offsets (within the SAS Attestation account `data`).
    ///
    /// Layout:
    /// ```text
    /// 0       1     discriminator (=2)
    /// 1       32    nonce
    /// 33      32    credential
    /// 65      32    schema
    /// 97      4     data_len (u32 LE)
    /// 101     N     data (variable; SATI ReputationScoreV3 payload here)
    /// 101+N   32    signer
    /// 133+N   8     expiry (i64 LE; 0 means "never expires")
    /// 141+N   32    token_account
    /// ```
    pub const OFF_DISCRIMINATOR: usize = 0;
    pub const OFF_NONCE: usize = 1;
    pub const OFF_CREDENTIAL: usize = 33;
    pub const OFF_SCHEMA: usize = 65;
    pub const OFF_DATA_LEN: usize = 97;
    pub const OFF_DATA: usize = 101;

    /// Header bytes before the variable `data` section.
    pub const HEADER_SIZE: usize = OFF_DATA;
    /// Tail bytes after the variable `data` section: signer (32) + expiry (8) + token_account (32).
    pub const TAIL_SIZE: usize = 32 + 8 + 32;

    /// Tail field offsets, *relative to the end of `data`*.
    pub const TAIL_OFF_SIGNER: usize = 0;
    pub const TAIL_OFF_EXPIRY: usize = 32;
}

/// SATI (Solana Agent Trust Infrastructure) constants.
///
/// Source: github.com/cascade-protocol/sati
/// (packages/sdk/src/deployed/mainnet.json, packages/sdk/src/schemas.ts,
///  programs/sati/src/constants.rs)
pub mod sati {
    use super::*;

    /// SATI's official credential PDA. Same on mainnet and devnet
    /// (deterministic PDAs of the same authority).
    pub const CREDENTIAL: Pubkey = pubkey!("DQHW6fAhPfGAENuwJVYfzEvUN12DakZgaaGtPPRfGei1");

    /// SATI ReputationScoreV3 schema PDA. Same on mainnet and devnet.
    pub const REPUTATION_SCORE_V3_SCHEMA: Pubkey =
        pubkey!("7MoXgvrFhMxmB84AfAtp8LGfC4sEXUHD6JCQJpfj2jTj");

    /// Current ReputationScoreV3 layout version.
    pub const CURRENT_LAYOUT_VERSION: u8 = 1;

    /// ReputationScoreV3 payload offsets, *within* the SAS attestation `data` field.
    ///
    /// Layout:
    /// ```text
    /// 0       1     layout_version (=1)
    /// 1       32    task_ref (= keccak256(provider || agent_mint) for ReputationScoreV3)
    /// 33      32    agent_mint
    /// 65      32    counterparty (the provider pubkey)
    /// 97      1     outcome (0=Negative, 1=Neutral, 2=Positive)
    /// 98      32    data_hash (zero-filled in CounterpartySigned mode)
    /// 130     1     content_type
    /// 131     4     content_len (u32 LE)
    /// 135     N     content (typically JSON: {score, methodology, feedbackCount, ...})
    /// ```
    pub const PAY_OFF_LAYOUT_VERSION: usize = 0;
    pub const PAY_OFF_TASK_REF: usize = 1;
    pub const PAY_OFF_AGENT_MINT: usize = 33;
    pub const PAY_OFF_COUNTERPARTY: usize = 65;
    pub const PAY_OFF_OUTCOME: usize = 97;
    pub const PAY_OFF_CONTENT_LEN: usize = 131;
    pub const PAY_OFF_CONTENT: usize = 135;

    /// Minimum payload size (no content).
    pub const PAYLOAD_MIN_SIZE: usize = PAY_OFF_CONTENT;
}

/// Compute the SATI ReputationScoreV3 nonce for a given (provider, agent_mint) pair.
///
/// `nonce = keccak256(provider_bytes || agent_mint_bytes)` (64 bytes total).
/// Source: cascade-protocol/sati packages/sdk/src/hashes.ts `computeReputationNonce`.
pub fn compute_reputation_nonce(provider: &Pubkey, agent_mint: &Pubkey) -> [u8; 32] {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&provider.to_bytes());
    buf[32..].copy_from_slice(&agent_mint.to_bytes());
    solana_keccak_hasher::hash(&buf).0
}
