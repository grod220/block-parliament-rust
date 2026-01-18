//! Block Parliament Validator Financial Tracking
//!
//! This tool tracks all revenue and expenses for the validator by querying
//! on-chain data and labeling known addresses.

mod addresses;
mod cache;
mod config;
mod constants;
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
            let config = config::Config::from_file(&file_config, rpc_url);
            println!("Using RPC: {}\n", config.rpc_url);

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
            println!("Estimating vote costs for epochs {}..{}...\n", start, end);

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
    let config = config::Config::from_file(&file_config, args.rpc_url);
    println!("Vote Account: {}", config.vote_account);
    println!("Identity: {}", config.identity);
    println!("RPC: {}\n", config.rpc_url);

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

    // Step 1: Fetch inflation rewards by epoch (with caching)
    println!("Fetching inflation rewards...");
    let rewards =
        fetch_rewards_with_cache(&cache, &config, start_epoch, end_epoch, args.no_cache).await?;
    println!("  Found {} epochs with rewards\n", rewards.len());

    // Step 2: Fetch all SOL transfers to/from our accounts (with caching)
    println!("Loading transaction history...");
    let transfers =
        fetch_transfers_with_cache(&cache, &config, args.no_cache, args.verbose).await?;
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

    // Step 6: Load vote costs from cache
    println!("Loading vote costs...");
    let vote_costs = cache.get_vote_costs(start_epoch, end_epoch).await?;
    if vote_costs.is_empty() {
        println!(
            "  No vote costs cached (use 'vote-costs import' or 'vote-costs estimate' to add)\n"
        );
    } else {
        let total_vote_cost = vote_costs::total_vote_costs_sol(&vote_costs);
        println!(
            "  Loaded {} epochs totaling {:.6} SOL in vote fees\n",
            vote_costs.len(),
            total_vote_cost
        );
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
    reports::generate_all_reports(
        &args.output_dir,
        &rewards,
        &categorized,
        &mev_claims,
        &leader_fees,
        &vote_costs,
        &all_expenses,
        &price_cache,
        &config,
        args.year,
    )?;

    // Step 10: Print summary
    reports::print_summary(
        &rewards,
        &categorized,
        &mev_claims,
        &leader_fees,
        &vote_costs,
        &all_expenses,
        &price_cache,
        &config,
        args.year,
    );

    println!("\nDone! Reports written to: {}", args.output_dir.display());

    Ok(())
}

/// Fetch rewards with caching - only fetch missing epochs
async fn fetch_rewards_with_cache(
    cache: &Cache,
    config: &config::Config,
    start_epoch: u64,
    end_epoch: u64,
    no_cache: bool,
) -> Result<Vec<transactions::EpochReward>> {
    if no_cache {
        // Fetch everything fresh
        let rewards =
            transactions::fetch_inflation_rewards(config, start_epoch, Some(end_epoch)).await?;
        cache.store_epoch_rewards(&rewards).await?;
        return Ok(rewards);
    }

    // Get cached rewards for completed epochs (exclude current)
    let mut rewards = cache.get_epoch_rewards(start_epoch, end_epoch).await?;
    let cached_count = rewards.len();

    // Find missing epochs
    let missing = cache
        .get_missing_reward_epochs(start_epoch, end_epoch)
        .await?;

    if !missing.is_empty() {
        println!("    Fetching {} missing epochs...", missing.len());

        // Fetch missing epochs one by one
        for epoch in missing {
            if let Ok(mut fetched) =
                transactions::fetch_inflation_rewards(config, epoch, Some(epoch)).await
            {
                // Store in cache (completed epochs only)
                cache.store_epoch_rewards(&fetched).await?;
                rewards.append(&mut fetched);
            }
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

    // Get cached claims for completed epochs
    let completed_end = end_epoch.min(current_epoch.saturating_sub(1));
    let mut claims = cache.get_mev_claims(start_epoch, completed_end).await?;
    let cached_count = claims.len();

    // Check if we need to fetch fresh data
    let missing = cache
        .get_missing_mev_epochs(start_epoch, completed_end)
        .await?;
    let need_current = end_epoch >= current_epoch;

    if !missing.is_empty() || need_current || cached_count == 0 {
        // Jito API returns all epochs at once, so fetch fresh
        let fresh_claims = jito::fetch_mev_claims(config).await?;

        // Store completed epochs in cache
        let completed: Vec<_> = fresh_claims
            .iter()
            .filter(|c| c.epoch < current_epoch)
            .cloned()
            .collect();
        cache.store_mev_claims(&completed).await?;

        // Filter to requested range
        claims = fresh_claims
            .into_iter()
            .filter(|c| c.epoch >= start_epoch && c.epoch <= end_epoch)
            .collect();
    } else if cached_count > 0 {
        println!("    ({} epochs from cache)", cached_count);
    }

    // Sort by epoch
    claims.sort_by_key(|c| c.epoch);

    Ok(claims)
}

/// Fetch leader fees with caching - this is the expensive one!
async fn fetch_leader_fees_with_cache(
    cache: &Cache,
    config: &config::Config,
    start_epoch: u64,
    end_epoch: u64,
    current_epoch: u64,
    no_cache: bool,
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
        println!(
            "    Fetching {} missing epochs (this may take a while)...",
            missing.len()
        );

        // Fetch missing epochs
        for epoch in &missing {
            if let Ok(fetched) = leader_fees::fetch_leader_fees(config, *epoch, Some(*epoch)).await
            {
                // Store completed epochs in cache
                let completed: Vec<_> = fetched
                    .iter()
                    .filter(|f| f.epoch < current_epoch)
                    .cloned()
                    .collect();
                cache.store_leader_fees(&completed).await?;
                fees.extend(fetched);
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

/// Fetch prices with caching
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

    // Fetch fresh prices (will fill in any gaps)
    let fresh_prices = prices::fetch_historical_prices(rewards, transfers, api_key).await?;

    // Merge and store new prices
    for (date, price) in &fresh_prices {
        price_cache.insert(date.clone(), *price);
    }

    cache.store_prices(&price_cache).await?;

    if cached_count > 0 && cached_count < price_cache.len() {
        println!(
            "    ({} from cache, {} new)",
            cached_count,
            price_cache.len() - cached_count
        );
    }

    Ok(price_cache)
}

/// Fetch SOL transfers with caching - only fetch new transactions since last run
async fn fetch_transfers_with_cache(
    cache: &Cache,
    config: &config::Config,
    no_cache: bool,
    verbose: bool,
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
    for (label, account) in transactions::get_tracked_accounts(config) {
        let latest_slot = cache.get_latest_transfer_slot(label).await?;

        if verbose {
            println!("    {}: latest cached slot = {:?}", label, latest_slot);
        }

        // Fetch new transfers (will stop when hitting already-cached slot)
        let new_transfers = transactions::fetch_transfers_for_account(
            config,
            &account,
            label,
            latest_slot,
            verbose,
        )
        .await?;

        if !new_transfers.is_empty() {
            if verbose {
                println!(
                    "    {}: fetched {} new transfers",
                    label,
                    new_transfers.len()
                );
            }

            // Store new transfers (all of them, for tracking per-account progress)
            cache.store_transfers(&new_transfers, label).await?;

            // Add unique transfers to our collection (deduplicate across accounts)
            for transfer in new_transfers {
                if seen_signatures.insert(transfer.signature.clone()) {
                    new_count += 1;
                    all_transfers.push(transfer);
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
