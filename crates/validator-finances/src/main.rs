//! Block Parliament Validator Financial Tracking
//!
//! This tool tracks all revenue and expenses for the validator by querying
//! on-chain data and labeling known addresses.

mod addresses;
mod cache;
mod config;
mod constants;
mod dune;
mod expenses;
mod jito;
mod leader_fees;
mod notion;
mod prices;
mod reports;
mod transactions;
mod vote_costs;

use anyhow::Result;
use clap::{Parser, Subcommand};
use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use std::path::PathBuf;

use cache::Cache;
use config::FileConfig;
use expenses::{Expense, ExpenseCategory};

/// Default config file path
const CONFIG_FILE: &str = "config.toml";

/// Load config file or exit with helpful message
fn load_config_file() -> Result<FileConfig> {
    let path = std::path::Path::new(CONFIG_FILE);

    if !path.exists() {
        anyhow::bail!(
            "Config file '{}' not found.\n\n\
            To get started:\n\
            1. Copy config.toml.example to config.toml\n\
            2. Fill in your API keys\n\n\
            See config.toml.example for the required format.",
            CONFIG_FILE
        );
    }

    FileConfig::load(path)
}

/// Mask API keys in URLs for safe logging
/// Converts "https://example.com/?api-key=SECRET" to "https://example.com/?api-key=****"
fn mask_api_key(url: &str) -> String {
    if let Some(idx) = url.find("api-key=") {
        let prefix = &url[..idx + 8]; // Include "api-key="
        format!("{}****", prefix)
    } else if let Some(idx) = url.find("apikey=") {
        let prefix = &url[..idx + 7];
        format!("{}****", prefix)
    } else {
        url.to_string()
    }
}

#[derive(Parser, Debug)]
#[command(name = "validator-finances")]
#[command(about = "Financial tracking for Block Parliament Solana validator")]
struct Args {
    /// Data directory for database (expenses, cache)
    #[arg(short, long, default_value = "./data", global = true)]
    data_dir: PathBuf,

    /// Output directory for generated CSV reports
    #[arg(short, long, default_value = "./output", global = true)]
    output_dir: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,

    /// Starting epoch (default: first epoch with rewards)
    #[arg(long)]
    start_epoch: Option<u64>,

    /// Ending epoch (default: current - 1)
    #[arg(long)]
    end_epoch: Option<u64>,

    /// Filter reports to a specific year (e.g., 2025)
    #[arg(long)]
    year: Option<i32>,

    /// RPC URL (uses private endpoint by default)
    #[arg(long)]
    rpc_url: Option<String>,

    /// Only fetch transactions (skip rewards query)
    #[arg(long)]
    transactions_only: bool,

    /// Force refresh all data (ignore cache)
    #[arg(long)]
    no_cache: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage expenses
    Expense {
        #[command(subcommand)]
        action: ExpenseCommand,
    },

    /// Import historical leader slot data
    LeaderSlots {
        #[command(subcommand)]
        action: LeaderSlotsCommand,
    },

    /// Manage vote transaction costs
    VoteCosts {
        #[command(subcommand)]
        action: VoteCostsCommand,
    },

    /// Import data from Dune Analytics (for backfilling pruned RPC data)
    Dune {
        #[command(subcommand)]
        action: DuneCommand,
    },
}

#[derive(Subcommand, Debug)]
enum VoteCostsCommand {
    /// Import vote costs from Dune Analytics JSON export
    Import {
        /// Path to JSON file with historical vote cost data
        file: PathBuf,
    },

    /// Show cached vote cost data
    List,

    /// Estimate vote costs for missing epochs
    Estimate {
        /// Starting epoch
        #[arg(long)]
        start: u64,

        /// Ending epoch
        #[arg(long)]
        end: u64,
    },
}

#[derive(Subcommand, Debug)]
enum LeaderSlotsCommand {
    /// Import leader slots from Dune Analytics JSON export
    Import {
        /// Path to JSON file with historical leader slot data
        file: PathBuf,

        /// RPC URL (uses private endpoint by default)
        #[arg(long)]
        rpc_url: Option<String>,
    },

    /// Show cached leader fee data
    List,
}

#[derive(Subcommand, Debug)]
enum ExpenseCommand {
    /// List all expenses
    List,

    /// Add a new expense
    Add {
        /// Date (YYYY-MM-DD)
        #[arg(long)]
        date: String,

        /// Vendor name
        #[arg(long)]
        vendor: String,

        /// Category: Hosting, Contractor, Hardware, Software, VoteFees, Other
        #[arg(long)]
        category: String,

        /// Description
        #[arg(long)]
        description: String,

        /// Amount in USD
        #[arg(long)]
        amount: f64,

        /// Payment method (e.g., "Credit Card", "USD", "SOL")
        #[arg(long, default_value = "USD")]
        paid_with: String,

        /// Invoice ID (optional)
        #[arg(long)]
        invoice_id: Option<String>,
    },

    /// Delete an expense by ID
    Delete {
        /// Expense ID to delete
        id: i64,
    },

    /// Import expenses from CSV file
    Import {
        /// Path to CSV file
        file: PathBuf,
    },

    /// Export expenses to CSV file
    Export {
        /// Path to output CSV file
        file: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum DuneCommand {
    /// Import inflation rewards from Dune
    Rewards {
        /// Start date for query (YYYY-MM-DD)
        #[arg(long)]
        since: String,
    },

    /// Import leader slot fees from Dune
    LeaderFees {
        /// Start date for query (YYYY-MM-DD)
        #[arg(long)]
        since: String,
    },

    /// Import vote transaction costs from Dune
    VoteCosts {
        /// Start date for query (YYYY-MM-DD)
        #[arg(long)]
        since: String,
    },

    /// Import SOL transfers from Dune
    Transfers {
        /// Start date for query (YYYY-MM-DD)
        #[arg(long)]
        since: String,
    },

    /// Import all data types from Dune
    All {
        /// Start date for query (YYYY-MM-DD)
        #[arg(long)]
        since: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Create directories
    std::fs::create_dir_all(&args.data_dir)?;
    std::fs::create_dir_all(&args.output_dir)?;

    // Open cache database (in data directory)
    let cache_path = args.data_dir.join(constants::CACHE_FILENAME);
    let cache = Cache::open(&cache_path).await?;

    // Handle subcommands
    if let Some(command) = args.command {
        return handle_command(command, &cache).await;
    }

    // No subcommand - run the main report generation
    run_report_generation(args, cache).await
}

/// Handle expense management subcommands
async fn handle_command(command: Command, cache: &Cache) -> Result<()> {
    match command {
        Command::Expense { action } => handle_expense_command(action, cache).await,
        Command::LeaderSlots { action } => handle_leader_slots_command(action, cache).await,
        Command::VoteCosts { action } => handle_vote_costs_command(action, cache).await,
        Command::Dune { action } => handle_dune_command(action, cache).await,
    }
}

/// Handle leader slots subcommands
async fn handle_leader_slots_command(action: LeaderSlotsCommand, cache: &Cache) -> Result<()> {
    match action {
        LeaderSlotsCommand::Import { file, rpc_url } => {
            println!(
                "Importing historical leader slot data from {}...\n",
                file.display()
            );

            // Load config file and initialize runtime configuration
            let file_config = load_config_file()?;
            let config = config::Config::from_file(&file_config, rpc_url)?;
            println!("Using RPC: {}\n", mask_api_key(&config.rpc_url));

            // Import and fetch fees for historical slots
            let fees = leader_fees::import_historical_leader_fees(&config, &file).await?;

            if fees.is_empty() {
                println!("\nNo leader fee data imported.");
                return Ok(());
            }

            // Store in cache
            cache.store_leader_fees(&fees).await?;

            println!("\n=============================================");
            println!("Import Summary:");
            println!("=============================================");

            let mut total_slots = 0u64;
            let mut total_blocks = 0u64;
            let mut total_fees = 0.0f64;

            for fee in &fees {
                println!(
                    "  Epoch {}: {} slots, {} blocks, {:.6} SOL",
                    fee.epoch, fee.leader_slots, fee.blocks_produced, fee.total_fees_sol
                );
                total_slots += fee.leader_slots;
                total_blocks += fee.blocks_produced;
                total_fees += fee.total_fees_sol;
            }

            println!("---------------------------------------------");
            println!(
                "  Total: {} slots, {} blocks, {:.6} SOL",
                total_slots, total_blocks, total_fees
            );
            println!("\nData cached to database.");

            Ok(())
        }

        LeaderSlotsCommand::List => {
            // Show all cached leader fee data (use a reasonable max epoch)
            let fees = cache.get_leader_fees(0, 10_000).await?;

            if fees.is_empty() {
                println!("No leader fee data cached.");
                println!(
                    "\nUse 'validator-finances leader-slots import <file.json>' to import data"
                );
            } else {
                println!(
                    "{:<8} {:<12} {:>10} {:>10} {:>10} {:>14}",
                    "Epoch", "Date", "Slots", "Blocks", "Skipped", "Fees (SOL)"
                );
                println!("{}", "-".repeat(70));

                let mut total_slots = 0u64;
                let mut total_blocks = 0u64;
                let mut total_fees = 0.0f64;

                for fee in &fees {
                    println!(
                        "{:<8} {:<12} {:>10} {:>10} {:>10} {:>14.6}",
                        fee.epoch,
                        fee.date.as_deref().unwrap_or("-"),
                        fee.leader_slots,
                        fee.blocks_produced,
                        fee.skipped_slots,
                        fee.total_fees_sol,
                    );
                    total_slots += fee.leader_slots;
                    total_blocks += fee.blocks_produced;
                    total_fees += fee.total_fees_sol;
                }

                println!("{}", "-".repeat(70));
                println!(
                    "{:<8} {:>12} {:>10} {:>10} {:>10} {:>14.6}",
                    "Total", "", total_slots, total_blocks, "", total_fees
                );
                println!("\n{} epoch(s) cached", fees.len());
            }
            Ok(())
        }
    }
}

/// Handle vote costs subcommands
async fn handle_vote_costs_command(action: VoteCostsCommand, cache: &Cache) -> Result<()> {
    match action {
        VoteCostsCommand::Import { file } => {
            println!("Importing vote cost data from {}...\n", file.display());

            let costs = vote_costs::import_historical_vote_costs(&file)?;

            if costs.is_empty() {
                println!("No vote cost data found in file.");
                return Ok(());
            }

            // Store in cache
            cache.store_vote_costs(&costs).await?;

            println!("Imported {} epochs:\n", costs.len());
            println!(
                "{:<8} {:<12} {:>12} {:>14} {:>10}",
                "Epoch", "Date", "Votes", "Cost (SOL)", "Source"
            );
            println!("{}", "-".repeat(60));

            let mut total_votes = 0u64;
            let mut total_cost = 0.0f64;

            for cost in &costs {
                println!(
                    "{:<8} {:<12} {:>12} {:>14.6} {:>10}",
                    cost.epoch,
                    cost.date.as_deref().unwrap_or("-"),
                    cost.vote_count,
                    cost.total_fee_sol,
                    cost.source,
                );
                total_votes += cost.vote_count;
                total_cost += cost.total_fee_sol;
            }

            println!("{}", "-".repeat(60));
            println!(
                "{:<8} {:>12} {:>12} {:>14.6}",
                "Total", "", total_votes, total_cost
            );

            println!("\nData cached to database.");
            Ok(())
        }

        VoteCostsCommand::List => {
            let costs = cache.get_vote_costs(0, 10_000).await?;

            if costs.is_empty() {
                println!("No vote cost data cached.");
                println!("\nUse 'validator-finances vote-costs import <file.json>' to import data");
                println!(
                    "Or 'validator-finances vote-costs estimate --start N --end M' to estimate"
                );
            } else {
                println!(
                    "{:<8} {:<12} {:>12} {:>14} {:>10}",
                    "Epoch", "Date", "Votes", "Cost (SOL)", "Source"
                );
                println!("{}", "-".repeat(60));

                let mut total_votes = 0u64;
                let mut total_cost = 0.0f64;

                for cost in &costs {
                    println!(
                        "{:<8} {:<12} {:>12} {:>14.6} {:>10}",
                        cost.epoch,
                        cost.date.as_deref().unwrap_or("-"),
                        cost.vote_count,
                        cost.total_fee_sol,
                        cost.source,
                    );
                    total_votes += cost.vote_count;
                    total_cost += cost.total_fee_sol;
                }

                println!("{}", "-".repeat(60));
                println!(
                    "{:<8} {:>12} {:>12} {:>14.6}",
                    "Total", "", total_votes, total_cost
                );
                println!("\n{} epoch(s) cached", costs.len());
            }
            Ok(())
        }

        VoteCostsCommand::Estimate { start, end } => {
            let epoch_word = if start == end { "epoch" } else { "epochs" };
            println!(
                "Estimating vote costs for {} {}-{}...\n",
                epoch_word, start, end
            );

            let estimates = vote_costs::estimate_vote_costs(start, end);

            // Store in cache
            cache.store_vote_costs(&estimates).await?;

            println!(
                "Estimated {} epochs at ~{:.3} SOL each:",
                estimates.len(),
                vote_costs::TYPICAL_COST_PER_EPOCH_SOL
            );
            println!(
                "Total estimated cost: {:.6} SOL\n",
                vote_costs::total_vote_costs_sol(&estimates)
            );

            println!("Data cached to database.");
            Ok(())
        }
    }
}

/// Handle expense subcommands
async fn handle_expense_command(action: ExpenseCommand, cache: &Cache) -> Result<()> {
    match action {
        ExpenseCommand::List => {
            let expenses = cache.get_expenses().await?;
            if expenses.is_empty() {
                println!("No expenses recorded.");
                println!("\nUse 'validator-finances expense add' to add expenses");
                println!("Or 'validator-finances expense import <file.csv>' to import from CSV");
            } else {
                println!(
                    "{:<4} {:<12} {:<15} {:<12} {:>10}  Description",
                    "ID", "Date", "Vendor", "Category", "Amount"
                );
                println!("{}", "-".repeat(80));

                let mut total = 0.0;
                for expense in &expenses {
                    let id = expense.id.map(|i| i.to_string()).unwrap_or_default();
                    println!(
                        "{:<4} {:<12} {:<15} {:<12} ${:>9.2}  {}",
                        id,
                        expense.date,
                        truncate(&expense.vendor, 14),
                        expense.category,
                        expense.amount_usd,
                        truncate(&expense.description, 30),
                    );
                    total += expense.amount_usd;
                }
                println!("{}", "-".repeat(80));
                println!("{:>54} ${:>9.2}", "Total:", total);
                println!("\n{} expense(s)", expenses.len());
            }
            Ok(())
        }

        ExpenseCommand::Add {
            date,
            vendor,
            category,
            description,
            amount,
            paid_with,
            invoice_id,
        } => {
            let category = parse_category(&category)?;

            let expense = Expense {
                id: None,
                date,
                vendor,
                category,
                description,
                amount_usd: amount,
                paid_with,
                invoice_id,
            };

            let id = cache.add_expense(&expense).await?;
            println!(
                "Added expense #{}: {} - ${:.2}",
                id, expense.vendor, expense.amount_usd
            );
            Ok(())
        }

        ExpenseCommand::Delete { id } => {
            if cache.delete_expense(id).await? {
                println!("Deleted expense #{}", id);
            } else {
                println!("Expense #{} not found", id);
            }
            Ok(())
        }

        ExpenseCommand::Import { file } => {
            let expenses = expenses::load_from_csv(&file)?;
            let count = cache.import_expenses(&expenses).await?;
            println!("Imported {} expenses from {}", count, file.display());
            Ok(())
        }

        ExpenseCommand::Export { file } => {
            let expenses = cache.get_expenses().await?;
            expenses::export_to_csv(&expenses, &file)?;
            println!("Exported {} expenses to {}", expenses.len(), file.display());
            Ok(())
        }
    }
}

/// Handle Dune Analytics import subcommands
async fn handle_dune_command(action: DuneCommand, cache: &Cache) -> Result<()> {
    // Load config to get API key and validator addresses
    let file_config = load_config_file()?;
    let config = config::Config::from_file(&file_config, None)?;

    let api_key = file_config
        .api_keys
        .dune
        .ok_or_else(|| anyhow::anyhow!("Dune API key not configured in config.toml"))?;

    let client = dune::DuneClient::new(api_key, &config);

    println!("Dune Analytics Import");
    println!("=====================\n");

    match action {
        DuneCommand::Rewards { since } => {
            println!("Importing inflation rewards since {}...\n", since);

            let rewards = client.fetch_inflation_rewards(&since).await?;

            if rewards.is_empty() {
                println!("No rewards found.");
                return Ok(());
            }

            cache.store_epoch_rewards(&rewards).await?;

            println!("\nImported {} epochs:", rewards.len());
            let mut total_sol = 0.0;
            for reward in &rewards {
                println!(
                    "  Epoch {}: {:.6} SOL ({})",
                    reward.epoch,
                    reward.amount_sol,
                    reward.date.as_deref().unwrap_or("-")
                );
                total_sol += reward.amount_sol;
            }
            println!("\nTotal: {:.6} SOL", total_sol);
            println!("\nData cached to database.");
        }

        DuneCommand::LeaderFees { since } => {
            println!("Importing leader fees since {}...\n", since);

            let fees = client.fetch_leader_fees(&since).await?;

            if fees.is_empty() {
                println!("No leader fees found.");
                return Ok(());
            }

            cache.store_leader_fees(&fees).await?;

            println!("\nImported {} epochs:", fees.len());
            let mut total_sol = 0.0;
            for fee in &fees {
                println!(
                    "  Epoch {}: {} blocks, {:.6} SOL",
                    fee.epoch, fee.blocks_produced, fee.total_fees_sol
                );
                total_sol += fee.total_fees_sol;
            }
            println!("\nTotal: {:.6} SOL", total_sol);
            println!("\nData cached to database.");
        }

        DuneCommand::VoteCosts { since } => {
            println!("Importing vote costs since {}...\n", since);

            let costs = client.fetch_vote_costs(&since).await?;

            if costs.is_empty() {
                println!("No vote costs found.");
                return Ok(());
            }

            cache.store_vote_costs(&costs).await?;

            println!("\nImported {} epochs:", costs.len());
            let mut total_sol = 0.0;
            let mut total_votes = 0u64;
            for cost in &costs {
                println!(
                    "  Epoch {}: {} votes, {:.6} SOL",
                    cost.epoch, cost.vote_count, cost.total_fee_sol
                );
                total_sol += cost.total_fee_sol;
                total_votes += cost.vote_count;
            }
            println!("\nTotal: {} votes, {:.6} SOL", total_votes, total_sol);
            println!("\nData cached to database.");
        }

        DuneCommand::Transfers { since } => {
            println!("Importing SOL transfers since {}...\n", since);

            let transfers = client.fetch_transfers(&since).await?;

            if transfers.is_empty() {
                println!("No transfers found.");
                return Ok(());
            }

            // Store transfers (use "dune" as the account key to track them)
            cache.store_transfers(&transfers, "dune").await?;

            println!("\nImported {} transfers:", transfers.len());
            for transfer in transfers.iter().take(10) {
                println!(
                    "  {} -> {}: {:.6} SOL",
                    transfer.from_label, transfer.to_label, transfer.amount_sol
                );
            }
            if transfers.len() > 10 {
                println!("  ... and {} more", transfers.len() - 10);
            }
            println!("\nData cached to database.");
        }

        DuneCommand::All { since } => {
            println!("Importing all data since {}...\n", since);

            // Rewards
            println!("--- Inflation Rewards ---");
            let rewards = client.fetch_inflation_rewards(&since).await?;
            if !rewards.is_empty() {
                cache.store_epoch_rewards(&rewards).await?;
                println!(
                    "  Imported {} epochs, {:.6} SOL total\n",
                    rewards.len(),
                    rewards.iter().map(|r| r.amount_sol).sum::<f64>()
                );
            } else {
                println!("  No rewards found\n");
            }

            // Leader fees
            println!("--- Leader Fees ---");
            let fees = client.fetch_leader_fees(&since).await?;
            if !fees.is_empty() {
                cache.store_leader_fees(&fees).await?;
                println!(
                    "  Imported {} epochs, {:.6} SOL total\n",
                    fees.len(),
                    fees.iter().map(|f| f.total_fees_sol).sum::<f64>()
                );
            } else {
                println!("  No leader fees found\n");
            }

            // Vote costs
            println!("--- Vote Costs ---");
            let costs = client.fetch_vote_costs(&since).await?;
            if !costs.is_empty() {
                cache.store_vote_costs(&costs).await?;
                println!(
                    "  Imported {} epochs, {:.6} SOL total\n",
                    costs.len(),
                    costs.iter().map(|c| c.total_fee_sol).sum::<f64>()
                );
            } else {
                println!("  No vote costs found\n");
            }

            // Transfers
            println!("--- SOL Transfers ---");
            let transfers = client.fetch_transfers(&since).await?;
            if !transfers.is_empty() {
                cache.store_transfers(&transfers, "dune").await?;
                println!("  Imported {} transfers\n", transfers.len());
            } else {
                println!("  No transfers found\n");
            }

            println!("All data cached to database.");
        }
    }

    Ok(())
}

// =============================================================================
// Dune Fallback Helper
// =============================================================================

/// Prepare Dune Analytics fallback for epochs that RPC couldn't fetch.
/// Returns None if no fallback is needed or possible.
/// Returns Some((client, start_date)) if ready to attempt Dune fetch.
fn prepare_dune_fallback(
    rpc_failures: &[u64],
    dune_api_key: Option<&str>,
    config: &config::Config,
) -> Option<(dune::DuneClient, String)> {
    if rpc_failures.is_empty() {
        return None;
    }

    match dune_api_key {
        Some(api_key) => {
            println!(
                "    RPC failed for {} epochs, falling back to Dune...",
                rpc_failures.len()
            );
            let earliest_epoch = *rpc_failures.iter().min().unwrap();
            let start_date = transactions::epoch_to_date(earliest_epoch);
            let client = dune::DuneClient::new(api_key.to_string(), config);
            Some((client, start_date))
        }
        None => {
            eprintln!(
                "    Warning: {} epochs missing (no Dune API key for fallback)",
                rpc_failures.len()
            );
            None
        }
    }
}

/// Parse expense category from string
fn parse_category(s: &str) -> Result<ExpenseCategory> {
    match s.to_lowercase().as_str() {
        "hosting" => Ok(ExpenseCategory::Hosting),
        "contractor" => Ok(ExpenseCategory::Contractor),
        "hardware" => Ok(ExpenseCategory::Hardware),
        "software" => Ok(ExpenseCategory::Software),
        "votefees" | "vote_fees" | "vote-fees" => Ok(ExpenseCategory::VoteFees),
        "other" => Ok(ExpenseCategory::Other),
        _ => anyhow::bail!(
            "Invalid category '{}'. Use: Hosting, Contractor, Hardware, Software, VoteFees, Other",
            s
        ),
    }
}

/// Truncate string for display
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Run the main report generation workflow
async fn run_report_generation(args: Args, cache: Cache) -> Result<()> {
    println!("Block Parliament Validator Financial Tracker");
    println!("=============================================\n");

    // Load config file and initialize runtime configuration
    let file_config = load_config_file()?;
    let config = config::Config::from_file(&file_config, args.rpc_url)?;
    println!("Vote Account: {}", config.vote_account);
    println!("Identity: {}", config.identity);
    println!("RPC: {}\n", mask_api_key(&config.rpc_url));

    // Show cache stats
    let stats = cache.stats().await?;
    if !args.no_cache && (stats.epoch_rewards > 0 || stats.leader_fees > 0 || stats.transfers > 0) {
        println!("Cache: {}", stats);
    }

    // Get current epoch to know what's "complete" vs "in progress"
    let rpc_client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());
    let current_epoch = rpc_client.get_epoch_info()?.epoch;
    println!("Current epoch: {}\n", current_epoch);

    let start_epoch = args.start_epoch.unwrap_or(config.first_reward_epoch);
    let end_epoch = args.end_epoch.unwrap_or(current_epoch);

    // Get Dune API key for fallback (if configured)
    let dune_api_key = file_config.api_keys.dune.as_deref();

    // Step 1: Fetch inflation rewards by epoch (with caching)
    println!("Fetching inflation rewards...");
    let rewards = fetch_rewards_with_cache(
        &cache,
        &config,
        start_epoch,
        end_epoch,
        current_epoch,
        args.no_cache,
        dune_api_key,
    )
    .await?;
    println!("  Found {} epochs with rewards\n", rewards.len());

    // Step 2: Fetch all SOL transfers to/from our accounts (with caching)
    println!("Loading transaction history...");
    let transfers = fetch_transfers_with_cache(
        &cache,
        &config,
        args.no_cache,
        args.verbose,
        dune_api_key,
        &config.bootstrap_date,
    )
    .await?;
    println!("  Found {} SOL transfers\n", transfers.len());

    // Step 3: Categorize transfers
    println!("Categorizing transactions...");
    let categorized = transactions::categorize_transfers(&transfers, &config);

    println!("  Initial seeding: {} transfers", categorized.seeding.len());
    println!(
        "  SFDP reimbursements: {} transfers",
        categorized.sfdp_reimbursements.len()
    );
    println!(
        "  MEV deposits: {} transfers",
        categorized.mev_deposits.len()
    );
    println!(
        "  Vote fee funding: {} transfers",
        categorized.vote_funding.len()
    );
    println!("  Withdrawals: {} transfers", categorized.withdrawals.len());
    println!("  Other: {} transfers\n", categorized.other.len());

    // Step 4: Fetch Jito MEV claims (with caching)
    println!("Fetching Jito MEV claims...");
    let mev_claims = fetch_mev_with_cache(
        &cache,
        &config,
        start_epoch,
        end_epoch,
        current_epoch,
        args.no_cache,
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("  Warning: Failed to fetch MEV claims: {}", e);
        Vec::new()
    });
    let total_mev = jito::total_mev_sol(&mev_claims);
    println!(
        "  Found {} MEV claims totaling {:.6} SOL\n",
        mev_claims.len(),
        total_mev
    );

    // Step 5: Fetch leader slot fees (with caching - this is the slow one!)
    println!("Fetching leader slot fees...");
    let leader_fees = fetch_leader_fees_with_cache(
        &cache,
        &config,
        start_epoch,
        end_epoch,
        current_epoch,
        args.no_cache,
        dune_api_key,
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("  Warning: Failed to fetch leader fees: {}", e);
        Vec::new()
    });
    let total_leader_fees = leader_fees::total_leader_fees_sol(&leader_fees);
    println!(
        "  Found {} epochs with leader fees totaling {:.6} SOL\n",
        leader_fees.len(),
        total_leader_fees
    );

    // Step 6: Load vote costs from cache, auto-estimate missing epochs
    println!("Loading vote costs...");
    let mut vote_costs = cache.get_vote_costs(start_epoch, end_epoch).await?;
    let cached_count = vote_costs.len();

    // Find missing epochs and auto-estimate them
    let cached_epochs: std::collections::HashSet<u64> =
        vote_costs.iter().map(|c| c.epoch).collect();
    let mut estimated_epochs = Vec::new();
    for epoch in start_epoch..=end_epoch {
        if !cached_epochs.contains(&epoch) {
            let estimate = vote_costs::estimate_vote_cost(epoch);
            vote_costs.push(estimate);
            estimated_epochs.push(epoch);
        }
    }
    // Sort by epoch after adding estimates
    vote_costs.sort_by_key(|c| c.epoch);

    if vote_costs.is_empty() {
        println!(
            "  No vote costs cached (use 'vote-costs import' or 'vote-costs estimate' to add)\n"
        );
    } else {
        let total_vote_cost = vote_costs::total_vote_costs_sol(&vote_costs);
        if !estimated_epochs.is_empty() {
            println!(
                "  Loaded {} epochs, estimated {} missing ({:?}): {:.6} SOL total\n",
                cached_count,
                estimated_epochs.len(),
                estimated_epochs,
                total_vote_cost
            );
        } else {
            println!(
                "  Loaded {} epochs totaling {:.6} SOL in vote fees\n",
                vote_costs.len(),
                total_vote_cost
            );
        }
    }

    // Step 7: Load expenses (database + Notion contractor hours)
    println!("Loading expenses...");
    let mut all_expenses = cache.get_expenses().await?;
    let db_expense_count = all_expenses.len();

    // Fetch contractor hours from Notion if configured
    if let Some(notion_config) = &file_config.notion {
        println!("  Fetching contractor hours from Notion...");
        match notion::fetch_hours_log(notion_config).await {
            Ok(hours_entries) => {
                let summary = notion::hours_summary(&hours_entries);
                println!(
                    "    Found {} entries: {:.1}h total (${:.2}), {:.1}h unpaid (${:.2})",
                    summary.total_entries,
                    summary.total_hours,
                    summary.total_amount,
                    summary.unpaid_hours,
                    summary.unpaid_amount
                );

                // Convert hours to expenses and add to list
                let contractor_expenses = notion::hours_to_expenses(&hours_entries);
                all_expenses.extend(contractor_expenses);
            }
            Err(e) => {
                eprintln!("    Warning: Failed to fetch Notion data: {}", e);
            }
        }
    }

    let total_expense = expenses::total_expenses(&all_expenses);
    if all_expenses.is_empty() {
        println!("  No expenses recorded\n");
    } else {
        let notion_count = all_expenses.len() - db_expense_count;
        if notion_count > 0 {
            println!(
                "  Loaded {} expenses ({} from database, {} from Notion) totaling ${:.2}\n",
                all_expenses.len(),
                db_expense_count,
                notion_count,
                total_expense
            );
        } else {
            println!(
                "  Loaded {} expense entries totaling ${:.2}\n",
                all_expenses.len(),
                total_expense
            );
        }
    }

    // Step 8: Fetch historical prices (with caching)
    println!("Fetching historical SOL prices...");
    let price_cache = fetch_prices_with_cache(
        &cache,
        &rewards,
        &transfers,
        &config.coingecko_api_key,
        args.no_cache,
    )
    .await?;
    println!("  Cached {} daily prices\n", price_cache.len());

    // Step 9: Generate reports
    if let Some(year) = args.year {
        println!("Generating reports for year {}...", year);
    } else {
        println!("Generating reports...");
    }
    let report_data = reports::ReportData {
        rewards: &rewards,
        categorized: &categorized,
        mev_claims: &mev_claims,
        leader_fees: &leader_fees,
        vote_costs: &vote_costs,
        expenses: &all_expenses,
        prices: &price_cache,
        config: &config,
    };
    reports::generate_all_reports(&args.output_dir, &report_data, args.year)?;

    // Step 10: Print summary
    reports::print_summary(&report_data, args.year);

    println!("\nDone! Reports written to: {}", args.output_dir.display());

    Ok(())
}

/// Fetch rewards with caching - only fetch missing epochs
/// Falls back to Dune Analytics if RPC fails and API key is configured
async fn fetch_rewards_with_cache(
    cache: &Cache,
    config: &config::Config,
    start_epoch: u64,
    end_epoch: u64,
    current_epoch: u64,
    no_cache: bool,
    dune_api_key: Option<&str>,
) -> Result<Vec<transactions::EpochReward>> {
    if no_cache {
        // Fetch everything fresh
        let rewards =
            transactions::fetch_inflation_rewards(config, start_epoch, Some(end_epoch)).await?;
        cache.store_epoch_rewards(&rewards).await?;
        return Ok(rewards);
    }

    // Only query cache/Dune for completed epochs (current epoch won't have data yet)
    let completed_end = end_epoch.min(current_epoch.saturating_sub(1));

    // Get cached rewards for completed epochs
    let mut rewards = cache.get_epoch_rewards(start_epoch, completed_end).await?;
    let cached_count = rewards.len();

    // Find missing completed epochs (exclude current - it's always "missing" but not fetchable)
    let missing = cache
        .get_missing_reward_epochs(start_epoch, completed_end)
        .await?;

    if !missing.is_empty() {
        let epoch_word = if missing.len() == 1 {
            "epoch"
        } else {
            "epochs"
        };
        println!("    Fetching {} missing {}...", missing.len(), epoch_word);

        let mut rpc_failures: Vec<u64> = Vec::new();

        // Fetch missing epochs one by one via RPC
        for epoch in &missing {
            match transactions::fetch_inflation_rewards(config, *epoch, Some(*epoch)).await {
                Ok(mut fetched) if !fetched.is_empty() => {
                    cache.store_epoch_rewards(&fetched).await?;
                    rewards.append(&mut fetched);
                }
                _ => {
                    // RPC failed or returned empty - track for Dune fallback
                    rpc_failures.push(*epoch);
                }
            }
        }

        // Fall back to Dune for epochs that RPC couldn't fetch
        if let Some((dune_client, start_date)) =
            prepare_dune_fallback(&rpc_failures, dune_api_key, config)
        {
            match dune_client.fetch_inflation_rewards(&start_date).await {
                Ok(dune_rewards) => {
                    // Filter to only the epochs we need
                    let needed: Vec<_> = dune_rewards
                        .into_iter()
                        .filter(|r| rpc_failures.contains(&r.epoch))
                        .collect();

                    if !needed.is_empty() {
                        println!("    Dune returned {} epochs", needed.len());
                        cache.store_epoch_rewards(&needed).await?;

                        // Track which epochs were filled by Dune
                        let filled_epochs: std::collections::HashSet<u64> =
                            needed.iter().map(|r| r.epoch).collect();
                        rewards.extend(needed);

                        // Cache epochs that remain unfilled (no data exists)
                        let unfilled: Vec<_> = rpc_failures
                            .iter()
                            .filter(|e| !filled_epochs.contains(e))
                            .map(|&epoch| transactions::EpochReward {
                                epoch,
                                effective_slot: epoch * constants::SLOTS_PER_EPOCH,
                                amount_lamports: 0,
                                amount_sol: 0.0,
                                commission: config.commission_percent,
                                date: Some(transactions::epoch_to_date(epoch)),
                            })
                            .collect();

                        if !unfilled.is_empty() {
                            println!("    Caching {} epochs with no reward data", unfilled.len());
                            cache.store_epoch_rewards(&unfilled).await?;
                        }
                    } else {
                        // Dune returned data but none for our requested epochs
                        let empty_epochs: Vec<_> = rpc_failures
                            .iter()
                            .map(|&epoch| transactions::EpochReward {
                                epoch,
                                effective_slot: epoch * constants::SLOTS_PER_EPOCH,
                                amount_lamports: 0,
                                amount_sol: 0.0,
                                commission: config.commission_percent,
                                date: Some(transactions::epoch_to_date(epoch)),
                            })
                            .collect();
                        println!(
                            "    Caching {} epochs with no reward data",
                            empty_epochs.len()
                        );
                        cache.store_epoch_rewards(&empty_epochs).await?;
                    }
                }
                Err(e) => {
                    eprintln!("    Warning: Dune fallback failed: {}", e);
                }
            }
        }
    }

    // Fetch current epoch if requested (always fresh, don't cache, no Dune fallback)
    // Note: Current epoch rewards are pending until epoch completion
    if end_epoch >= current_epoch {
        if let Ok(mut current_rewards) =
            transactions::fetch_current_epoch_rewards(config, current_epoch).await
        {
            rewards.append(&mut current_rewards);
        }
    }

    if cached_count > 0 {
        println!("    ({} epochs from cache)", cached_count);
    }

    // Sort by epoch
    rewards.sort_by_key(|r| r.epoch);

    Ok(rewards)
}

/// Fetch MEV claims with caching
///
/// MEV claims only exist for completed epochs (distributed at epoch boundaries),
/// so we only need to check for missing completed epochs, not the current epoch.
async fn fetch_mev_with_cache(
    cache: &Cache,
    config: &config::Config,
    start_epoch: u64,
    end_epoch: u64,
    current_epoch: u64,
    no_cache: bool,
) -> Result<Vec<jito::MevClaim>> {
    if no_cache {
        let claims = jito::fetch_mev_claims(config).await?;
        cache.store_mev_claims(&claims).await?;
        return Ok(claims);
    }

    // MEV claims only exist for completed epochs
    let completed_end = end_epoch.min(current_epoch.saturating_sub(1));

    // Get cached claims for completed epochs
    let mut claims = cache.get_mev_claims(start_epoch, completed_end).await?;
    let cached_count = claims.len();

    // Check if we need to fetch from Jito API
    // The Jito API returns all epochs with MEV for this validator at once.
    // If we have MEV data for a recent completed epoch, we're up to date.
    // (Epochs without MEV rewards are not returned by the API, so checking
    // for "missing" epochs would cause constant re-fetching.)
    let has_recent_data = claims
        .iter()
        .any(|c| c.epoch >= completed_end.saturating_sub(1));

    if !has_recent_data {
        println!(
            "    Fetching from Jito API (need data through epoch {})...",
            completed_end
        );
        let fresh_claims = jito::fetch_mev_claims(config).await?;

        // Store completed epochs in cache
        let completed: Vec<_> = fresh_claims
            .iter()
            .filter(|c| c.epoch < current_epoch)
            .cloned()
            .collect();
        if !completed.is_empty() {
            cache.store_mev_claims(&completed).await?;
            println!("    Cached {} completed epochs", completed.len());
        }

        // Filter to requested range
        claims = fresh_claims
            .into_iter()
            .filter(|c| c.epoch >= start_epoch && c.epoch <= end_epoch)
            .collect();
    } else {
        println!("    ({} epochs from cache)", cached_count);
    }

    // Sort by epoch
    claims.sort_by_key(|c| c.epoch);

    Ok(claims)
}

/// Fetch leader fees with caching - this is the expensive one!
/// Falls back to Dune Analytics if RPC fails and API key is configured
async fn fetch_leader_fees_with_cache(
    cache: &Cache,
    config: &config::Config,
    start_epoch: u64,
    end_epoch: u64,
    current_epoch: u64,
    no_cache: bool,
    dune_api_key: Option<&str>,
) -> Result<Vec<leader_fees::EpochLeaderFees>> {
    if no_cache {
        let fees = leader_fees::fetch_leader_fees(config, start_epoch, Some(end_epoch)).await?;
        // Only cache completed epochs
        let completed: Vec<_> = fees
            .iter()
            .filter(|f| f.epoch < current_epoch)
            .cloned()
            .collect();
        cache.store_leader_fees(&completed).await?;
        return Ok(fees);
    }

    // Get cached fees for completed epochs
    let completed_end = end_epoch.min(current_epoch.saturating_sub(1));
    let mut fees = cache.get_leader_fees(start_epoch, completed_end).await?;
    let cached_count = fees.len();

    // Find missing completed epochs
    let missing: Vec<u64> = cache
        .get_missing_leader_fee_epochs(start_epoch, completed_end)
        .await?
        .into_iter()
        .collect();

    // Also need to fetch current epoch if requested
    let need_current = end_epoch >= current_epoch;

    if !missing.is_empty() {
        let epoch_word = if missing.len() == 1 {
            "epoch"
        } else {
            "epochs"
        };
        println!(
            "    Fetching {} missing {} (this may take a while)...",
            missing.len(),
            epoch_word
        );

        let mut rpc_failures: Vec<u64> = Vec::new();

        // Fetch missing epochs via RPC
        for epoch in &missing {
            match leader_fees::fetch_leader_fees(config, *epoch, Some(*epoch)).await {
                Ok(fetched) if !fetched.is_empty() => {
                    // Store completed epochs in cache
                    let completed: Vec<_> = fetched
                        .iter()
                        .filter(|f| f.epoch < current_epoch)
                        .cloned()
                        .collect();
                    cache.store_leader_fees(&completed).await?;
                    fees.extend(fetched);
                }
                _ => {
                    // RPC failed or returned empty - track for Dune fallback
                    rpc_failures.push(*epoch);
                }
            }
        }

        // Fall back to Dune for epochs that RPC couldn't fetch
        if let Some((dune_client, start_date)) =
            prepare_dune_fallback(&rpc_failures, dune_api_key, config)
        {
            match dune_client.fetch_leader_fees(&start_date).await {
                Ok(dune_fees) => {
                    let needed: Vec<_> = dune_fees
                        .into_iter()
                        .filter(|f| rpc_failures.contains(&f.epoch))
                        .collect();

                    if !needed.is_empty() {
                        println!("    Dune returned {} epochs", needed.len());
                        cache.store_leader_fees(&needed).await?;

                        // Track which epochs were filled by Dune
                        let filled_epochs: std::collections::HashSet<u64> =
                            needed.iter().map(|f| f.epoch).collect();
                        fees.extend(needed);

                        // Cache epochs that remain unfilled (no data exists)
                        // This prevents re-querying epochs that genuinely have no leader slots
                        let unfilled: Vec<_> = rpc_failures
                            .iter()
                            .filter(|e| !filled_epochs.contains(e))
                            .map(|&epoch| leader_fees::EpochLeaderFees {
                                epoch,
                                leader_slots: 0,
                                blocks_produced: 0,
                                skipped_slots: 0,
                                total_fees_lamports: 0,
                                total_fees_sol: 0.0,
                                date: Some(transactions::epoch_to_date(epoch)),
                            })
                            .collect();

                        if !unfilled.is_empty() {
                            println!("    Caching {} epochs with no leader data", unfilled.len());
                            cache.store_leader_fees(&unfilled).await?;
                        }
                    } else {
                        // Dune returned data but none for our requested epochs
                        // Cache the requested epochs as having no data
                        let empty_epochs: Vec<_> = rpc_failures
                            .iter()
                            .map(|&epoch| leader_fees::EpochLeaderFees {
                                epoch,
                                leader_slots: 0,
                                blocks_produced: 0,
                                skipped_slots: 0,
                                total_fees_lamports: 0,
                                total_fees_sol: 0.0,
                                date: Some(transactions::epoch_to_date(epoch)),
                            })
                            .collect();
                        println!(
                            "    Caching {} epochs with no leader data",
                            empty_epochs.len()
                        );
                        cache.store_leader_fees(&empty_epochs).await?;
                    }
                }
                Err(e) => {
                    eprintln!("    Warning: Dune fallback failed: {}", e);
                }
            }
        }
    }

    // Fetch current epoch (always fresh, don't cache)
    if need_current {
        if let Ok(current_fees) =
            leader_fees::fetch_leader_fees(config, current_epoch, Some(current_epoch)).await
        {
            fees.extend(current_fees);
        }
    }

    if cached_count > 0 {
        println!("    ({} epochs from cache)", cached_count);
    }

    // Sort by epoch
    fees.sort_by_key(|f| f.epoch);

    Ok(fees)
}

/// Fetch prices with caching - only fetches missing dates
async fn fetch_prices_with_cache(
    cache: &Cache,
    rewards: &[transactions::EpochReward],
    transfers: &[transactions::SolTransfer],
    api_key: &str,
    no_cache: bool,
) -> Result<prices::PriceCache> {
    if no_cache {
        let prices = prices::fetch_historical_prices(rewards, transfers, api_key).await?;
        cache.store_prices(&prices).await?;
        return Ok(prices);
    }

    // Get cached prices
    let mut price_cache = cache.get_prices().await?;
    let cached_count = price_cache.len();

    // Fetch only missing prices (skips dates already in cache)
    let new_prices =
        prices::fetch_historical_prices_with_cache(rewards, transfers, api_key, Some(&price_cache))
            .await?;

    // Merge new prices into cache
    let new_count = new_prices.len();
    if new_count > 0 {
        for (date, price) in new_prices {
            price_cache.insert(date, price);
        }
        cache.store_prices(&price_cache).await?;
    }

    if cached_count > 0 {
        if new_count > 0 {
            println!("    ({} from cache, {} new)", cached_count, new_count);
        } else {
            println!("    ({} from cache, all dates covered)", cached_count);
        }
    }

    Ok(price_cache)
}

/// Fetch SOL transfers with caching - only fetch new transactions since last run
/// Falls back to Dune Analytics if RPC fails and API key is configured
async fn fetch_transfers_with_cache(
    cache: &Cache,
    config: &config::Config,
    no_cache: bool,
    verbose: bool,
    dune_api_key: Option<&str>,
    bootstrap_date: &str,
) -> Result<Vec<transactions::SolTransfer>> {
    if no_cache {
        // Fetch everything fresh
        let transfers = transactions::fetch_sol_transfers(config, verbose).await?;

        // Store by account
        for (label, account) in transactions::get_tracked_accounts(config) {
            let account_transfers: Vec<_> = transfers
                .iter()
                .filter(|t| t.from == account || t.to == account)
                .cloned()
                .collect();
            cache.store_transfers(&account_transfers, label).await?;
        }

        return Ok(transfers);
    }

    // Get cached transfers
    let cached_transfers = cache.get_all_transfers().await?;
    let cached_count = cached_transfers.len();

    // Track signatures we've seen (for deduplication)
    let mut seen_signatures: std::collections::HashSet<String> = cached_transfers
        .iter()
        .map(|t| t.signature.clone())
        .collect();

    let mut all_transfers = cached_transfers;

    // For each tracked account, fetch new transfers since the last cached slot
    let mut new_count = 0;
    let mut rpc_failed = false;

    for (label, account) in transactions::get_tracked_accounts(config) {
        // Use account progress (tracks highest slot seen, even if no transfers found)
        let latest_slot = cache.get_account_progress(label).await?;

        if verbose {
            println!("    {}: latest cached slot = {:?}", label, latest_slot);
        }

        // Fetch new transfers (will stop when hitting already-cached slot)
        match transactions::fetch_transfers_for_account(
            config,
            &account,
            label,
            latest_slot,
            verbose,
        )
        .await
        {
            Ok(result) => {
                // Store progress even if no transfers found (for accounts with only versioned txs)
                if let Some(highest_slot) = result.highest_slot_seen {
                    cache.set_account_progress(label, highest_slot).await?;
                }

                if !result.transfers.is_empty() {
                    if verbose {
                        println!(
                            "    {}: fetched {} new transfers",
                            label,
                            result.transfers.len()
                        );
                    }

                    // Store new transfers
                    cache.store_transfers(&result.transfers, label).await?;

                    // Add unique transfers to our collection (deduplicate across accounts)
                    for transfer in result.transfers {
                        if seen_signatures.insert(transfer.signature.clone()) {
                            new_count += 1;
                            all_transfers.push(transfer);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("    Warning: RPC failed for {}: {}", label, e);
                rpc_failed = true;
            }
        }
    }

    // Fall back to Dune if RPC failed and we have few/no transfers
    if (rpc_failed || all_transfers.is_empty()) && dune_api_key.is_some() {
        if let Some(api_key) = dune_api_key {
            println!("    Falling back to Dune for transfer history...");

            let dune_client = dune::DuneClient::new(api_key.to_string(), config);
            match dune_client.fetch_transfers(bootstrap_date).await {
                Ok(dune_transfers) => {
                    // Collect transfers that are actually new (not already seen)
                    let mut dune_new_transfers = Vec::new();
                    for transfer in dune_transfers {
                        if seen_signatures.insert(transfer.signature.clone()) {
                            dune_new_transfers.push(transfer);
                        }
                    }
                    if !dune_new_transfers.is_empty() {
                        println!(
                            "    Dune returned {} new transfers",
                            dune_new_transfers.len()
                        );
                        // Store the new Dune transfers to cache
                        cache.store_transfers(&dune_new_transfers, "dune").await?;
                        // Add to our collection
                        all_transfers.extend(dune_new_transfers);
                    }
                }
                Err(e) => {
                    eprintln!("    Warning: Dune fallback failed: {}", e);
                }
            }
        }
    }

    // Report caching stats
    if cached_count > 0 {
        if new_count > 0 {
            println!("    ({} from cache, {} new)", cached_count, new_count);
        } else {
            println!("    ({} from cache)", cached_count);
        }
    }

    // Sort by slot (newest first, matching the original behavior)
    all_transfers.sort_by(|a, b| b.slot.cmp(&a.slot));

    Ok(all_transfers)
}
