use gloo_net::http::Request;
use serde::Deserialize;
use shared::CONFIG;

/// Stakewiz validator data response
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct StakewizValidator {
    pub rank: u32,
    pub identity: String,
    pub vote_identity: String,
    pub last_vote: u64,
    pub root_slot: u64,
    pub credits: u64,
    pub epoch_credits: u64,
    pub activated_stake: f64,
    pub version: String,
    pub delinquent: bool,
    pub skip_rate: f64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub commission: u8,
    pub is_jito: bool,
    pub jito_commission_bps: u32,
    pub vote_success: f64,
    pub wiz_score: f64,
    pub uptime: f64,
    pub ip_city: Option<String>,
    pub ip_country: Option<String>,
    pub ip_org: Option<String>,
    pub epoch: u64,
    pub apy_estimate: Option<f64>,
    pub staking_apy: f64,
    pub jito_apy: f64,
    pub total_apy: f64,
    pub credit_ratio: f64,
    pub stake_ratio: Option<f64>,
    pub stake_weight: Option<f64>,
    pub asn: Option<String>,
}

/// Fetch validator data from Stakewiz API
pub async fn get_validator_data() -> Option<StakewizValidator> {
    let url = format!("https://api.stakewiz.com/validator/{}", CONFIG.vote_account);

    let response = Request::get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("Stakewiz API error: {}", response.status()).into());
        return None;
    }

    // Stakewiz returns `false` for unknown validators
    let text = response.text().await.ok()?;
    if text == "false" {
        web_sys::console::error_1(&"Validator not found on Stakewiz".into());
        return None;
    }

    serde_json::from_str(&text).ok()
}

/// Format stake in SOL with commas
pub fn format_stake(stake: f64) -> String {
    let rounded = stake.round() as i64;
    // Simple comma formatting
    let s = rounded.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}

/// Format percentage
pub fn format_percent(value: f64, decimals: usize) -> String {
    format!("{:.prec$}%", value, prec = decimals)
}
