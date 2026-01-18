//! Vote transaction cost tracking
//!
//! Validators pay transaction fees for every vote they submit.
//! - Each vote transaction costs ~5000 lamports (0.000005 SOL)
//! - A healthy validator submits ~431,000 votes per epoch
//! - Total cost is approximately 2.15 SOL per epoch
//!
//! SFDP (Solana Foundation Delegation Program) reimburses vote costs
//! on a declining schedule over 12 months.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::transactions::epoch_to_date;

/// Vote cost data for a single epoch
#[derive(Debug, Clone)]
pub struct EpochVoteCost {
    pub epoch: u64,
    pub vote_count: u64,
    pub total_fee_lamports: u64,
    pub total_fee_sol: f64,
    /// Source of data: "dune" (imported), "estimated" (calculated), "rpc" (queried)
    pub source: String,
    pub date: Option<String>,
}

// =============================================================================
// Constants
// =============================================================================

/// Cost per vote transaction in lamports
pub const LAMPORTS_PER_VOTE: u64 = 5000;

/// Average votes per epoch for a healthy validator
pub const TYPICAL_VOTES_PER_EPOCH: u64 = 431_000;

/// Average cost per epoch in SOL (for quick estimates)
pub const TYPICAL_COST_PER_EPOCH_SOL: f64 = 2.155;

// =============================================================================
// Historical Import (from Dune Analytics JSON)
// =============================================================================

/// Historical vote cost data from Dune Analytics JSON export
#[derive(Debug, Deserialize)]
pub struct HistoricalVoteCosts {
    pub vote_costs_by_epoch: HashMap<String, EpochVoteCostInfo>,
}

/// Per-epoch vote cost info from Dune export
#[derive(Debug, Deserialize)]
pub struct EpochVoteCostInfo {
    pub vote_count: u64,
    pub total_fee_sol: f64,
    #[serde(default)]
    pub note: Option<String>,
}

/// Load historical vote cost data from a JSON file
pub fn load_historical_vote_costs(path: &Path) -> Result<HistoricalVoteCosts> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    serde_json::from_str(&content)
        .with_context(|| "Failed to parse historical vote costs JSON")
}

/// Import historical vote costs from Dune Analytics JSON export
pub fn import_historical_vote_costs(json_path: &Path) -> Result<Vec<EpochVoteCost>> {
    let historical = load_historical_vote_costs(json_path)?;

    let mut results = Vec::new();

    // Process epochs in order
    let mut epochs: Vec<_> = historical.vote_costs_by_epoch.keys()
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    epochs.sort();

    for epoch in epochs {
        let epoch_str = epoch.to_string();
        if let Some(info) = historical.vote_costs_by_epoch.get(&epoch_str) {
            let total_fee_lamports = (info.total_fee_sol * 1e9) as u64;

            results.push(EpochVoteCost {
                epoch,
                vote_count: info.vote_count,
                total_fee_lamports,
                total_fee_sol: info.total_fee_sol,
                source: "dune".to_string(),
                date: Some(epoch_to_date(epoch)),
            });
        }
    }

    Ok(results)
}

// =============================================================================
// Estimation
// =============================================================================

/// Estimate vote cost for an epoch using typical values
///
/// This provides a reasonable estimate when actual data isn't available.
/// Most validators see very consistent vote counts (~431K per epoch).
pub fn estimate_vote_cost(epoch: u64) -> EpochVoteCost {
    let vote_count = TYPICAL_VOTES_PER_EPOCH;
    let total_fee_lamports = vote_count * LAMPORTS_PER_VOTE;
    let total_fee_sol = total_fee_lamports as f64 / 1e9;

    EpochVoteCost {
        epoch,
        vote_count,
        total_fee_lamports,
        total_fee_sol,
        source: "estimated".to_string(),
        date: Some(epoch_to_date(epoch)),
    }
}

/// Estimate vote costs for a range of epochs
pub fn estimate_vote_costs(start_epoch: u64, end_epoch: u64) -> Vec<EpochVoteCost> {
    (start_epoch..=end_epoch)
        .map(estimate_vote_cost)
        .collect()
}

// =============================================================================
// Utilities
// =============================================================================

/// Get total vote costs in SOL
pub fn total_vote_costs_sol(costs: &[EpochVoteCost]) -> f64 {
    costs.iter().map(|c| c.total_fee_sol).sum()
}

/// Get total vote count
pub fn total_vote_count(costs: &[EpochVoteCost]) -> u64 {
    costs.iter().map(|c| c.vote_count).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_vote_cost() {
        let cost = estimate_vote_cost(900);
        assert_eq!(cost.epoch, 900);
        assert_eq!(cost.vote_count, TYPICAL_VOTES_PER_EPOCH);
        assert_eq!(cost.total_fee_lamports, TYPICAL_VOTES_PER_EPOCH * LAMPORTS_PER_VOTE);
        assert!((cost.total_fee_sol - TYPICAL_COST_PER_EPOCH_SOL).abs() < 0.01);
        assert_eq!(cost.source, "estimated");
    }

    #[test]
    fn test_lamports_calculation() {
        // 431,000 votes * 5000 lamports = 2,155,000,000 lamports = 2.155 SOL
        let votes = 431_000u64;
        let lamports = votes * LAMPORTS_PER_VOTE;
        assert_eq!(lamports, 2_155_000_000);
        assert!((lamports as f64 / 1e9 - 2.155).abs() < 0.001);
    }
}
