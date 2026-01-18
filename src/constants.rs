//! Centralized constants for the validator financial tracker
//!
//! This module contains all magic numbers, URLs, and configuration values
//! to make them easy to find and update.

// =============================================================================
// API Endpoints
// =============================================================================

/// Helius RPC base URL (append API key)
pub const HELIUS_RPC_BASE: &str = "https://mainnet.helius-rpc.com/?api-key=";

/// Jito MEV API base URL
pub const JITO_API_BASE: &str = "https://kobe.mainnet.jito.network/api/v1";

/// CoinGecko API base URL
pub const COINGECKO_API_BASE: &str = "https://api.coingecko.com/api/v3";

/// CoinGecko historical price endpoint (append from/to timestamps)
pub const COINGECKO_MARKET_CHART: &str = "/coins/solana/market_chart/range?vs_currency=usd";

/// CoinGecko current price endpoint
pub const COINGECKO_SIMPLE_PRICE: &str = "/simple/price?ids=solana&vs_currencies=usd";

// =============================================================================
// Solana Network Constants
// =============================================================================

/// Slots per epoch on mainnet
pub const SLOTS_PER_EPOCH: u64 = 432_000;

/// Approximate epoch duration in seconds (~2 days)
pub const EPOCH_DURATION_SECONDS: i64 = 172_800;

/// Lamports per SOL (code often uses 1e9 directly for brevity)
#[allow(dead_code)]
pub const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

// =============================================================================
// Epoch to Date Calibration
// Reference point for converting epoch numbers to approximate dates
// =============================================================================

/// Reference epoch for date calculation
pub const REFERENCE_EPOCH: i64 = 896;

/// Unix timestamp for reference epoch (2025-12-16 00:00:00 UTC)
pub const REFERENCE_EPOCH_TIMESTAMP: i64 = 1765843200;

// =============================================================================
// Block Parliament Validator Addresses
// =============================================================================

/// Vote account address
pub const VOTE_ACCOUNT: &str = "4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg";

/// Validator identity address
pub const IDENTITY: &str = "mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e";

/// Withdraw authority address
pub const WITHDRAW_AUTHORITY: &str = "AN58nFDFdehKbP7d3KALhnCJAsWNE7cWpCR6dLVAj9xm";

/// Personal wallet address (for seeding detection)
pub const PERSONAL_WALLET: &str = "CDfxi8DUxspoFPjdiXGyKkewCuQ8wJMszbBEtT4FTMZX";

/// SFDP vote cost reimbursement address
pub const SFDP_REIMBURSEMENT: &str = "DtZWL3BPKa5hw7yQYvaFR29PcXThpLHVU2XAAZrcLiSe";

// =============================================================================
// Validator Configuration
// =============================================================================

/// Commission percentage (5%)
pub const COMMISSION_PERCENT: u8 = 5;

/// Jito MEV commission percentage (10%)
pub const JITO_MEV_COMMISSION_PERCENT: u8 = 10;

/// First epoch with staking rewards
pub const FIRST_REWARD_EPOCH: u64 = 900;

/// SFDP acceptance date (epoch 896)
pub const SFDP_ACCEPTANCE_DATE: &str = "2025-12-16";

/// Validator bootstrap date
pub const BOOTSTRAP_DATE: &str = "2025-11-19";

// =============================================================================
// File Names
// =============================================================================

/// Cache database filename
pub const CACHE_FILENAME: &str = "cache.sqlite";

/// Expenses CSV filename (kept for CSV import/export compatibility)
#[allow(dead_code)]
pub const EXPENSES_FILENAME: &str = "expenses.csv";

/// Income ledger CSV filename
pub const INCOME_LEDGER_FILENAME: &str = "income_ledger.csv";

/// Expense ledger CSV filename
pub const EXPENSE_LEDGER_FILENAME: &str = "expense_ledger.csv";

/// Treasury ledger CSV filename
pub const TREASURY_LEDGER_FILENAME: &str = "treasury_ledger.csv";

/// Summary CSV filename
pub const SUMMARY_FILENAME: &str = "summary.csv";

// =============================================================================
// Rate Limiting
// =============================================================================

/// Delay between RPC signature fetches (ms)
pub const RPC_SIGNATURE_DELAY_MS: u64 = 200;

/// Delay between RPC transaction fetches (ms)
pub const RPC_TRANSACTION_DELAY_MS: u64 = 100;

/// Delay between block fetches for leader fees (ms)
pub const BLOCK_FETCH_DELAY_MS: u64 = 50;

/// Delay between epoch reward fetches (ms)
pub const EPOCH_REWARD_DELAY_MS: u64 = 100;

/// Maximum signatures to fetch per account
pub const MAX_SIGNATURES_PER_ACCOUNT: usize = 500;

// =============================================================================
// Thresholds
// =============================================================================

/// Minimum transfer amount to consider (lamports) - filters out fee dust
pub const MIN_TRANSFER_LAMPORTS: i64 = 1_000_000; // 0.001 SOL

/// Fallback SOL price if API fails
pub const FALLBACK_SOL_PRICE: f64 = 185.0;
