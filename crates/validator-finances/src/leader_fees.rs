//! Leader slot fee tracking
//!
//! Validators earn transaction fees when they produce blocks as the leader.
//! These fees go to the identity account, not the vote account.
//! - 50% of base fees (other 50% is burned)
//! - 100% of priority fees

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

use crate::config::Config;
use crate::constants;
use crate::transactions::epoch_to_date;

/// Historical leader slot data from Dune Analytics JSON export
#[derive(Debug, Deserialize)]
pub struct HistoricalLeaderSlots {
    pub epochs: HashMap<String, EpochSlotInfo>,
    pub slots_by_epoch: HashMap<String, Vec<u64>>,
}

/// Per-epoch slot information from Dune export
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields from JSON may not all be used
pub struct EpochSlotInfo {
    pub slot_count: u64,
    #[serde(default)]
    first_slot: Option<u64>,
    #[serde(default)]
    last_slot: Option<u64>,
    #[serde(default)]
    note: Option<String>,
}

/// Leader fee revenue for a single epoch
#[derive(Debug, Clone)]
pub struct EpochLeaderFees {
    pub epoch: u64,
    pub leader_slots: u64,
    pub blocks_produced: u64,
    pub skipped_slots: u64,
    #[allow(dead_code)]
    pub total_fees_lamports: u64,
    pub total_fees_sol: f64,
    pub date: Option<String>,
}

/// RPC response for getLeaderSchedule
#[derive(Debug, Deserialize)]
struct LeaderScheduleResponse {
    result: Option<HashMap<String, Vec<u64>>>,
}

/// RPC response for getBlock
#[derive(Debug, Deserialize)]
struct BlockResponse {
    result: Option<BlockResult>,
}

#[derive(Debug, Deserialize)]
struct BlockResult {
    rewards: Option<Vec<BlockReward>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlockReward {
    pubkey: String,
    lamports: i64,
    reward_type: Option<String>,
}

/// Fetch leader fees for a range of epochs
pub async fn fetch_leader_fees(
    config: &Config,
    start_epoch: u64,
    end_epoch: Option<u64>,
) -> Result<Vec<EpochLeaderFees>> {
    let client = reqwest::Client::new();

    // Get current epoch
    let current_epoch = get_current_epoch(&client, &config.rpc_url).await?;
    let end = end_epoch.unwrap_or(current_epoch);

    let epoch_word = if start_epoch == end {
        "epoch"
    } else {
        "epochs"
    };
    println!(
        "    Fetching leader fees for {} {}-{}...",
        epoch_word, start_epoch, end
    );

    let mut all_fees = Vec::new();

    for epoch in start_epoch..=end {
        match fetch_epoch_leader_fees(&client, config, epoch).await {
            Ok(fees) => {
                if fees.leader_slots > 0 {
                    println!(
                        "      Epoch {}: {} slots, {} blocks, {:.4} SOL",
                        epoch, fees.leader_slots, fees.blocks_produced, fees.total_fees_sol
                    );
                    all_fees.push(fees);
                }
            }
            Err(e) => {
                eprintln!("      Epoch {}: Error - {}", epoch, e);
            }
        }

        // Rate limiting between epochs
        sleep(Duration::from_millis(100)).await;
    }

    Ok(all_fees)
}

/// Fetch leader fees for a single epoch
async fn fetch_epoch_leader_fees(
    client: &reqwest::Client,
    config: &Config,
    epoch: u64,
) -> Result<EpochLeaderFees> {
    let epoch_start_slot = epoch * constants::SLOTS_PER_EPOCH;
    let identity = config.identity.to_string();

    // Get leader schedule for this epoch
    let leader_slots = get_leader_schedule(client, &config.rpc_url, epoch_start_slot, &identity)
        .await
        .context("Failed to get leader schedule")?;

    if leader_slots.is_empty() {
        return Ok(EpochLeaderFees {
            epoch,
            leader_slots: 0,
            blocks_produced: 0,
            skipped_slots: 0,
            total_fees_lamports: 0,
            total_fees_sol: 0.0,
            date: Some(epoch_to_date(epoch)),
        });
    }

    // Convert slot offsets to absolute slots
    let absolute_slots: Vec<u64> = leader_slots
        .iter()
        .map(|offset| epoch_start_slot + offset)
        .collect();

    // Fetch block rewards for each leader slot
    let mut total_fees: u64 = 0;
    let mut blocks_produced: u64 = 0;

    for slot in &absolute_slots {
        match get_block_fee_reward(client, &config.rpc_url, *slot, &identity).await {
            Ok(Some(fee)) => {
                total_fees += fee;
                blocks_produced += 1;
            }
            Ok(None) => {
                // Block was skipped or no fee reward
            }
            Err(_) => {
                // Block data unavailable (pruned or skipped)
            }
        }

        // Rate limiting - be gentle with the RPC
        sleep(Duration::from_millis(constants::BLOCK_FETCH_DELAY_MS)).await;
    }

    let skipped = absolute_slots.len() as u64 - blocks_produced;

    Ok(EpochLeaderFees {
        epoch,
        leader_slots: absolute_slots.len() as u64,
        blocks_produced,
        skipped_slots: skipped,
        total_fees_lamports: total_fees,
        total_fees_sol: total_fees as f64 / 1e9,
        date: Some(epoch_to_date(epoch)),
    })
}

/// Get current epoch from RPC
async fn get_current_epoch(client: &reqwest::Client, rpc_url: &str) -> Result<u64> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getEpochInfo",
        "params": []
    });

    let response: serde_json::Value = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;

    response["result"]["epoch"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Failed to get current epoch"))
}

/// Get leader schedule for a specific epoch
async fn get_leader_schedule(
    client: &reqwest::Client,
    rpc_url: &str,
    epoch_start_slot: u64,
    identity: &str,
) -> Result<Vec<u64>> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLeaderSchedule",
        "params": [
            epoch_start_slot + 1,
            {"identity": identity}
        ]
    });

    let response: LeaderScheduleResponse = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;

    Ok(response
        .result
        .and_then(|map| map.get(identity).cloned())
        .unwrap_or_default())
}

/// Get fee reward from a specific block
async fn get_block_fee_reward(
    client: &reqwest::Client,
    rpc_url: &str,
    slot: u64,
    identity: &str,
) -> Result<Option<u64>> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getBlock",
        "params": [
            slot,
            {
                "rewards": true,
                "maxSupportedTransactionVersion": 0,
                "transactionDetails": "none"
            }
        ]
    });

    let response: BlockResponse = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;

    if let Some(result) = response.result {
        if let Some(rewards) = result.rewards {
            for reward in rewards {
                if reward.pubkey == identity
                    && reward.reward_type.as_deref() == Some("Fee")
                    && reward.lamports > 0
                {
                    return Ok(Some(reward.lamports as u64));
                }
            }
        }
    }

    Ok(None)
}

/// Get total leader fees in SOL
pub fn total_leader_fees_sol(fees: &[EpochLeaderFees]) -> f64 {
    fees.iter().map(|f| f.total_fees_sol).sum()
}

// =============================================================================
// Historical Slot Import (from Dune Analytics)
// =============================================================================

/// Load historical leader slot data from a JSON file (exported from Dune Analytics)
pub fn load_historical_slots(path: &Path) -> Result<HistoricalLeaderSlots> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    serde_json::from_str(&content).with_context(|| "Failed to parse historical leader slots JSON")
}

/// Fetch leader fees for specific slots (used for historical data import)
///
/// This queries each slot individually to get the fee reward. Useful when we
/// have slot numbers from external sources (e.g., Dune Analytics) but need
/// to get the actual fee amounts from RPC.
pub async fn fetch_fees_for_slots(
    config: &Config,
    epoch: u64,
    slots: &[u64],
) -> Result<EpochLeaderFees> {
    let client = reqwest::Client::new();
    let identity = config.identity.to_string();

    let mut total_fees: u64 = 0;
    let mut blocks_produced: u64 = 0;
    let mut skipped: u64 = 0;
    let mut unavailable: u64 = 0;

    println!(
        "      Fetching {} slots for epoch {}...",
        slots.len(),
        epoch
    );

    for (i, slot) in slots.iter().enumerate() {
        match get_block_fee_reward(&client, &config.rpc_url, *slot, &identity).await {
            Ok(Some(fee)) => {
                total_fees += fee;
                blocks_produced += 1;
            }
            Ok(None) => {
                // Block was skipped or no fee reward
                skipped += 1;
            }
            Err(_) => {
                // Block data unavailable (pruned)
                unavailable += 1;
            }
        }

        // Progress indicator every 20 slots
        if (i + 1) % 20 == 0 {
            println!(
                "        Progress: {}/{} slots ({} blocks, {} skipped, {} unavailable)",
                i + 1,
                slots.len(),
                blocks_produced,
                skipped,
                unavailable
            );
        }

        // Rate limiting
        sleep(Duration::from_millis(constants::BLOCK_FETCH_DELAY_MS)).await;
    }

    println!(
        "      Epoch {} complete: {} blocks produced, {:.6} SOL in fees",
        epoch,
        blocks_produced,
        total_fees as f64 / 1e9
    );

    if unavailable > 0 {
        println!(
            "      Warning: {} slots had unavailable/pruned block data",
            unavailable
        );
    }

    Ok(EpochLeaderFees {
        epoch,
        leader_slots: slots.len() as u64,
        blocks_produced,
        skipped_slots: skipped + unavailable,
        total_fees_lamports: total_fees,
        total_fees_sol: total_fees as f64 / 1e9,
        date: Some(epoch_to_date(epoch)),
    })
}

/// Import historical leader fees from Dune Analytics JSON export
///
/// Reads the JSON file containing slot numbers, queries RPC for each slot's
/// fee reward, and returns EpochLeaderFees for caching.
pub async fn import_historical_leader_fees(
    config: &Config,
    json_path: &Path,
) -> Result<Vec<EpochLeaderFees>> {
    let historical = load_historical_slots(json_path)?;

    println!("    Loaded historical slot data:");
    for (epoch_str, info) in &historical.epochs {
        if info.slot_count > 0 {
            println!("      Epoch {}: {} slots", epoch_str, info.slot_count);
        } else if let Some(note) = &info.note {
            println!("      Epoch {}: {} ({})", epoch_str, info.slot_count, note);
        }
    }

    let mut results = Vec::new();

    // Process epochs in order
    let mut epochs: Vec<_> = historical
        .slots_by_epoch
        .keys()
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    epochs.sort();

    for epoch in epochs {
        let epoch_str = epoch.to_string();
        if let Some(slots) = historical.slots_by_epoch.get(&epoch_str) {
            if !slots.is_empty() {
                let fees = fetch_fees_for_slots(config, epoch, slots).await?;
                results.push(fees);
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_slot_calculation() {
        assert_eq!(904 * constants::SLOTS_PER_EPOCH, 390_528_000);
        assert_eq!(912 * constants::SLOTS_PER_EPOCH, 393_984_000);
    }
}
