//! Dune Analytics API integration for backfilling historical data
//!
//! When RPC data is pruned, we can query Dune Analytics to recover:
//! - Inflation rewards (vote account commission)
//! - Leader slot fees (identity account)
//! - Vote transaction costs (identity account)
//! - SOL transfers (all tracked accounts)
//!
//! API docs: https://docs.dune.com/api-reference/executions/endpoint/execute-query

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;

use crate::addresses::get_label;
use crate::config::Config;
use crate::constants;
use crate::leader_fees::EpochLeaderFees;
use crate::transactions::{epoch_to_date, SolTransfer};
use crate::vote_costs::EpochVoteCost;

/// Dune API base URL
const DUNE_API_BASE: &str = "https://api.dune.com/api/v1";

/// Timeout for query execution (5 minutes)
const QUERY_TIMEOUT_SECS: u64 = 300;

/// Poll interval for checking query status
const POLL_INTERVAL_SECS: u64 = 3;

/// Initial delay before first poll
const INITIAL_DELAY_SECS: u64 = 5;

// =============================================================================
// API Types
// =============================================================================

/// Request body for executing a SQL query
#[derive(Serialize)]
struct ExecuteRequest {
    sql: String,
    performance: String,
}

/// Response from query execution
#[derive(Deserialize)]
struct ExecuteResponse {
    execution_id: String,
}

/// Response from getting query results
#[derive(Deserialize)]
struct ResultsResponse {
    state: String,
    result: Option<QueryResult>,
    error: Option<String>,
}

/// Query result data
#[derive(Deserialize)]
struct QueryResult {
    rows: Vec<HashMap<String, serde_json::Value>>,
}

// =============================================================================
// Dune Client
// =============================================================================

/// Dune Analytics API client
pub struct DuneClient {
    api_key: String,
    client: reqwest::Client,
    /// Vote account address (for inflation rewards queries)
    vote_account: String,
    /// Identity address (for leader fees and vote costs queries)
    identity: String,
    /// Withdraw authority address (for transfer queries)
    withdraw_authority: String,
    /// Commission percentage (for reward records)
    commission_percent: u8,
}

impl DuneClient {
    /// Create a new Dune client with validator configuration
    pub fn new(api_key: String, config: &Config) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
            vote_account: config.vote_account.to_string(),
            identity: config.identity.to_string(),
            withdraw_authority: config.withdraw_authority.to_string(),
            commission_percent: config.commission_percent,
        }
    }

    /// Execute a SQL query and wait for results
    async fn execute_query(&self, sql: &str) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        // Submit query
        let execute_url = format!("{}/sql/execute", DUNE_API_BASE);
        let request = ExecuteRequest {
            sql: sql.to_string(),
            performance: "medium".to_string(),
        };

        let response: ExecuteResponse = self
            .client
            .post(&execute_url)
            .header("X-Dune-Api-Key", &self.api_key)
            .json(&request)
            .send()
            .await
            .context("Failed to submit Dune query")?
            .json()
            .await
            .context("Failed to parse Dune execute response")?;

        let execution_id = response.execution_id;
        println!("    Query submitted (execution_id: {})", execution_id);

        // Wait for initial processing
        sleep(Duration::from_secs(INITIAL_DELAY_SECS)).await;

        // Poll for results
        let results_url = format!("{}/execution/{}/results", DUNE_API_BASE, execution_id);
        let timeout = Duration::from_secs(QUERY_TIMEOUT_SECS);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                anyhow::bail!("Query timed out after {} seconds", QUERY_TIMEOUT_SECS);
            }

            let response: ResultsResponse = self
                .client
                .get(&results_url)
                .header("X-Dune-Api-Key", &self.api_key)
                .send()
                .await
                .context("Failed to get Dune results")?
                .json()
                .await
                .context("Failed to parse Dune results response")?;

            match response.state.as_str() {
                "QUERY_STATE_COMPLETED" => {
                    if let Some(result) = response.result {
                        return Ok(result.rows);
                    }
                    return Ok(Vec::new());
                }
                "QUERY_STATE_FAILED" => {
                    let error = response
                        .error
                        .unwrap_or_else(|| "Unknown error".to_string());
                    anyhow::bail!("Query failed: {}", error);
                }
                state => {
                    print!("    Status: {}...\r", state);
                    sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                }
            }
        }
    }

    // =========================================================================
    // Inflation Rewards
    // =========================================================================

    /// Fetch inflation rewards from Dune
    ///
    /// Queries the solana.rewards table for Voting rewards to the vote account.
    /// This captures the commission earned on staking rewards.
    pub async fn fetch_inflation_rewards(
        &self,
        start_date: &str,
    ) -> Result<Vec<crate::transactions::EpochReward>> {
        println!("  Querying Dune for inflation rewards...");

        let sql = format!(
            r#"
            SELECT
              FLOOR(block_slot / 432000) as epoch,
              SUM(lamports) / 1e9 as reward_sol,
              MIN(block_time) as reward_time
            FROM solana.rewards
            WHERE reward_type = 'Voting'
              AND recipient = '{}'
              AND block_date >= DATE '{}'
            GROUP BY FLOOR(block_slot / 432000)
            ORDER BY epoch
            "#,
            self.vote_account, start_date
        );

        let rows = self.execute_query(&sql).await?;
        println!("    Found {} epochs with rewards", rows.len());

        let mut rewards = Vec::new();
        for row in rows {
            let epoch = get_u64(&row, "epoch")?;
            let reward_sol = get_f64(&row, "reward_sol")?;
            let reward_lamports = (reward_sol * 1e9) as u64;

            rewards.push(crate::transactions::EpochReward {
                epoch,
                effective_slot: epoch * constants::SLOTS_PER_EPOCH, // Approximate
                amount_lamports: reward_lamports,
                amount_sol: reward_sol,
                commission: self.commission_percent,
                date: Some(epoch_to_date(epoch)),
            });
        }

        Ok(rewards)
    }

    // =========================================================================
    // Leader Fees
    // =========================================================================

    /// Fetch leader slot fees from Dune
    ///
    /// Queries the solana.rewards table for Fee rewards to the identity account.
    /// This captures transaction fees earned when producing blocks as leader.
    pub async fn fetch_leader_fees(&self, start_date: &str) -> Result<Vec<EpochLeaderFees>> {
        println!("  Querying Dune for leader fees...");

        let sql = format!(
            r#"
            SELECT
              FLOOR(block_slot / 432000) as epoch,
              COUNT(*) as blocks_produced,
              SUM(lamports) / 1e9 as total_fees_sol
            FROM solana.rewards
            WHERE reward_type = 'Fee'
              AND recipient = '{}'
              AND block_date >= DATE '{}'
            GROUP BY FLOOR(block_slot / 432000)
            ORDER BY epoch
            "#,
            self.identity, start_date
        );

        let rows = self.execute_query(&sql).await?;
        println!("    Found {} epochs with leader fees", rows.len());

        let mut fees = Vec::new();
        for row in rows {
            let epoch = get_u64(&row, "epoch")?;
            let blocks_produced = get_u64(&row, "blocks_produced")?;
            let total_fees_sol = get_f64(&row, "total_fees_sol")?;
            let total_fees_lamports = (total_fees_sol * 1e9) as u64;

            fees.push(EpochLeaderFees {
                epoch,
                leader_slots: blocks_produced, // We only have blocks, not assigned slots
                blocks_produced,
                skipped_slots: 0, // Can't determine from rewards table
                total_fees_lamports,
                total_fees_sol,
                date: Some(epoch_to_date(epoch)),
            });
        }

        Ok(fees)
    }

    // =========================================================================
    // Vote Costs
    // =========================================================================

    /// Fetch vote transaction costs from Dune
    ///
    /// Queries the solana.vote_transactions table for votes signed by identity.
    pub async fn fetch_vote_costs(&self, start_date: &str) -> Result<Vec<EpochVoteCost>> {
        println!("  Querying Dune for vote costs...");

        let sql = format!(
            r#"
            SELECT
              FLOOR(block_slot / 432000) as epoch,
              COUNT(*) as vote_count,
              SUM(fee) / 1e9 as total_fee_sol
            FROM solana.vote_transactions
            WHERE signer = '{}'
              AND block_date >= DATE '{}'
            GROUP BY FLOOR(block_slot / 432000)
            ORDER BY epoch
            "#,
            self.identity, start_date
        );

        let rows = self.execute_query(&sql).await?;
        println!("    Found {} epochs with vote costs", rows.len());

        let mut costs = Vec::new();
        for row in rows {
            let epoch = get_u64(&row, "epoch")?;
            let vote_count = get_u64(&row, "vote_count")?;
            let total_fee_sol = get_f64(&row, "total_fee_sol")?;
            let total_fee_lamports = (total_fee_sol * 1e9) as u64;

            costs.push(EpochVoteCost {
                epoch,
                vote_count,
                total_fee_lamports,
                total_fee_sol,
                source: "dune".to_string(),
                date: Some(epoch_to_date(epoch)),
            });
        }

        Ok(costs)
    }

    // =========================================================================
    // SOL Transfers
    // =========================================================================

    /// Fetch SOL transfers from Dune
    ///
    /// Queries the tokens_solana.transfers table for native SOL transfers
    /// involving any of our tracked accounts.
    pub async fn fetch_transfers(&self, start_date: &str) -> Result<Vec<SolTransfer>> {
        println!("  Querying Dune for SOL transfers...");

        // Build the account list from config
        let accounts = [
            self.identity.as_str(),
            self.withdraw_authority.as_str(),
            self.vote_account.as_str(),
        ];
        let account_list = accounts
            .iter()
            .map(|a| format!("'{}'", a))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            r#"
            SELECT
              block_date,
              block_slot,
              FLOOR(block_slot / 432000) as epoch,
              from_owner,
              to_owner,
              amount_display as amount_sol,
              tx_id as signature,
              block_time
            FROM tokens_solana.transfers
            WHERE token_mint_address = 'So11111111111111111111111111111111111111111'
              AND block_date >= DATE '{}'
              AND (
                from_owner IN ({})
                OR to_owner IN ({})
              )
            ORDER BY block_slot DESC
            LIMIT 1000
            "#,
            start_date, account_list, account_list
        );

        let rows = self.execute_query(&sql).await?;
        println!("    Found {} transfers", rows.len());

        let mut transfers = Vec::new();
        for row in rows {
            let slot = get_u64(&row, "block_slot")?;
            let from_str = get_string(&row, "from_owner")?;
            let to_str = get_string(&row, "to_owner")?;
            let amount_sol = get_f64(&row, "amount_sol")?;
            let signature = get_string(&row, "signature")?;
            let date = get_string_opt(&row, "block_date");
            let timestamp = get_timestamp_opt(&row, "block_time");

            // Skip tiny transfers (dust)
            let amount_lamports = (amount_sol * 1e9) as u64;
            if (amount_lamports as i64) < constants::MIN_TRANSFER_LAMPORTS {
                continue;
            }

            // Parse pubkeys
            let from = Pubkey::from_str(&from_str).unwrap_or_default();
            let to = Pubkey::from_str(&to_str).unwrap_or_default();

            // Label the addresses using the addresses module
            let from_label_info = get_label(&from);
            let to_label_info = get_label(&to);

            transfers.push(SolTransfer {
                signature,
                slot,
                timestamp,
                date,
                from,
                to,
                amount_lamports,
                amount_sol,
                from_label: from_label_info.name,
                to_label: to_label_info.name,
                from_category: from_label_info.category,
                to_category: to_label_info.category,
            });
        }

        Ok(transfers)
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Extract u64 from JSON value
fn get_u64(row: &HashMap<String, serde_json::Value>, key: &str) -> Result<u64> {
    row.get(key)
        .and_then(|v| v.as_f64())
        .map(|f| f as u64)
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid field: {}", key))
}

/// Extract f64 from JSON value
fn get_f64(row: &HashMap<String, serde_json::Value>, key: &str) -> Result<f64> {
    row.get(key)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid field: {}", key))
}

/// Extract string from JSON value
fn get_string(row: &HashMap<String, serde_json::Value>, key: &str) -> Result<String> {
    row.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid field: {}", key))
}

/// Extract optional string from JSON value
fn get_string_opt(row: &HashMap<String, serde_json::Value>, key: &str) -> Option<String> {
    row.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Extract optional timestamp from JSON value
fn get_timestamp_opt(row: &HashMap<String, serde_json::Value>, key: &str) -> Option<i64> {
    row.get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
}
