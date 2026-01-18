//! Jito MEV tip tracking via Jito API
//!
//! MEV tips are claimed to the vote account by Jito's merkle_root_upload_authority.
//! We query Jito's API to get per-epoch MEV rewards for the validator.

use anyhow::Result;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

use crate::config::Config;
use crate::constants;
use crate::transactions::epoch_to_date;

/// MEV reward for a single epoch
#[derive(Debug, Clone)]
pub struct MevClaim {
    pub epoch: u64,
    pub total_tips_lamports: u64,
    #[allow(dead_code)]
    pub commission_lamports: u64, // 10% of total
    pub amount_sol: f64, // Commission in SOL
    pub date: Option<String>,
}

/// Per-epoch MEV data from Jito API
#[derive(Debug, Deserialize)]
struct JitoEpochData {
    epoch: u64,
    mev_commission_bps: u64,
    mev_rewards: u64, // Total tips in lamports
    #[serde(default)]
    #[allow(dead_code)]
    priority_fee_commission_bps: u64,
    #[serde(default)]
    #[allow(dead_code)]
    priority_fee_rewards: u64,
}

/// Fetch MEV claims from Jito API with retry logic
pub async fn fetch_mev_claims(config: &Config) -> Result<Vec<MevClaim>> {
    let client = reqwest::Client::new();

    let url = format!(
        "{}/validators/{}",
        constants::JITO_API_BASE,
        config.vote_account
    );
    println!("    Querying Jito API...");

    // Retry with exponential backoff
    let max_retries = 3;
    let mut last_error = None;

    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt as u32));
            println!(
                "    Retry {}/{} after {:?}...",
                attempt,
                max_retries - 1,
                delay
            );
            sleep(delay).await;
        }

        match client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    // API returns an array of epoch data directly
                    let epochs: Vec<JitoEpochData> = response.json().await?;
                    return process_jito_epochs(epochs);
                } else if response.status().as_u16() == 429 {
                    // Rate limited - always retry
                    last_error = Some(anyhow::anyhow!("Rate limited (429)"));
                    continue;
                } else {
                    last_error = Some(anyhow::anyhow!(
                        "Jito API returned status: {}",
                        response.status()
                    ));
                }
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("Request failed: {}", e));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed after {} retries", max_retries)))
}

/// Process Jito epoch data into MevClaims
fn process_jito_epochs(epochs: Vec<JitoEpochData>) -> Result<Vec<MevClaim>> {
    println!("    Found {} epochs with MEV data", epochs.len());

    let mut claims = Vec::new();

    for epoch_data in epochs {
        // Validator commission is based on mev_commission_bps (1000 = 10%)
        let commission_rate = epoch_data.mev_commission_bps as f64 / 10000.0;
        let commission_lamports = (epoch_data.mev_rewards as f64 * commission_rate) as u64;
        let amount_sol = commission_lamports as f64 / 1e9;
        let date = epoch_to_date(epoch_data.epoch);

        claims.push(MevClaim {
            epoch: epoch_data.epoch,
            total_tips_lamports: epoch_data.mev_rewards,
            commission_lamports,
            amount_sol,
            date: Some(date),
        });

        println!(
            "      Epoch {}: {:.4} SOL tips -> {:.4} SOL commission ({}%)",
            epoch_data.epoch,
            epoch_data.mev_rewards as f64 / 1e9,
            amount_sol,
            epoch_data.mev_commission_bps / 100
        );
    }

    // Sort by epoch
    claims.sort_by(|a, b| a.epoch.cmp(&b.epoch));

    Ok(claims)
}

/// Get total MEV commission in SOL
pub fn total_mev_sol(claims: &[MevClaim]) -> f64 {
    claims.iter().map(|c| c.amount_sol).sum()
}
