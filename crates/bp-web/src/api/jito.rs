use gloo_net::http::Request;
use serde::Deserialize;
use shared::CONFIG;

const JITO_API_BASE: &str = "https://kobe.mainnet.jito.network";

/// Single epoch reward data from Jito
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct JitoEpochReward {
    pub epoch: u64,
    #[serde(default)]
    pub mev_rewards: u64,
    #[serde(alias = "MEV_rewards", default)]
    pub mev_rewards_alt: u64,
    #[serde(default)]
    pub total_rewards: u64,
    #[serde(default)]
    pub mev_commission_earned: u64,
    #[serde(alias = "commission_earned", default)]
    pub commission_earned_alt: u64,
}

impl JitoEpochReward {
    /// Get MEV rewards, checking both field names
    pub fn get_mev_rewards(&self) -> u64 {
        if self.mev_rewards > 0 {
            self.mev_rewards
        } else {
            self.mev_rewards_alt
        }
    }
}

/// MEV rewards history for a validator
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct JitoMevHistory {
    pub vote_account: String,
    pub epochs: Vec<JitoEpochReward>,
}

/// Raw API response - can be array or object with epochs
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JitoApiResponse {
    Array(Vec<JitoEpochReward>),
    Object { epochs: Option<Vec<JitoEpochReward>> },
}

/// Fetch MEV rewards history from Jito API
pub async fn get_jito_mev_history(epoch_count: usize) -> Option<JitoMevHistory> {
    let url = format!("{}/api/v1/validators/{}", JITO_API_BASE, CONFIG.vote_account);

    let response = Request::get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("Jito API error: {}", response.status()).into());
        return None;
    }

    let text = response.text().await.ok()?;
    let data: JitoApiResponse = serde_json::from_str(&text).ok()?;

    let epochs_array = match data {
        JitoApiResponse::Array(arr) => arr,
        JitoApiResponse::Object { epochs } => epochs.unwrap_or_default(),
    };

    // Take last N epochs
    let epochs: Vec<JitoEpochReward> = epochs_array
        .into_iter()
        .rev()
        .take(epoch_count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    Some(JitoMevHistory {
        vote_account: CONFIG.vote_account.to_string(),
        epochs,
    })
}

/// Format lamports to SOL with appropriate precision
pub fn format_lamports_to_sol(lamports: u64, decimals: usize) -> String {
    let sol = lamports as f64 / 1_000_000_000.0;
    if sol == 0.0 {
        return "0".to_string();
    }
    if sol < 0.0001 {
        return "<0.0001".to_string();
    }
    format!("{:.prec$}", sol, prec = decimals)
}
