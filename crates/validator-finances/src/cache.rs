//! SQLite caching for historical epoch data and expense storage
//!
//! Completed epochs are immutable, so we cache them to avoid re-querying.
//! Current/incomplete epochs are always re-fetched.
//! Expenses are stored persistently for financial tracking.

use anyhow::{Context, Result};
use sqlx::{FromRow, SqlitePool};
use std::path::Path;

use crate::addresses::AddressCategory;
use crate::expenses::{Expense, ExpenseCategory};
use crate::jito::MevClaim;
use crate::leader_fees::EpochLeaderFees;
use crate::prices::PriceCache;
use crate::transactions::{EpochReward, SolTransfer};
use crate::vote_costs::EpochVoteCost;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Cache database wrapper
pub struct Cache {
    pool: SqlitePool,
}

/// Row type for epoch rewards query
#[derive(FromRow)]
struct EpochRewardRow {
    epoch: i64,
    amount_lamports: i64,
    amount_sol: f64,
    commission: i64,
    effective_slot: i64,
    date: Option<String>,
}

/// Row type for leader fees query
#[derive(FromRow)]
struct LeaderFeesRow {
    epoch: i64,
    leader_slots: i64,
    blocks_produced: i64,
    skipped_slots: i64,
    total_fees_lamports: i64,
    total_fees_sol: f64,
    date: Option<String>,
}

/// Row type for MEV claims query
#[derive(FromRow)]
struct MevClaimRow {
    epoch: i64,
    total_tips_lamports: i64,
    commission_lamports: i64,
    amount_sol: f64,
    date: Option<String>,
}

/// Row type for vote costs query
#[derive(FromRow)]
struct VoteCostRow {
    epoch: i64,
    vote_count: i64,
    total_fee_lamports: i64,
    total_fee_sol: f64,
    source: String,
    date: Option<String>,
}

/// Row type for expenses query
#[derive(FromRow)]
struct ExpenseRow {
    id: i64,
    date: String,
    vendor: String,
    category: String,
    description: String,
    amount_usd: f64,
    paid_with: String,
    invoice_id: Option<String>,
}

/// Row type for sol_transfers query
#[derive(FromRow)]
struct SolTransferRow {
    signature: String,
    slot: i64,
    timestamp: Option<i64>,
    date: Option<String>,
    from_address: String,
    to_address: String,
    amount_lamports: i64,
    amount_sol: f64,
    from_label: String,
    to_label: String,
    from_category: String,
    to_category: String,
}

impl Cache {
    /// Open or create cache database
    pub async fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // SQLx requires the file to exist for SQLite
        if !path.exists() {
            std::fs::File::create(path)?;
        }

        let url = format!("sqlite:{}", path.display());
        let pool = SqlitePool::connect(&url)
            .await
            .context("Failed to open cache database")?;

        // Enable WAL mode for better concurrency and set busy timeout
        // This prevents SQLITE_BUSY errors when multiple processes access the DB
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA busy_timeout=5000")
            .execute(&pool)
            .await?;

        let cache = Self { pool };
        cache.init_schema().await?;

        Ok(cache)
    }

    /// Initialize database schema
    async fn init_schema(&self) -> Result<()> {
        sqlx::query(
            "
            -- Commission rewards per epoch
            CREATE TABLE IF NOT EXISTS epoch_rewards (
                epoch INTEGER PRIMARY KEY,
                amount_lamports INTEGER NOT NULL,
                amount_sol REAL NOT NULL,
                commission INTEGER NOT NULL,
                effective_slot INTEGER NOT NULL,
                date TEXT,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Leader slot fees per epoch
            CREATE TABLE IF NOT EXISTS leader_fees (
                epoch INTEGER PRIMARY KEY,
                leader_slots INTEGER NOT NULL,
                blocks_produced INTEGER NOT NULL,
                skipped_slots INTEGER NOT NULL,
                total_fees_lamports INTEGER NOT NULL,
                total_fees_sol REAL NOT NULL,
                date TEXT,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Jito MEV claims per epoch
            CREATE TABLE IF NOT EXISTS mev_claims (
                epoch INTEGER PRIMARY KEY,
                total_tips_lamports INTEGER NOT NULL,
                commission_lamports INTEGER NOT NULL,
                amount_sol REAL NOT NULL,
                date TEXT,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Vote transaction costs per epoch
            CREATE TABLE IF NOT EXISTS vote_costs (
                epoch INTEGER PRIMARY KEY,
                vote_count INTEGER NOT NULL,
                total_fee_lamports INTEGER NOT NULL,
                total_fee_sol REAL NOT NULL,
                source TEXT NOT NULL,
                date TEXT,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Historical SOL prices
            CREATE TABLE IF NOT EXISTS prices (
                date TEXT PRIMARY KEY,
                usd_price REAL NOT NULL,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Cache metadata
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Expenses (persistent storage, not cache)
            CREATE TABLE IF NOT EXISTS expenses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                vendor TEXT NOT NULL,
                category TEXT NOT NULL,
                description TEXT NOT NULL,
                amount_usd REAL NOT NULL,
                paid_with TEXT NOT NULL,
                invoice_id TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- SOL transfers (cached per account to avoid re-fetching)
            CREATE TABLE IF NOT EXISTS sol_transfers (
                signature TEXT NOT NULL,
                slot INTEGER NOT NULL,
                timestamp INTEGER,
                date TEXT,
                from_address TEXT NOT NULL,
                to_address TEXT NOT NULL,
                amount_lamports INTEGER NOT NULL,
                amount_sol REAL NOT NULL,
                from_label TEXT NOT NULL,
                to_label TEXT NOT NULL,
                from_category TEXT NOT NULL,
                to_category TEXT NOT NULL,
                account_key TEXT NOT NULL,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (signature, account_key)
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // Index for quick lookups by account
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_transfers_account ON sol_transfers(account_key)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Track the highest slot checked per account (even if no transfers found)
            CREATE TABLE IF NOT EXISTS account_progress (
                account_key TEXT PRIMARY KEY,
                highest_slot INTEGER NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // =========================================================================
    // Epoch Rewards (Commission)
    // =========================================================================

    /// Get cached epoch rewards
    pub async fn get_epoch_rewards(
        &self,
        start_epoch: u64,
        end_epoch: u64,
    ) -> Result<Vec<EpochReward>> {
        let rows: Vec<EpochRewardRow> = sqlx::query_as(
            "SELECT epoch, amount_lamports, amount_sol, commission, effective_slot, date
             FROM epoch_rewards
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EpochReward {
                epoch: r.epoch as u64,
                amount_lamports: r.amount_lamports as u64,
                amount_sol: r.amount_sol,
                commission: r.commission as u8,
                effective_slot: r.effective_slot as u64,
                date: r.date,
            })
            .collect())
    }

    /// Get epochs that are missing from cache
    pub async fn get_missing_reward_epochs(
        &self,
        start_epoch: u64,
        end_epoch: u64,
    ) -> Result<Vec<u64>> {
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT epoch FROM epoch_rewards WHERE epoch >= ? AND epoch <= ?")
                .bind(start_epoch as i64)
                .bind(end_epoch as i64)
                .fetch_all(&self.pool)
                .await?;

        let cached: Vec<u64> = rows.into_iter().map(|(e,)| e as u64).collect();

        let missing: Vec<u64> = (start_epoch..=end_epoch)
            .filter(|e| !cached.contains(e))
            .collect();

        Ok(missing)
    }

    /// Store epoch rewards (in a transaction for atomicity)
    pub async fn store_epoch_rewards(&self, rewards: &[EpochReward]) -> Result<()> {
        if rewards.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for reward in rewards {
            sqlx::query(
                "INSERT OR REPLACE INTO epoch_rewards
                 (epoch, amount_lamports, amount_sol, commission, effective_slot, date)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(reward.epoch as i64)
            .bind(reward.amount_lamports as i64)
            .bind(reward.amount_sol)
            .bind(reward.commission as i64)
            .bind(reward.effective_slot as i64)
            .bind(&reward.date)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Leader Fees
    // =========================================================================

    /// Get cached leader fees
    pub async fn get_leader_fees(
        &self,
        start_epoch: u64,
        end_epoch: u64,
    ) -> Result<Vec<EpochLeaderFees>> {
        let rows: Vec<LeaderFeesRow> = sqlx::query_as(
            "SELECT epoch, leader_slots, blocks_produced, skipped_slots,
                    total_fees_lamports, total_fees_sol, date
             FROM leader_fees
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EpochLeaderFees {
                epoch: r.epoch as u64,
                leader_slots: r.leader_slots as u64,
                blocks_produced: r.blocks_produced as u64,
                skipped_slots: r.skipped_slots as u64,
                total_fees_lamports: r.total_fees_lamports as u64,
                total_fees_sol: r.total_fees_sol,
                date: r.date,
            })
            .collect())
    }

    /// Get epochs missing leader fee data
    pub async fn get_missing_leader_fee_epochs(
        &self,
        start_epoch: u64,
        end_epoch: u64,
    ) -> Result<Vec<u64>> {
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT epoch FROM leader_fees WHERE epoch >= ? AND epoch <= ?")
                .bind(start_epoch as i64)
                .bind(end_epoch as i64)
                .fetch_all(&self.pool)
                .await?;

        let cached: Vec<u64> = rows.into_iter().map(|(e,)| e as u64).collect();

        let missing: Vec<u64> = (start_epoch..=end_epoch)
            .filter(|e| !cached.contains(e))
            .collect();

        Ok(missing)
    }

    /// Store leader fees (in a transaction for atomicity)
    pub async fn store_leader_fees(&self, fees: &[EpochLeaderFees]) -> Result<()> {
        if fees.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for fee in fees {
            sqlx::query(
                "INSERT OR REPLACE INTO leader_fees
                 (epoch, leader_slots, blocks_produced, skipped_slots, total_fees_lamports, total_fees_sol, date)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(fee.epoch as i64)
            .bind(fee.leader_slots as i64)
            .bind(fee.blocks_produced as i64)
            .bind(fee.skipped_slots as i64)
            .bind(fee.total_fees_lamports as i64)
            .bind(fee.total_fees_sol)
            .bind(&fee.date)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // MEV Claims
    // =========================================================================

    /// Get cached MEV claims
    pub async fn get_mev_claims(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<MevClaim>> {
        let rows: Vec<MevClaimRow> = sqlx::query_as(
            "SELECT epoch, total_tips_lamports, commission_lamports, amount_sol, date
             FROM mev_claims
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| MevClaim {
                epoch: r.epoch as u64,
                total_tips_lamports: r.total_tips_lamports as u64,
                commission_lamports: r.commission_lamports as u64,
                amount_sol: r.amount_sol,
                date: r.date,
            })
            .collect())
    }

    /// Store MEV claims (in a transaction for atomicity)
    pub async fn store_mev_claims(&self, claims: &[MevClaim]) -> Result<()> {
        if claims.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for claim in claims {
            sqlx::query(
                "INSERT OR REPLACE INTO mev_claims
                 (epoch, total_tips_lamports, commission_lamports, amount_sol, date)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(claim.epoch as i64)
            .bind(claim.total_tips_lamports as i64)
            .bind(claim.commission_lamports as i64)
            .bind(claim.amount_sol)
            .bind(&claim.date)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Vote Costs
    // =========================================================================

    /// Get cached vote costs
    pub async fn get_vote_costs(
        &self,
        start_epoch: u64,
        end_epoch: u64,
    ) -> Result<Vec<EpochVoteCost>> {
        let rows: Vec<VoteCostRow> = sqlx::query_as(
            "SELECT epoch, vote_count, total_fee_lamports, total_fee_sol, source, date
             FROM vote_costs
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EpochVoteCost {
                epoch: r.epoch as u64,
                vote_count: r.vote_count as u64,
                total_fee_lamports: r.total_fee_lamports as u64,
                total_fee_sol: r.total_fee_sol,
                source: r.source,
                date: r.date,
            })
            .collect())
    }

    /// Store vote costs (in a transaction for atomicity)
    pub async fn store_vote_costs(&self, costs: &[EpochVoteCost]) -> Result<()> {
        if costs.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for cost in costs {
            sqlx::query(
                "INSERT OR REPLACE INTO vote_costs
                 (epoch, vote_count, total_fee_lamports, total_fee_sol, source, date)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(cost.epoch as i64)
            .bind(cost.vote_count as i64)
            .bind(cost.total_fee_lamports as i64)
            .bind(cost.total_fee_sol)
            .bind(&cost.source)
            .bind(&cost.date)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Prices
    // =========================================================================

    /// Get cached prices
    pub async fn get_prices(&self) -> Result<PriceCache> {
        let rows: Vec<(String, f64)> = sqlx::query_as("SELECT date, usd_price FROM prices")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().collect())
    }

    /// Store prices (in a transaction for atomicity)
    pub async fn store_prices(&self, prices: &PriceCache) -> Result<()> {
        if prices.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for (date, price) in prices {
            sqlx::query("INSERT OR REPLACE INTO prices (date, usd_price) VALUES (?, ?)")
                .bind(date)
                .bind(price)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Metadata
    // =========================================================================

    /// Get metadata value
    #[allow(dead_code)]
    pub async fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM metadata WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|(v,)| v))
    }

    /// Set metadata value
    #[allow(dead_code)]
    pub async fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // =========================================================================
    // Expenses
    // =========================================================================

    /// Get all expenses
    pub async fn get_expenses(&self) -> Result<Vec<Expense>> {
        let rows: Vec<ExpenseRow> = sqlx::query_as(
            "SELECT id, date, vendor, category, description, amount_usd, paid_with, invoice_id
             FROM expenses
             ORDER BY date, id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let category = match r.category.as_str() {
                    "Hosting" => ExpenseCategory::Hosting,
                    "Contractor" => ExpenseCategory::Contractor,
                    "Hardware" => ExpenseCategory::Hardware,
                    "Software" => ExpenseCategory::Software,
                    "VoteFees" => ExpenseCategory::VoteFees,
                    _ => ExpenseCategory::Other,
                };

                Expense {
                    id: Some(r.id),
                    date: r.date,
                    vendor: r.vendor,
                    category,
                    description: r.description,
                    amount_usd: r.amount_usd,
                    paid_with: r.paid_with,
                    invoice_id: r.invoice_id,
                }
            })
            .collect())
    }

    /// Add a new expense, returns the ID
    pub async fn add_expense(&self, expense: &Expense) -> Result<i64> {
        let category_str = match expense.category {
            ExpenseCategory::Hosting => "Hosting",
            ExpenseCategory::Contractor => "Contractor",
            ExpenseCategory::Hardware => "Hardware",
            ExpenseCategory::Software => "Software",
            ExpenseCategory::VoteFees => "VoteFees",
            ExpenseCategory::Other => "Other",
        };

        let result = sqlx::query(
            "INSERT INTO expenses (date, vendor, category, description, amount_usd, paid_with, invoice_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&expense.date)
        .bind(&expense.vendor)
        .bind(category_str)
        .bind(&expense.description)
        .bind(expense.amount_usd)
        .bind(&expense.paid_with)
        .bind(&expense.invoice_id)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Delete an expense by ID
    pub async fn delete_expense(&self, id: i64) -> Result<bool> {
        let result = sqlx::query("DELETE FROM expenses WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Import multiple expenses (for bulk import from CSV)
    pub async fn import_expenses(&self, expenses: &[Expense]) -> Result<usize> {
        let mut count = 0;
        for expense in expenses {
            self.add_expense(expense).await?;
            count += 1;
        }
        Ok(count)
    }

    // =========================================================================
    // SOL Transfers
    // =========================================================================

    /// Get all cached transfers
    pub async fn get_all_transfers(&self) -> Result<Vec<SolTransfer>> {
        let rows: Vec<SolTransferRow> = sqlx::query_as(
            "SELECT DISTINCT signature, slot, timestamp, date, from_address, to_address,
                    amount_lamports, amount_sol, from_label, to_label,
                    from_category, to_category
             FROM sol_transfers
             ORDER BY slot DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        // Deduplicate by signature (same transfer may be cached under multiple accounts)
        let mut seen = std::collections::HashSet::new();
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                if seen.contains(&r.signature) {
                    None
                } else {
                    seen.insert(r.signature.clone());
                    row_to_transfer(r)
                }
            })
            .collect())
    }

    /// Get the highest slot we've checked for an account (even if no transfers were found)
    /// This is useful for accounts with only versioned/undecodable transactions
    pub async fn get_account_progress(&self, account_key: &str) -> Result<Option<u64>> {
        // Check both account_progress table and sol_transfers, use the higher value
        let progress_row: Option<(i64,)> =
            sqlx::query_as("SELECT highest_slot FROM account_progress WHERE account_key = ?")
                .bind(account_key)
                .fetch_optional(&self.pool)
                .await?;

        let transfer_row: Option<(i64,)> =
            sqlx::query_as("SELECT MAX(slot) FROM sol_transfers WHERE account_key = ?")
                .bind(account_key)
                .fetch_optional(&self.pool)
                .await?;

        let progress_slot = progress_row.map(|(s,)| s as u64);
        let transfer_slot = transfer_row.and_then(|(s,)| if s > 0 { Some(s as u64) } else { None });

        Ok(match (progress_slot, transfer_slot) {
            (Some(p), Some(t)) => Some(p.max(t)),
            (Some(p), None) => Some(p),
            (None, Some(t)) => Some(t),
            (None, None) => None,
        })
    }

    /// Store the highest slot we've checked for an account
    pub async fn set_account_progress(&self, account_key: &str, highest_slot: u64) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO account_progress (account_key, highest_slot) VALUES (?, ?)",
        )
        .bind(account_key)
        .bind(highest_slot as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Store transfers for a specific account (in a transaction for atomicity)
    pub async fn store_transfers(
        &self,
        transfers: &[SolTransfer],
        account_key: &str,
    ) -> Result<()> {
        if transfers.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for transfer in transfers {
            sqlx::query(
                "INSERT OR REPLACE INTO sol_transfers
                 (signature, slot, timestamp, date, from_address, to_address,
                  amount_lamports, amount_sol, from_label, to_label,
                  from_category, to_category, account_key)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&transfer.signature)
            .bind(transfer.slot as i64)
            .bind(transfer.timestamp)
            .bind(&transfer.date)
            .bind(transfer.from.to_string())
            .bind(transfer.to.to_string())
            .bind(transfer.amount_lamports as i64)
            .bind(transfer.amount_sol)
            .bind(&transfer.from_label)
            .bind(&transfer.to_label)
            .bind(category_to_string(&transfer.from_category))
            .bind(category_to_string(&transfer.to_category))
            .bind(account_key)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Utilities
    // =========================================================================

    /// Get cache statistics
    pub async fn stats(&self) -> Result<CacheStats> {
        let epoch_rewards: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM epoch_rewards")
            .fetch_one(&self.pool)
            .await?;
        let leader_fees: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leader_fees")
            .fetch_one(&self.pool)
            .await?;
        let mev_claims: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM mev_claims")
            .fetch_one(&self.pool)
            .await?;
        let vote_costs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM vote_costs")
            .fetch_one(&self.pool)
            .await?;
        let prices: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM prices")
            .fetch_one(&self.pool)
            .await?;
        let expenses: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM expenses")
            .fetch_one(&self.pool)
            .await?;
        let transfers: (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT signature) FROM sol_transfers")
                .fetch_one(&self.pool)
                .await
                .unwrap_or((0,));

        Ok(CacheStats {
            epoch_rewards: epoch_rewards.0 as u64,
            leader_fees: leader_fees.0 as u64,
            mev_claims: mev_claims.0 as u64,
            vote_costs: vote_costs.0 as u64,
            prices: prices.0 as u64,
            expenses: expenses.0 as u64,
            transfers: transfers.0 as u64,
        })
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Convert a SolTransferRow to a SolTransfer
fn row_to_transfer(r: SolTransferRow) -> Option<SolTransfer> {
    let from = Pubkey::from_str(&r.from_address).ok()?;
    let to = Pubkey::from_str(&r.to_address).ok()?;

    Some(SolTransfer {
        signature: r.signature,
        slot: r.slot as u64,
        timestamp: r.timestamp,
        date: r.date,
        from,
        to,
        amount_lamports: r.amount_lamports as u64,
        amount_sol: r.amount_sol,
        from_label: r.from_label,
        to_label: r.to_label,
        from_category: string_to_category(&r.from_category),
        to_category: string_to_category(&r.to_category),
    })
}

/// Convert AddressCategory to string for storage
fn category_to_string(cat: &AddressCategory) -> &'static str {
    match cat {
        AddressCategory::SolanaFoundation => "SolanaFoundation",
        AddressCategory::JitoMev => "JitoMev",
        AddressCategory::Exchange => "Exchange",
        AddressCategory::ValidatorSelf => "ValidatorSelf",
        AddressCategory::PersonalWallet => "PersonalWallet",
        AddressCategory::SystemProgram => "SystemProgram",
        AddressCategory::StakeProgram => "StakeProgram",
        AddressCategory::VoteProgram => "VoteProgram",
        AddressCategory::Unknown => "Unknown",
    }
}

/// Convert string to AddressCategory
fn string_to_category(s: &str) -> AddressCategory {
    match s {
        "SolanaFoundation" => AddressCategory::SolanaFoundation,
        "JitoMev" => AddressCategory::JitoMev,
        "Exchange" => AddressCategory::Exchange,
        "ValidatorSelf" => AddressCategory::ValidatorSelf,
        "PersonalWallet" => AddressCategory::PersonalWallet,
        "SystemProgram" => AddressCategory::SystemProgram,
        "StakeProgram" => AddressCategory::StakeProgram,
        "VoteProgram" => AddressCategory::VoteProgram,
        _ => AddressCategory::Unknown,
    }
}

/// Cache statistics
#[derive(Debug)]
pub struct CacheStats {
    pub epoch_rewards: u64,
    pub leader_fees: u64,
    pub mev_claims: u64,
    pub vote_costs: u64,
    pub prices: u64,
    pub expenses: u64,
    pub transfers: u64,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} rewards, {} leader fees, {} MEV claims, {} vote costs, {} transfers, {} prices, {} expenses",
            self.epoch_rewards, self.leader_fees, self.mev_claims, self.vote_costs, self.transfers, self.prices, self.expenses
        )
    }
}
