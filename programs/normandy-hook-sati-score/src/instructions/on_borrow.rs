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
        match verify_score_account(acc, &agent, config, now) {
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

/// Verify a single candidate SAS attestation account against the hook config.
///
/// On success, returns the matched provider pubkey. On failure, returns the
/// most-specific error so the caller can surface it.
fn verify_score_account(
    acc: &AccountInfo,
    agent: &Pubkey,
    config: &HookConfig,
    now: i64,
) -> std::result::Result<Pubkey, HookError> {
    // 1. Owner must be the SAS program.
    if acc.owner != &sas::PROGRAM_ID {
        return Err(HookError::InvalidScoreAccount);
    }

    let data = acc
        .try_borrow_data()
        .map_err(|_| HookError::InvalidScoreLayout)?;

    // 2. Minimum size for an empty-payload SATI attestation: SAS header + SATI
    //    minimum payload + SAS tail.
    let min_size = sas::HEADER_SIZE + sati::PAYLOAD_MIN_SIZE + sas::TAIL_SIZE;
    if data.len() < min_size {
        return Err(HookError::InvalidScoreLayout);
    }

    // 3. SAS discriminator byte.
    if data[sas::OFF_DISCRIMINATOR] != sas::ATTESTATION_DISCRIMINATOR {
        return Err(HookError::InvalidScoreLayout);
    }

    // 4. Embedded credential + schema must be SATI's official ones (rejects
    //    spoofed schemas that happen to use the SAS program).
    let credential = read_pubkey(&data, sas::OFF_CREDENTIAL);
    let schema = read_pubkey(&data, sas::OFF_SCHEMA);
    if credential != sati::CREDENTIAL || schema != sati::REPUTATION_SCORE_V3_SCHEMA {
        return Err(HookError::InvalidScoreLayout);
    }

    // 5. Match the embedded nonce against the expected nonce for some
    //    (allowed_provider, agent) pair. First match wins.
    let nonce = read_bytes32(&data, sas::OFF_NONCE);
    let mut matched_provider: Option<Pubkey> = None;
    for provider in config.allowed_providers.iter() {
        if compute_reputation_nonce(provider, agent) == nonce {
            matched_provider = Some(*provider);
            break;
        }
    }
    let provider = matched_provider.ok_or(HookError::UntrustedProvider)?;

    // 6. Read data_len and locate the payload + tail.
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

    // 7. Layout version + cross-checks against PDA-derived facts. The PDA match
    //    in step 5 already constrains (provider, agent) cryptographically; these
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

    // 8. Outcome threshold.
    let outcome = payload[sati::PAY_OFF_OUTCOME];
    if outcome < config.min_outcome {
        return Err(HookError::ReputationScoreBelowThreshold);
    }

    // 9. Expiry.
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
