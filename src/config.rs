//! Configuration for the validator financial tracker

use anyhow::{Context, Result};
use chrono::Datelike;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use std::path::Path;
use std::str::FromStr;

use crate::constants;

// =============================================================================
// File-based Configuration (config.toml)
// =============================================================================

/// Configuration loaded from config.toml
#[derive(Debug, Deserialize)]
pub struct FileConfig {
    pub validator: ValidatorConfig,
    pub api_keys: ApiKeys,
    #[serde(default)]
    pub notion: Option<NotionConfig>,
}

/// Validator-specific configuration
#[derive(Debug, Deserialize)]
pub struct ValidatorConfig {
    /// Vote account address
    pub vote_account: String,
    /// Identity account address
    pub identity: String,
    /// Withdraw authority address
    pub withdraw_authority: String,
    /// Personal wallet address (for categorizing seeding/withdrawals)
    pub personal_wallet: String,
    /// Commission percentage (0-100)
    pub commission_percent: u8,
    /// Jito MEV commission percentage
    #[serde(default = "default_jito_commission")]
    pub jito_mev_commission_percent: u8,
    /// First epoch with staking rewards
    pub first_reward_epoch: u64,
    /// Bootstrap date (when validator was set up)
    pub bootstrap_date: String,
    /// SFDP acceptance date (optional - only if in SFDP program)
    #[serde(default)]
    pub sfdp_acceptance_date: Option<String>,
}

fn default_jito_commission() -> u8 {
    10
}

/// API keys section
#[derive(Debug, Deserialize)]
pub struct ApiKeys {
    pub helius: String,
    pub coingecko: String,
    #[serde(default)]
    pub dune: Option<String>,
}

/// Notion integration configuration
#[derive(Debug, Clone, Deserialize)]
pub struct NotionConfig {
    pub api_token: String,
    pub hours_database_id: String,
}

impl FileConfig {
    /// Load configuration from a TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        toml::from_str(&content).with_context(|| "Failed to parse config.toml")
    }
}

// =============================================================================
// Runtime Configuration
// =============================================================================

/// Main configuration struct with parsed values
pub struct Config {
    /// Vote account pubkey
    pub vote_account: Pubkey,
    /// Validator identity pubkey
    pub identity: Pubkey,
    /// Withdraw authority pubkey
    pub withdraw_authority: Pubkey,
    /// Personal wallet (for detecting seeding transactions)
    pub personal_wallet: Pubkey,
    /// RPC URL
    pub rpc_url: String,
    /// CoinGecko API key
    pub coingecko_api_key: String,
    /// Dune Analytics API key (optional, for backfilling pruned data)
    #[allow(dead_code)]
    pub dune_api_key: Option<String>,
    /// Commission percentage
    pub commission_percent: u8,
    /// Jito MEV commission percentage
    #[allow(dead_code)]
    pub jito_mev_commission_percent: u8,
    /// First epoch with rewards
    pub first_reward_epoch: u64,
    /// SFDP acceptance date (for calculating coverage schedule)
    pub sfdp_acceptance_date: Option<String>,
    /// Bootstrap date (for finding initial seeding)
    pub bootstrap_date: String,
}

impl Config {
    /// Create config from file config and optional RPC URL override
    pub fn from_file(file_config: &FileConfig, rpc_url: Option<String>) -> Result<Self> {
        let validator = &file_config.validator;

        Ok(Self {
            // Parse validator addresses from config
            vote_account: Pubkey::from_str(&validator.vote_account)
                .with_context(|| "Invalid vote_account address")?,
            identity: Pubkey::from_str(&validator.identity)
                .with_context(|| "Invalid identity address")?,
            withdraw_authority: Pubkey::from_str(&validator.withdraw_authority)
                .with_context(|| "Invalid withdraw_authority address")?,
            personal_wallet: Pubkey::from_str(&validator.personal_wallet)
                .with_context(|| "Invalid personal_wallet address")?,

            // Helius RPC endpoint (has historical transaction data)
            rpc_url: rpc_url.unwrap_or_else(|| {
                format!(
                    "{}{}",
                    constants::HELIUS_RPC_BASE,
                    &file_config.api_keys.helius
                )
            }),

            // CoinGecko API key for price lookups
            coingecko_api_key: file_config.api_keys.coingecko.clone(),

            // Dune API key for backfilling pruned data
            dune_api_key: file_config.api_keys.dune.clone(),

            // Commission rates from config
            commission_percent: validator.commission_percent,
            jito_mev_commission_percent: validator.jito_mev_commission_percent,

            // First epoch where validator earned rewards
            first_reward_epoch: validator.first_reward_epoch,

            // SFDP acceptance date (optional)
            sfdp_acceptance_date: validator.sfdp_acceptance_date.clone(),

            // Bootstrap date (when validator was first set up)
            bootstrap_date: validator.bootstrap_date.clone(),
        })
    }

    /// Check if a pubkey is one of our validator accounts
    pub fn is_our_account(&self, pubkey: &Pubkey) -> bool {
        *pubkey == self.vote_account
            || *pubkey == self.identity
            || *pubkey == self.withdraw_authority
    }

    /// Check if a pubkey is any account we care about (including personal wallet)
    pub fn is_relevant_account(&self, pubkey: &Pubkey) -> bool {
        self.is_our_account(pubkey) || *pubkey == self.personal_wallet
    }

    /// Calculate SFDP vote cost coverage percentage for a given date
    /// Schedule from acceptance date:
    /// - Months 1-3: 100% coverage
    /// - Months 4-6: 75% coverage
    /// - Months 7-9: 50% coverage
    /// - Months 10-12: 25% coverage
    /// - After 12 months: 0%
    pub fn sfdp_coverage_percent(&self, date: &chrono::NaiveDate) -> f64 {
        use chrono::NaiveDate;

        let Some(ref acceptance_str) = self.sfdp_acceptance_date else {
            return 0.0; // Not in SFDP program
        };

        let Ok(acceptance) = NaiveDate::parse_from_str(acceptance_str, "%Y-%m-%d") else {
            return 0.0; // Invalid date
        };

        let months_diff = (date.year() - acceptance.year()) * 12
            + (date.month() as i32 - acceptance.month() as i32);

        if months_diff < 0 {
            0.0
        } else if months_diff < 3 {
            1.0 // 100%
        } else if months_diff < 6 {
            0.75
        } else if months_diff < 9 {
            0.50
        } else if months_diff < 12 {
            0.25
        } else {
            0.0
        }
    }
}
