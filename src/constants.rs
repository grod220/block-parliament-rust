//! Centralized constants for the validator financial tracker
//!
//! This module contains universal constants that apply to all Solana validators.
//! Validator-specific configuration is loaded from config.toml.

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
// Known External Addresses (Solana ecosystem, not validator-specific)
// =============================================================================

/// SFDP vote cost reimbursement address (Solana Foundation)
pub const SFDP_REIMBURSEMENT: &str = "DtZWL3BPKa5hw7yQYvaFR29PcXThpLHVU2XAAZrcLiSe";

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
/// Note: Increased from 500 to handle longer transaction history
pub const MAX_SIGNATURES_PER_ACCOUNT: usize = 2000;

// =============================================================================
// Thresholds
// =============================================================================

/// Minimum transfer amount to consider (lamports) - filters out fee dust
pub const MIN_TRANSFER_LAMPORTS: i64 = 1_000_000; // 0.001 SOL

/// Fallback SOL price if API fails
pub const FALLBACK_SOL_PRICE: f64 = 185.0;

/// Fallback date for missing dates in SFDP calculations
/// Used when epoch->date conversion fails or date is unknown
/// This should be updated to a reasonable current date periodically
pub const FALLBACK_DATE: &str = "2025-12-15";
