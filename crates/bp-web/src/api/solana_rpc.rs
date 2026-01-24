use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

const RPC_ENDPOINT: &str = "https://api.mainnet-beta.solana.com";

/// Network comparison stats for a validator
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NetworkComparison {
    pub total_validators: usize,
    pub skip_rate_percentile: u8,
    pub vote_success_percentile: u8,
    pub stake_percentile: u8,
    pub network_avg_skip_rate: f64,
    pub network_avg_vote_success: f64,
}

#[derive(Serialize)]
struct RpcRequest {
    jsonrpc: &'static str,
    id: u32,
    method: &'static str,
    params: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RpcResponse {
    result: Option<VoteAccountsResult>,
}

#[derive(Deserialize)]
struct VoteAccountsResult {
    current: Vec<VoteAccount>,
}

#[derive(Deserialize)]
struct VoteAccount {
    #[serde(rename = "activatedStake")]
    activated_stake: u64,
}

/// Fetch network comparison data using getVoteAccounts
pub async fn get_network_comparison(
    current_skip_rate: f64,
    _current_vote_success: f64,
    current_stake: f64,
) -> Option<NetworkComparison> {
    let request = RpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "getVoteAccounts",
        params: vec![serde_json::json!({"commitment": "confirmed"})],
    };

    let body = serde_json::to_string(&request).ok()?;

    let response = Request::post(RPC_ENDPOINT)
        .header("Content-Type", "application/json")
        .body(body)
        .ok()?
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("RPC error: {}", response.status()).into());
        return None;
    }

    let data: RpcResponse = response.json().await.ok()?;
    let validators = data.result?.current;
    let total_validators = validators.len();

    if total_validators == 0 {
        return None;
    }

    // Calculate stake percentile
    let mut stakes: Vec<u64> = validators.iter().map(|v| v.activated_stake).collect();
    stakes.sort_by(|a, b| b.cmp(a)); // Sort descending

    let current_stake_lamports = (current_stake * 1_000_000_000.0) as u64;
    let stake_rank = stakes
        .iter()
        .position(|&s| s <= current_stake_lamports)
        .unwrap_or(total_validators)
        + 1;
    let stake_percentile = ((stake_rank as f64 / total_validators as f64) * 100.0).round() as u8;

    // Network averages (typical values)
    let network_avg_skip_rate = 0.2;
    let network_avg_vote_success = 99.5;

    // Estimate skip rate percentile
    let skip_rate_percentile = if current_skip_rate <= network_avg_skip_rate {
        ((1.0 - (current_skip_rate / network_avg_skip_rate) * 0.5) * 50.0).round() as u8
    } else {
        (50.0 + (current_skip_rate / network_avg_skip_rate - 1.0) * 50.0).round() as u8
    };

    Some(NetworkComparison {
        total_validators,
        skip_rate_percentile: skip_rate_percentile.clamp(1, 100),
        vote_success_percentile: 10, // Placeholder - would need more data
        stake_percentile: stake_percentile.clamp(1, 100),
        network_avg_skip_rate,
        network_avg_vote_success,
    })
}
