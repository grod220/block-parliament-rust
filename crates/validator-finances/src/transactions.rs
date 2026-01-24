//! On-chain transaction fetching and parsing

use anyhow::Result;
use chrono::DateTime;
use serde::Serialize;
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiTransactionEncoding,
};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;

use crate::addresses::{self, AddressCategory};
use crate::config::Config;
use crate::constants;

/// Extract account keys from transaction (works for both legacy and versioned)
fn extract_account_keys(tx: &EncodedTransaction, _debug: bool) -> Option<Vec<Pubkey>> {
    match tx {
        EncodedTransaction::Json(ui_tx) => {
            // JSON-parsed transaction - works for both legacy and versioned
            match &ui_tx.message {
                UiMessage::Parsed(parsed_msg) => {
                    // Parsed message has account_keys as strings
                    parsed_msg
                        .account_keys
                        .iter()
                        .filter_map(|key| Pubkey::from_str(&key.pubkey).ok())
                        .collect::<Vec<_>>()
                        .into()
                }
                UiMessage::Raw(raw_msg) => {
                    // Raw message has account_keys as strings
                    raw_msg
                        .account_keys
                        .iter()
                        .filter_map(|key| Pubkey::from_str(key).ok())
                        .collect::<Vec<_>>()
                        .into()
                }
            }
        }
        EncodedTransaction::LegacyBinary(_) | EncodedTransaction::Binary(_, _) => {
            // Try to decode binary format
            tx.decode()
                .map(|decoded| decoded.message.static_account_keys().to_vec())
        }
        EncodedTransaction::Accounts(_) => {
            // Accounts-only encoding doesn't have full tx data
            None
        }
    }
}

/// Inflation reward for a single epoch
#[derive(Debug, Clone, Serialize)]
pub struct EpochReward {
    pub epoch: u64,
    pub amount_lamports: u64,
    pub amount_sol: f64,
    pub commission: u8,
    pub effective_slot: u64,
    pub date: Option<String>,
}

/// SOL transfer parsed from transaction
#[derive(Debug, Clone, Serialize)]
pub struct SolTransfer {
    pub signature: String,
    pub slot: u64,
    pub timestamp: Option<i64>,
    pub date: Option<String>,
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount_lamports: u64,
    pub amount_sol: f64,
    pub from_label: String,
    pub to_label: String,
    pub from_category: AddressCategory,
    pub to_category: AddressCategory,
}

/// Categorized transfers
#[derive(Debug, Default)]
pub struct CategorizedTransfers {
    /// Initial seeding from personal wallet
    pub seeding: Vec<SolTransfer>,
    /// SFDP vote cost reimbursements
    pub sfdp_reimbursements: Vec<SolTransfer>,
    /// Jito MEV deposits
    pub mev_deposits: Vec<SolTransfer>,
    /// Internal transfers to fund vote account
    pub vote_funding: Vec<SolTransfer>,
    /// Withdrawals to exchanges or personal
    pub withdrawals: Vec<SolTransfer>,
    /// Other/uncategorized
    pub other: Vec<SolTransfer>,
}

/// Fetch inflation rewards for a range of epochs
pub async fn fetch_inflation_rewards(
    config: &Config,
    start_epoch: u64,
    end_epoch: Option<u64>,
) -> Result<Vec<EpochReward>> {
    fetch_inflation_rewards_internal(config, start_epoch, end_epoch, false).await
}

/// Fetch inflation rewards for current epoch (suppresses expected errors)
pub async fn fetch_current_epoch_rewards(
    config: &Config,
    current_epoch: u64,
) -> Result<Vec<EpochReward>> {
    fetch_inflation_rewards_internal(config, current_epoch, Some(current_epoch), true).await
}

/// Internal function to fetch inflation rewards with optional error suppression
async fn fetch_inflation_rewards_internal(
    config: &Config,
    start_epoch: u64,
    end_epoch: Option<u64>,
    suppress_errors: bool,
) -> Result<Vec<EpochReward>> {
    let client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());

    // Get current epoch if end not specified
    let current_epoch = client.get_epoch_info()?.epoch;
    let end = end_epoch.unwrap_or(current_epoch.saturating_sub(1));

    let mut rewards = Vec::new();

    for epoch in start_epoch..=end {
        // Rate limiting
        sleep(Duration::from_millis(constants::EPOCH_REWARD_DELAY_MS)).await;

        match client.get_inflation_reward(&[config.vote_account], Some(epoch)) {
            Ok(result) => {
                if let Some(Some(reward)) = result.first() {
                    let amount_sol = reward.amount as f64 / 1e9;
                    rewards.push(EpochReward {
                        epoch,
                        amount_lamports: reward.amount,
                        amount_sol,
                        commission: reward.commission.unwrap_or(config.commission_percent),
                        effective_slot: reward.effective_slot,
                        date: Some(epoch_to_date(epoch)),
                    });
                    println!("    Epoch {}: {:.6} SOL", epoch, amount_sol);
                } else if suppress_errors {
                    // Expected for current epoch - rewards not yet distributed
                    println!(
                        "    Epoch {} (current): rewards pending epoch completion",
                        epoch
                    );
                }
            }
            Err(e) => {
                if !suppress_errors {
                    eprintln!("    Epoch {}: Error - {}", epoch, e);
                }
                // For current epoch, empty result is expected
            }
        }
    }

    Ok(rewards)
}

/// Fetch all SOL transfers involving our accounts
/// Note: Limited to last 200 transactions per account to avoid RPC timeouts
pub async fn fetch_sol_transfers(config: &Config, verbose: bool) -> Result<Vec<SolTransfer>> {
    let client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());

    let mut all_transfers = Vec::new();

    // SFDP reimbursement address (Solana Foundation vote cost reimbursements)
    let sfdp_address =
        Pubkey::from_str(constants::SFDP_REIMBURSEMENT).expect("Invalid SFDP address");

    // Fetch for withdraw authority, personal wallet, and SFDP address
    // Skip identity (dominated by vote txs) and vote account
    // Personal wallet shows seeding; SFDP shows reimbursements to our accounts
    for (label, account) in [
        ("withdraw authority", config.withdraw_authority),
        ("personal wallet", config.personal_wallet),
        ("SFDP reimbursement", sfdp_address),
    ] {
        println!(
            "    Fetching transactions for {} ({})...",
            label,
            &account.to_string()[..8]
        );

        let mut signatures = Vec::new();
        let mut before: Option<Signature> = None;
        let max_signatures = constants::MAX_SIGNATURES_PER_ACCOUNT;

        // Paginate through signatures with retry logic
        let mut retries = 0;
        let max_retries = 3;

        loop {
            if signatures.len() >= max_signatures {
                println!("      Reached limit of {} signatures", max_signatures);
                break;
            }

            let sig_config = GetConfirmedSignaturesForAddress2Config {
                before,
                until: None,
                limit: Some(100), // Smaller batches
                commitment: Some(CommitmentConfig::confirmed()),
            };

            match client.get_signatures_for_address_with_config(&account, sig_config) {
                Ok(batch) => {
                    retries = 0; // Reset retries on success

                    if batch.is_empty() {
                        break;
                    }

                    let last_sig = batch
                        .last()
                        .and_then(|s| Signature::from_str(&s.signature).ok());
                    signatures.extend(batch);

                    if let Some(sig) = last_sig {
                        before = Some(sig);
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    retries += 1;
                    if retries >= max_retries {
                        eprintln!("      Failed after {} retries: {}", max_retries, e);
                        break;
                    }
                    eprintln!("      Retry {}/{}: {}", retries, max_retries, e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            }

            // Rate limiting
            sleep(Duration::from_millis(constants::RPC_SIGNATURE_DELAY_MS)).await;
        }

        println!("      Found {} signatures", signatures.len());

        // Parse each transaction for SOL transfers
        let mut processed = 0;
        let mut transfers_found = 0;
        let mut decode_failures = 0;

        for sig_info in &signatures {
            if let Ok(sig) = Signature::from_str(&sig_info.signature) {
                sleep(Duration::from_millis(constants::RPC_TRANSACTION_DELAY_MS)).await;

                let tx_config = RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::JsonParsed),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                };

                // Retry logic for getting transaction
                for retry in 0..3 {
                    match client.get_transaction_with_config(&sig, tx_config) {
                        Ok(tx) => {
                            match parse_sol_transfers_debug(
                                &tx,
                                &sig_info.signature,
                                config,
                                verbose && processed < 5,
                            ) {
                                Some(transfers) => {
                                    transfers_found += transfers.len();
                                    all_transfers.extend(transfers);
                                }
                                None => {
                                    // Check if it was a decode failure
                                    if tx.transaction.transaction.decode().is_none() {
                                        decode_failures += 1;
                                    }
                                }
                            }
                            break;
                        }
                        Err(_) if retry < 2 => {
                            sleep(Duration::from_secs(1)).await;
                        }
                        Err(_) => {
                            // Transaction might be too old or pruned
                            break;
                        }
                    }
                }
            }

            processed += 1;
            if verbose && processed % 50 == 0 {
                println!(
                    "      Processed {}/{} transactions, {} transfers found",
                    processed,
                    signatures.len(),
                    transfers_found
                );
            }
        }
        if decode_failures > 0 {
            println!(
                "      ({} versioned/undecodable transactions skipped)",
                decode_failures
            );
        }
        println!(
            "      Found {} SOL transfers in {} transactions",
            transfers_found, processed
        );
    }

    // Deduplicate (transfers might appear in both account histories)
    // Use composite key: (signature, from, to, amount) since one tx can have multiple transfers
    all_transfers.sort_by(|a, b| {
        (&a.signature, &a.from, &a.to, a.amount_lamports).cmp(&(
            &b.signature,
            &b.from,
            &b.to,
            b.amount_lamports,
        ))
    });
    all_transfers.dedup_by(|a, b| {
        a.signature == b.signature
            && a.from == b.from
            && a.to == b.to
            && a.amount_lamports == b.amount_lamports
    });

    // Sort by timestamp (oldest first)
    all_transfers.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    Ok(all_transfers)
}

/// Result of fetching transfers for an account
pub struct FetchTransfersResult {
    /// The SOL transfers found
    pub transfers: Vec<SolTransfer>,
    /// The highest slot we saw (for progress tracking even if no transfers found)
    pub highest_slot_seen: Option<u64>,
}

/// Fetch transfers for a single account, stopping at a specific slot
/// Returns transfers and the highest slot seen (for progress tracking)
pub async fn fetch_transfers_for_account(
    config: &Config,
    account: &Pubkey,
    label: &str,
    stop_at_slot: Option<u64>,
    verbose: bool,
) -> Result<FetchTransfersResult> {
    let client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());

    println!(
        "    Fetching transactions for {} ({})...",
        label,
        &account.to_string()[..8]
    );

    let mut signatures = Vec::new();
    let mut before: Option<Signature> = None;
    let max_signatures = constants::MAX_SIGNATURES_PER_ACCOUNT;
    let mut stopped_at_cached = false;
    let mut highest_slot_seen: Option<u64> = None;

    // Paginate through signatures with retry logic
    let mut retries = 0;
    let max_retries = 3;

    loop {
        if signatures.len() >= max_signatures {
            println!("      Reached limit of {} signatures", max_signatures);
            break;
        }

        let sig_config = GetConfirmedSignaturesForAddress2Config {
            before,
            until: None,
            limit: Some(100),
            commitment: Some(CommitmentConfig::confirmed()),
        };

        match client.get_signatures_for_address_with_config(account, sig_config) {
            Ok(batch) => {
                retries = 0;

                if batch.is_empty() {
                    break;
                }

                // Track the highest slot seen (first signature in batch is most recent)
                if let Some(first) = batch.first() {
                    highest_slot_seen =
                        Some(highest_slot_seen.map_or(first.slot, |h| h.max(first.slot)));
                }

                // Check if we've hit cached data
                if let Some(stop_slot) = stop_at_slot {
                    let mut should_stop = false;
                    for sig_info in &batch {
                        if sig_info.slot <= stop_slot {
                            should_stop = true;
                            stopped_at_cached = true;
                            break;
                        }
                        signatures.push(sig_info.clone());
                    }
                    if should_stop {
                        break;
                    }
                } else {
                    signatures.extend(batch.clone());
                }

                let last_sig = batch
                    .last()
                    .and_then(|s| Signature::from_str(&s.signature).ok());

                if let Some(sig) = last_sig {
                    before = Some(sig);
                } else {
                    break;
                }
            }
            Err(e) => {
                retries += 1;
                if retries >= max_retries {
                    eprintln!("      Failed after {} retries: {}", max_retries, e);
                    break;
                }
                eprintln!("      Retry {}/{}: {}", retries, max_retries, e);
                sleep(Duration::from_secs(2)).await;
                continue;
            }
        }

        sleep(Duration::from_millis(constants::RPC_SIGNATURE_DELAY_MS)).await;
    }

    if stopped_at_cached {
        println!(
            "      Found {} new signatures (stopped at cached data)",
            signatures.len()
        );
    } else {
        println!("      Found {} signatures", signatures.len());
    }

    if signatures.is_empty() {
        return Ok(FetchTransfersResult {
            transfers: Vec::new(),
            highest_slot_seen,
        });
    }

    // Parse each transaction for SOL transfers
    let mut transfers = Vec::new();
    let mut processed = 0;
    let mut transfers_found = 0;
    let mut decode_failures = 0;

    for sig_info in &signatures {
        if let Ok(sig) = Signature::from_str(&sig_info.signature) {
            sleep(Duration::from_millis(constants::RPC_TRANSACTION_DELAY_MS)).await;

            let tx_config = RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::JsonParsed),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            };

            for retry in 0..3 {
                match client.get_transaction_with_config(&sig, tx_config) {
                    Ok(tx) => {
                        match parse_sol_transfers_debug(
                            &tx,
                            &sig_info.signature,
                            config,
                            verbose && processed < 5,
                        ) {
                            Some(t) => {
                                transfers_found += t.len();
                                transfers.extend(t);
                            }
                            None => {
                                if tx.transaction.transaction.decode().is_none() {
                                    decode_failures += 1;
                                }
                            }
                        }
                        break;
                    }
                    Err(e) => {
                        if retry == 2 {
                            eprintln!(
                                "      Failed to fetch tx {}: {}",
                                &sig_info.signature[..16],
                                e
                            );
                        }
                        sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        processed += 1;
        if verbose && processed % 50 == 0 {
            println!(
                "      Processed {}/{} transactions, {} transfers found",
                processed,
                signatures.len(),
                transfers_found
            );
        }
    }

    if decode_failures > 0 {
        println!(
            "      ({} versioned/undecodable transactions skipped)",
            decode_failures
        );
    }
    println!(
        "      Found {} SOL transfers in {} transactions",
        transfers_found, processed
    );

    Ok(FetchTransfersResult {
        transfers,
        highest_slot_seen,
    })
}

/// Get the accounts we fetch transactions for (for caching purposes)
/// Note: We don't include vote_account/identity because they don't have SOL transfers
/// (they're all vote transactions). Transfers involving them will be captured when
/// we query the other accounts' histories.
pub fn get_tracked_accounts(config: &Config) -> Vec<(&'static str, Pubkey)> {
    let sfdp_address =
        Pubkey::from_str(constants::SFDP_REIMBURSEMENT).expect("Invalid SFDP address");

    vec![
        ("withdraw_authority", config.withdraw_authority),
        ("personal_wallet", config.personal_wallet),
        ("sfdp_reimbursement", sfdp_address),
    ]
}

/// Parse SOL transfers from a transaction with optional debug output
fn parse_sol_transfers_debug(
    tx: &EncodedConfirmedTransactionWithStatusMeta,
    signature: &str,
    config: &Config,
    debug: bool,
) -> Option<Vec<SolTransfer>> {
    let meta = tx.transaction.meta.as_ref()?;

    // Extract account keys - works for both legacy and versioned transactions
    let account_keys: Vec<Pubkey> = extract_account_keys(&tx.transaction.transaction, debug)?;

    let pre_balances = &meta.pre_balances;
    let post_balances = &meta.post_balances;

    if debug {
        println!(
            "        DEBUG tx {}: {} accounts, {} pre, {} post",
            &signature[..16],
            account_keys.len(),
            pre_balances.len(),
            post_balances.len()
        );
    }

    let timestamp = tx.block_time;
    let date = timestamp.map(|ts| {
        DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    });

    let mut transfers = Vec::new();

    // Look for significant balance changes (> 0.001 SOL)
    for i in 0..account_keys
        .len()
        .min(pre_balances.len())
        .min(post_balances.len())
    {
        let pre = pre_balances[i];
        let post = post_balances[i];
        let diff = post as i64 - pre as i64;

        let account = &account_keys[i];
        let is_relevant = config.is_relevant_account(account);

        // Debug: show significant balance changes
        if debug && diff.abs() >= constants::MIN_TRANSFER_LAMPORTS {
            println!(
                "          Account {}: {} -> {} (diff: {:.4} SOL, relevant: {})",
                &account.to_string()[..8],
                pre,
                post,
                diff as f64 / 1e9,
                is_relevant
            );
        }

        // Only care about our relevant accounts and significant changes
        if !is_relevant {
            continue;
        }

        if diff.abs() < constants::MIN_TRANSFER_LAMPORTS {
            // Less than 0.001 SOL, likely just fees
            continue;
        }

        // Find the counterparty
        for j in 0..account_keys
            .len()
            .min(pre_balances.len())
            .min(post_balances.len())
        {
            if i == j {
                continue;
            }

            let other_pre = pre_balances[j];
            let other_post = post_balances[j];
            let other_diff = other_post as i64 - other_pre as i64;

            // Look for matching opposite change
            if (diff > 0 && other_diff < 0) || (diff < 0 && other_diff > 0) {
                let (from, to, amount) = if diff > 0 {
                    (&account_keys[j], account, diff as u64)
                } else {
                    (account, &account_keys[j], (-diff) as u64)
                };

                let from_label = addresses::get_label(from);
                let to_label = addresses::get_label(to);

                transfers.push(SolTransfer {
                    signature: signature.to_string(),
                    slot: tx.slot,
                    timestamp,
                    date: date.clone(),
                    from: *from,
                    to: *to,
                    amount_lamports: amount,
                    amount_sol: amount as f64 / 1e9,
                    from_label: from_label.name.clone(),
                    to_label: to_label.name.clone(),
                    from_category: from_label.category,
                    to_category: to_label.category,
                });

                break; // Found the counterparty
            }
        }
    }

    if transfers.is_empty() {
        None
    } else {
        Some(transfers)
    }
}

/// Categorize transfers based on sender/receiver
pub fn categorize_transfers(transfers: &[SolTransfer], config: &Config) -> CategorizedTransfers {
    let mut categorized = CategorizedTransfers::default();

    for transfer in transfers {
        // Check if this is incoming to our accounts
        let is_incoming = config.is_our_account(&transfer.to);
        let is_outgoing = config.is_our_account(&transfer.from);

        if is_incoming {
            // Categorize incoming transfers
            if transfer.from == config.personal_wallet {
                // From personal wallet = seeding
                categorized.seeding.push(transfer.clone());
            } else if addresses::is_solana_foundation(&transfer.from) {
                // From SF = SFDP reimbursement
                categorized.sfdp_reimbursements.push(transfer.clone());
            } else if addresses::is_jito(&transfer.from) {
                // From Jito = MEV deposit
                categorized.mev_deposits.push(transfer.clone());
            } else if config.is_our_account(&transfer.from) {
                // Internal transfer (identity -> vote account for funding)
                categorized.vote_funding.push(transfer.clone());
            } else {
                categorized.other.push(transfer.clone());
            }
        } else if is_outgoing {
            // Outgoing transfers
            if addresses::is_exchange(&transfer.to) || transfer.to == config.personal_wallet {
                categorized.withdrawals.push(transfer.clone());
            } else if config.is_our_account(&transfer.to) {
                // Internal transfer
                categorized.vote_funding.push(transfer.clone());
            } else {
                categorized.other.push(transfer.clone());
            }
        }
    }

    categorized
}

/// Convert epoch number to approximate date
/// Calibrated: epoch 896 = 2025-12-16
pub fn epoch_to_date(epoch: u64) -> String {
    // Use saturating arithmetic to prevent overflow with extreme epoch values
    let epoch_i64 = epoch.min(i64::MAX as u64) as i64;
    let epoch_diff = epoch_i64.saturating_sub(constants::REFERENCE_EPOCH);
    let duration = epoch_diff.saturating_mul(constants::EPOCH_DURATION_SECONDS);
    let timestamp = constants::REFERENCE_EPOCH_TIMESTAMP.saturating_add(duration);

    DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_to_date() {
        assert_eq!(epoch_to_date(896), "2025-12-16");
        assert_eq!(epoch_to_date(900), "2025-12-24");
        assert_eq!(epoch_to_date(904), "2026-01-01");
    }
}
