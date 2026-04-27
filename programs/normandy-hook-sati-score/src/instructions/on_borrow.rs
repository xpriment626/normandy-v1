use anchor_lang::prelude::*;

use crate::constants::{compute_reputation_nonce, sas, sati};
use crate::errors::HookError;
use crate::state::HookConfig;

/// Return data shape sent back to normandy-core via set_return_data.
/// Must stay byte-compatible with normandy-core::cpi_interface::OnBorrowResult.
#[derive(AnchorSerialize)]
pub struct OnBorrowResult {
    pub approved: bool,
    pub interest_bips: u16,
    pub term_seconds: i64,
}

#[derive(Accounts)]
pub struct OnBorrow<'info> {
    #[account(
        seeds = [HookConfig::SEED, pool.key().as_ref()],
        bump = hook_config.bump,
    )]
    pub hook_config: Account<'info, HookConfig>,

    /// CHECK: The Pool PDA from normandy-core that signs via CPI.
    pub pool: Signer<'info>,
    // SATI ReputationScoreV3 attestation accounts arrive in ctx.remaining_accounts.
    // normandy-core::borrow forwards them; the hook iterates and verifies.
}

#[allow(clippy::too_many_arguments)]
pub fn handle_on_borrow<'info>(
    ctx: Context<'_, '_, '_, 'info, OnBorrow<'info>>,
    agent: Pubkey,
    amount: u64,
    _reputation_proof: Vec<u8>,
    min_interest_bips: u16,
    max_interest_bips: u16,
    min_term_seconds: i64,
    max_term_seconds: i64,
) -> Result<()> {
    let config = &ctx.accounts.hook_config;

    // Per-agent borrow cap (same as hook-fixed-term).
    require!(amount <= config.max_borrow_per_agent, HookError::BorrowExceedsCap);

    let now = Clock::get()?.unix_timestamp;

    // Track the most-specific failure across candidates so the borrower's client
    // sees a useful error if no remaining account passes.
    let mut last_failure = HookError::NoValidReputationScore;
    let mut approved_provider: Option<Pubkey> = None;

    for acc in ctx.remaining_accounts.iter() {
        // Owner check stays at the AccountInfo layer because pure-byte verify
        // can't see ownership metadata.
        if acc.owner != &sas::PROGRAM_ID {
            last_failure = HookError::InvalidScoreAccount;
            continue;
        }
        let data = match acc.try_borrow_data() {
            Ok(d) => d,
            Err(_) => {
                last_failure = HookError::InvalidScoreLayout;
                continue;
            }
        };
        match verify_score_bytes(&data, &agent, config, now) {
            Ok(provider) => {
                approved_provider = Some(provider);
                break;
            }
            Err(e) => {
                last_failure = e;
                continue;
            }
        }
    }

    if approved_provider.is_none() {
        return Err(error!(last_failure));
    }

    let result = OnBorrowResult {
        approved: true,
        interest_bips: min_interest_bips,
        term_seconds: min_term_seconds,
    };
    let bytes = result
        .try_to_vec()
        .map_err(|_| HookError::SerializationError)?;
    anchor_lang::solana_program::program::set_return_data(&bytes);

    // Rate scaling by score is deferred — see spec Open Decision 3.
    let _ = max_interest_bips;
    let _ = max_term_seconds;
    Ok(())
}

/// Verify a candidate SAS attestation account's bytes against the hook config.
///
/// **Owner check is the caller's responsibility** — this function operates on
/// raw bytes and can't see account metadata. The caller must enforce
/// `account.owner == sas::PROGRAM_ID` before invoking.
///
/// On success, returns the matched provider pubkey. On failure, returns the
/// most-specific error so the caller can surface it.
pub fn verify_score_bytes(
    data: &[u8],
    agent: &Pubkey,
    config: &HookConfig,
    now: i64,
) -> std::result::Result<Pubkey, HookError> {
    // 1. Minimum size for an empty-payload SATI attestation: SAS header + SATI
    //    minimum payload + SAS tail.
    let min_size = sas::HEADER_SIZE + sati::PAYLOAD_MIN_SIZE + sas::TAIL_SIZE;
    if data.len() < min_size {
        return Err(HookError::InvalidScoreLayout);
    }

    // 2. SAS discriminator byte.
    if data[sas::OFF_DISCRIMINATOR] != sas::ATTESTATION_DISCRIMINATOR {
        return Err(HookError::InvalidScoreLayout);
    }

    // 3. Embedded credential + schema must match the lender's configured ones
    //    (rejects spoofed schemas that happen to use the SAS program).
    let credential = read_pubkey(data, sas::OFF_CREDENTIAL);
    let schema = read_pubkey(data, sas::OFF_SCHEMA);
    if credential != config.credential || schema != config.schema {
        return Err(HookError::InvalidScoreLayout);
    }

    // 4. Match the embedded nonce against the expected nonce for some
    //    (allowed_provider, agent) pair. First match wins.
    let nonce = read_bytes32(data, sas::OFF_NONCE);
    let mut matched_provider: Option<Pubkey> = None;
    for provider in config.allowed_providers.iter() {
        if compute_reputation_nonce(provider, agent) == nonce {
            matched_provider = Some(*provider);
            break;
        }
    }
    let provider = matched_provider.ok_or(HookError::UntrustedProvider)?;

    // 5. Read data_len and locate the payload + tail.
    let data_len = u32::from_le_bytes(
        data[sas::OFF_DATA_LEN..sas::OFF_DATA_LEN + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let payload_start = sas::OFF_DATA;
    let payload_end = payload_start
        .checked_add(data_len)
        .ok_or(HookError::InvalidScoreLayout)?;
    let tail_end = payload_end
        .checked_add(sas::TAIL_SIZE)
        .ok_or(HookError::InvalidScoreLayout)?;
    if tail_end > data.len() {
        return Err(HookError::InvalidScoreLayout);
    }
    if data_len < sati::PAYLOAD_MIN_SIZE {
        return Err(HookError::InvalidScoreLayout);
    }

    let payload = &data[payload_start..payload_end];

    // 6. Layout version + cross-checks against PDA-derived facts. The PDA match
    //    in step 4 already constrains (provider, agent) cryptographically; these
    //    asserts catch future SAS layout drift or content corruption.
    if payload[sati::PAY_OFF_LAYOUT_VERSION] != sati::CURRENT_LAYOUT_VERSION {
        return Err(HookError::InvalidScoreLayout);
    }
    let task_ref = read_payload_bytes32(payload, sati::PAY_OFF_TASK_REF);
    let payload_agent = read_payload_pubkey(payload, sati::PAY_OFF_AGENT_MINT);
    let payload_counterparty = read_payload_pubkey(payload, sati::PAY_OFF_COUNTERPARTY);
    if task_ref != nonce || &payload_agent != agent || payload_counterparty != provider {
        return Err(HookError::InvalidScoreLayout);
    }

    // 7. Outcome threshold.
    let outcome = payload[sati::PAY_OFF_OUTCOME];
    if outcome < config.min_outcome {
        return Err(HookError::ReputationScoreBelowThreshold);
    }

    // 8. Expiry.
    //
    //    SAS does NOT store creation timestamp — only `expiry`. SATI's
    //    ReputationScoreV3 doesn't encode a timestamp in its payload either.
    //    V1 enforces:
    //      - if `max_score_age_seconds > 0`, expiry MUST be set (lender refuses
    //        never-expiring scores when freshness is required).
    //      - if expiry is set, it must not be in the past.
    //    True age-based freshness is deferred until SATI surfaces a creation
    //    timestamp or this hook starts parsing content JSON.
    let expiry_offset = payload_end + sas::TAIL_OFF_EXPIRY;
    let expiry = i64::from_le_bytes(
        data[expiry_offset..expiry_offset + 8]
            .try_into()
            .unwrap(),
    );
    if config.max_score_age_seconds > 0 && expiry == 0 {
        return Err(HookError::ReputationScoreStale);
    }
    if expiry > 0 && now > expiry {
        return Err(HookError::ReputationScoreStale);
    }

    Ok(provider)
}

#[inline]
fn read_pubkey(data: &[u8], offset: usize) -> Pubkey {
    Pubkey::new_from_array(read_bytes32(data, offset))
}

#[inline]
fn read_bytes32(data: &[u8], offset: usize) -> [u8; 32] {
    data[offset..offset + 32].try_into().unwrap()
}

#[inline]
fn read_payload_pubkey(payload: &[u8], offset: usize) -> Pubkey {
    Pubkey::new_from_array(read_payload_bytes32(payload, offset))
}

#[inline]
fn read_payload_bytes32(payload: &[u8], offset: usize) -> [u8; 32] {
    payload[offset..offset + 32].try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build SATI ReputationScoreV3 payload bytes (the "data" field of an
    /// SAS Attestation account).
    fn build_satv3_payload(
        layout_version: u8,
        task_ref: [u8; 32],
        agent_mint: Pubkey,
        counterparty: Pubkey,
        outcome: u8,
        content: &[u8],
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(135 + content.len());
        buf.push(layout_version);
        buf.extend_from_slice(&task_ref);
        buf.extend_from_slice(&agent_mint.to_bytes());
        buf.extend_from_slice(&counterparty.to_bytes());
        buf.push(outcome);
        buf.extend_from_slice(&[0u8; 32]); // data_hash (CounterpartySigned)
        buf.push(1u8); // content_type = JSON
        buf.extend_from_slice(&(content.len() as u32).to_le_bytes());
        buf.extend_from_slice(content);
        buf
    }

    /// Build a full SAS Attestation account data buffer wrapping a payload.
    fn build_attestation_bytes(
        discriminator: u8,
        nonce: [u8; 32],
        credential: Pubkey,
        schema: Pubkey,
        payload: &[u8],
        expiry: i64,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(101 + payload.len() + 72);
        buf.push(discriminator);
        buf.extend_from_slice(&nonce);
        buf.extend_from_slice(&credential.to_bytes());
        buf.extend_from_slice(&schema.to_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(payload);
        buf.extend_from_slice(&[0u8; 32]); // signer
        buf.extend_from_slice(&expiry.to_le_bytes());
        buf.extend_from_slice(&[0u8; 32]); // token_account
        buf
    }

    /// A "well-formed" fixture: real SAS layout, real SATI payload, valid nonce
    /// derivation, outcome=Positive, no expiry.
    struct Fixture {
        agent: Pubkey,
        provider: Pubkey,
        config: HookConfig,
        attestation_bytes: Vec<u8>,
    }

    fn good_fixture() -> Fixture {
        let agent = Pubkey::new_unique();
        let provider = Pubkey::new_unique();
        let credential = Pubkey::new_unique();
        let schema = Pubkey::new_unique();

        let nonce = compute_reputation_nonce(&provider, &agent);
        let payload = build_satv3_payload(
            sati::CURRENT_LAYOUT_VERSION,
            nonce,
            agent,
            provider,
            2, // Positive
            br#"{"score":85}"#,
        );
        let attestation_bytes = build_attestation_bytes(
            sas::ATTESTATION_DISCRIMINATOR,
            nonce,
            credential,
            schema,
            &payload,
            0, // never expires
        );

        let config = HookConfig {
            pool: Pubkey::default(),
            max_borrow_per_agent: 1_000_000,
            credential,
            schema,
            allowed_providers: vec![provider],
            min_outcome: 1, // Neutral
            max_score_age_seconds: 0,
            bump: 0,
        };

        Fixture {
            agent,
            provider,
            config,
            attestation_bytes,
        }
    }

    #[test]
    fn happy_path_approves() {
        let f = good_fixture();
        let result = verify_score_bytes(&f.attestation_bytes, &f.agent, &f.config, 1_700_000_000);
        assert_eq!(result, Ok(f.provider));
    }

    #[test]
    fn untrusted_provider_rejected() {
        // Build an attestation under provider X, but config trusts only provider Y.
        let mut f = good_fixture();
        let stranger = Pubkey::new_unique();
        f.config.allowed_providers = vec![stranger]; // doesn't include f.provider
        let result = verify_score_bytes(&f.attestation_bytes, &f.agent, &f.config, 1_700_000_000);
        assert_eq!(result, Err(HookError::UntrustedProvider));
    }

    #[test]
    fn below_threshold_rejected() {
        // Build a fixture with outcome=Neutral (1) but config requires Positive (2).
        let agent = Pubkey::new_unique();
        let provider = Pubkey::new_unique();
        let credential = Pubkey::new_unique();
        let schema = Pubkey::new_unique();
        let nonce = compute_reputation_nonce(&provider, &agent);
        let payload = build_satv3_payload(
            sati::CURRENT_LAYOUT_VERSION,
            nonce,
            agent,
            provider,
            1, // Neutral
            b"",
        );
        let bytes = build_attestation_bytes(
            sas::ATTESTATION_DISCRIMINATOR,
            nonce,
            credential,
            schema,
            &payload,
            0,
        );
        let config = HookConfig {
            pool: Pubkey::default(),
            max_borrow_per_agent: 0,
            credential,
            schema,
            allowed_providers: vec![provider],
            min_outcome: 2, // Positive required
            max_score_age_seconds: 0,
            bump: 0,
        };
        let result = verify_score_bytes(&bytes, &agent, &config, 1_700_000_000);
        assert_eq!(result, Err(HookError::ReputationScoreBelowThreshold));
    }

    #[test]
    fn stale_expiry_in_past_rejected() {
        // Build with expiry = 1000, check at now = 5000.
        let f = good_fixture();
        let agent = f.agent;
        let provider = f.provider;
        let nonce = compute_reputation_nonce(&provider, &agent);
        let payload = build_satv3_payload(
            sati::CURRENT_LAYOUT_VERSION,
            nonce,
            agent,
            provider,
            2,
            b"",
        );
        let bytes = build_attestation_bytes(
            sas::ATTESTATION_DISCRIMINATOR,
            nonce,
            f.config.credential,
            f.config.schema,
            &payload,
            1000, // expired
        );
        let result = verify_score_bytes(&bytes, &agent, &f.config, 5000);
        assert_eq!(result, Err(HookError::ReputationScoreStale));
    }

    #[test]
    fn stale_no_expiry_when_freshness_required_rejected() {
        // Lender requires freshness (max_score_age_seconds > 0), score has no expiry.
        let mut f = good_fixture();
        f.config.max_score_age_seconds = 86400;
        // f.attestation_bytes has expiry = 0
        let result = verify_score_bytes(&f.attestation_bytes, &f.agent, &f.config, 1_700_000_000);
        assert_eq!(result, Err(HookError::ReputationScoreStale));
    }

    #[test]
    fn invalid_layout_wrong_discriminator_rejected() {
        let mut f = good_fixture();
        f.attestation_bytes[sas::OFF_DISCRIMINATOR] = 99; // not 2
        let result = verify_score_bytes(&f.attestation_bytes, &f.agent, &f.config, 1_700_000_000);
        assert_eq!(result, Err(HookError::InvalidScoreLayout));
    }

    #[test]
    fn invalid_layout_too_short_rejected() {
        let bytes = vec![0u8; 50]; // way too small
        let f = good_fixture();
        let result = verify_score_bytes(&bytes, &f.agent, &f.config, 1_700_000_000);
        assert_eq!(result, Err(HookError::InvalidScoreLayout));
    }

    #[test]
    fn invalid_layout_wrong_credential_rejected() {
        let f = good_fixture();
        let mut config = f.config.clone();
        config.credential = Pubkey::new_unique(); // doesn't match attestation
        let result = verify_score_bytes(&f.attestation_bytes, &f.agent, &config, 1_700_000_000);
        assert_eq!(result, Err(HookError::InvalidScoreLayout));
    }

    #[test]
    fn invalid_layout_wrong_schema_rejected() {
        let f = good_fixture();
        let mut config = f.config.clone();
        config.schema = Pubkey::new_unique(); // doesn't match attestation
        let result = verify_score_bytes(&f.attestation_bytes, &f.agent, &config, 1_700_000_000);
        assert_eq!(result, Err(HookError::InvalidScoreLayout));
    }

    #[test]
    fn invalid_layout_wrong_payload_version_rejected() {
        let agent = Pubkey::new_unique();
        let provider = Pubkey::new_unique();
        let credential = Pubkey::new_unique();
        let schema = Pubkey::new_unique();
        let nonce = compute_reputation_nonce(&provider, &agent);
        let payload = build_satv3_payload(
            99, // wrong layout version
            nonce, agent, provider, 2, b"",
        );
        let bytes = build_attestation_bytes(
            sas::ATTESTATION_DISCRIMINATOR,
            nonce,
            credential,
            schema,
            &payload,
            0,
        );
        let config = HookConfig {
            pool: Pubkey::default(),
            max_borrow_per_agent: 0,
            credential,
            schema,
            allowed_providers: vec![provider],
            min_outcome: 1,
            max_score_age_seconds: 0,
            bump: 0,
        };
        let result = verify_score_bytes(&bytes, &agent, &config, 1_700_000_000);
        assert_eq!(result, Err(HookError::InvalidScoreLayout));
    }

    #[test]
    fn or_semantics_first_allowed_provider_wins() {
        // Three allowed providers, attestation under the third → still approves.
        let f = good_fixture();
        let mut config = f.config.clone();
        config.allowed_providers = vec![
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            f.provider, // last in list, but should still match
        ];
        let result = verify_score_bytes(&f.attestation_bytes, &f.agent, &config, 1_700_000_000);
        assert_eq!(result, Ok(f.provider));
    }

    #[test]
    fn agent_mismatch_rejected() {
        // Attestation is for agent A; verify is called with agent B.
        let f = good_fixture();
        let other_agent = Pubkey::new_unique();
        let result = verify_score_bytes(&f.attestation_bytes, &other_agent, &f.config, 1_700_000_000);
        // Nonce won't match any allowed_provider for `other_agent`.
        assert_eq!(result, Err(HookError::UntrustedProvider));
    }
}
