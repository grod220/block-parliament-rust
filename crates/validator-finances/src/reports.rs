//! Report generation (CSV outputs and console summary)

use anyhow::Result;
use csv::Writer;
use std::collections::HashMap;
use std::path::Path;

use crate::config::Config;
use crate::constants;
use crate::expenses::{Expense, ExpenseCategory};
use crate::jito::MevClaim;
use crate::leader_fees::EpochLeaderFees;
use crate::prices::{get_price, PriceCache};
use crate::transactions::{CategorizedTransfers, EpochReward};
use crate::vote_costs::EpochVoteCost;

/// Bundled report data to reduce function argument counts
pub struct ReportData<'a> {
    pub rewards: &'a [EpochReward],
    pub categorized: &'a CategorizedTransfers,
    pub mev_claims: &'a [MevClaim],
    pub leader_fees: &'a [EpochLeaderFees],
    pub vote_costs: &'a [EpochVoteCost],
    pub expenses: &'a [Expense],
    pub prices: &'a PriceCache,
    pub config: &'a Config,
}

/// Generate all CSV reports
pub fn generate_all_reports(
    output_dir: &Path,
    data: &ReportData,
    year_filter: Option<i32>,
) -> Result<()> {
    generate_income_ledger(
        output_dir,
        data.rewards,
        data.categorized,
        data.mev_claims,
        data.leader_fees,
        data.prices,
    )?;
    generate_expense_ledger(
        output_dir,
        data.expenses,
        data.vote_costs,
        data.prices,
        data.config,
    )?;
    generate_treasury_ledger(output_dir, data.categorized, data.prices)?;
    generate_summary(output_dir, data, year_filter)?;

    Ok(())
}

/// Generate income_ledger.csv
fn generate_income_ledger(
    output_dir: &Path,
    rewards: &[EpochReward],
    categorized: &CategorizedTransfers,
    mev_claims: &[MevClaim],
    leader_fees: &[EpochLeaderFees],
    prices: &PriceCache,
) -> Result<()> {
    let path = output_dir.join(constants::INCOME_LEDGER_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    // Header
    wtr.write_record([
        "Date",
        "Epoch",
        "Source",
        "From_Address",
        "From_Label",
        "Amount_SOL",
        "USD_Price",
        "USD_Value",
        "Tx_Signature",
        "Notes",
    ])?;

    // Commission rewards
    for reward in rewards {
        let date = reward.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = reward.amount_sol * price;

        wtr.write_record([
            date,
            &reward.epoch.to_string(),
            "Commission",
            "Vote Account",
            "Inflation Reward",
            &format!("{:.6}", reward.amount_sol),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &format!("epoch-{}", reward.epoch),
            &format!("{}% commission on delegator rewards", reward.commission),
        ])?;
    }

    // Note: SFDP reimbursements are NOT included in income - they are expense offsets

    // MEV: Use Jito API claims as source of truth to avoid double-counting.
    // mev_deposits (transfers) and mev_claims (API) represent the same income.
    // Only use mev_deposits as fallback when mev_claims is empty.

    // MEV deposits (from transfer detection) - only when no Jito API data
    // These are fallback data when Jito API doesn't have epoch info
    for transfer in &categorized.mev_deposits {
        // Skip if we have Jito API data for this epoch (avoid double-counting)
        // Note: transfers don't have epoch directly, so we include them only if
        // mev_claims is empty (no API data at all)
        if !mev_claims.is_empty() {
            continue;
        }

        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "",
            "Jito MEV",
            &transfer.from.to_string(),
            &transfer.from_label,
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "MEV tip distribution from Jito (fallback)",
        ])?;
    }

    // MEV claims from Jito API (primary source)
    for claim in mev_claims {
        let date = claim.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = claim.amount_sol * price;

        wtr.write_record([
            date,
            &claim.epoch.to_string(),
            "Jito MEV",
            "Jito Tip Distribution",
            "Vote Account",
            &format!("{:.6}", claim.amount_sol),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &format!("epoch-{}", claim.epoch),
            &format!(
                "{}% commission on {:.4} SOL tips",
                if claim.total_tips_lamports > 0 {
                    (claim.commission_lamports as f64 / claim.total_tips_lamports as f64 * 100.0)
                        .round() as u64
                } else {
                    0
                },
                claim.total_tips_lamports as f64 / 1e9
            ),
        ])?;
    }

    // Leader slot fees (block production rewards)
    for fees in leader_fees {
        let date = fees.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = fees.total_fees_sol * price;

        wtr.write_record([
            date,
            &fees.epoch.to_string(),
            "Leader Fees",
            "Identity Account",
            "Block Production",
            &format!("{:.6}", fees.total_fees_sol),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &format!("epoch-{}", fees.epoch),
            &format!(
                "{} blocks produced, {} skipped",
                fees.blocks_produced, fees.skipped_slots
            ),
        ])?;
    }

    wtr.flush()?;
    println!("  Generated: {}", path.display());

    Ok(())
}

/// Generate expense_ledger.csv
fn generate_expense_ledger(
    output_dir: &Path,
    expenses: &[Expense],
    vote_costs: &[EpochVoteCost],
    prices: &PriceCache,
    config: &Config,
) -> Result<()> {
    let path = output_dir.join(constants::EXPENSE_LEDGER_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    // Header
    wtr.write_record([
        "Date",
        "Epoch",
        "Vendor",
        "Category",
        "Description",
        "Amount_SOL",
        "Amount_USD",
        "Paid_With",
        "SFDP_Coverage",
        "Net_Amount_USD",
        "Invoice_ID",
    ])?;

    // Vote costs per epoch (actual on-chain data)
    for cost in vote_costs {
        let date = cost.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let gross_usd = cost.total_fee_sol * price;

        // Calculate SFDP coverage for this epoch's date
        let parsed_date =
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap_or_else(|_| {
                chrono::NaiveDate::parse_from_str(constants::FALLBACK_DATE, "%Y-%m-%d").unwrap()
            });
        let coverage = config.sfdp_coverage_percent(&parsed_date);
        let net_usd = gross_usd * (1.0 - coverage);

        wtr.write_record([
            date,
            &cost.epoch.to_string(),
            "Solana Network",
            "VoteFees",
            &format!("{} votes ({})", cost.vote_count, cost.source),
            &format!("{:.6}", cost.total_fee_sol),
            &format!("{:.2}", gross_usd),
            "SOL",
            &format!("{:.0}%", coverage * 100.0),
            &format!("{:.2}", net_usd),
            "",
        ])?;
    }

    // Off-chain expenses (hosting, contractors, etc.)
    for expense in expenses {
        let expense_usd = expense.amount_usd;
        wtr.write_record([
            &expense.date,
            "", // No epoch for off-chain expenses
            &expense.vendor,
            &expense.category.to_string(),
            &expense.description,
            "", // No SOL amount
            &format!("{:.2}", expense_usd),
            &expense.paid_with,
            "", // No SFDP coverage for off-chain expenses
            &format!("{:.2}", expense_usd),
            expense.invoice_id.as_deref().unwrap_or(""),
        ])?;
    }

    wtr.flush()?;
    println!("  Generated: {}", path.display());

    Ok(())
}

/// Generate treasury_ledger.csv (transfers, seeding, withdrawals)
fn generate_treasury_ledger(
    output_dir: &Path,
    categorized: &CategorizedTransfers,
    prices: &PriceCache,
) -> Result<()> {
    let path = output_dir.join(constants::TREASURY_LEDGER_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    // Header
    wtr.write_record([
        "Date",
        "Type",
        "From_Address",
        "From_Label",
        "To_Address",
        "To_Label",
        "Amount_SOL",
        "USD_Value",
        "Tx_Signature",
        "Notes",
    ])?;

    // Initial seeding
    for transfer in &categorized.seeding {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Capital Contribution",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Initial validator seeding",
        ])?;
    }

    // Vote funding (internal transfers)
    for transfer in &categorized.vote_funding {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Internal Transfer",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Vote account funding",
        ])?;
    }

    // Withdrawals
    for transfer in &categorized.withdrawals {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Withdrawal",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Withdrawal to exchange/personal",
        ])?;
    }

    // Other transfers
    for transfer in &categorized.other {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Other",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Uncategorized transfer",
        ])?;
    }

    wtr.flush()?;
    println!("  Generated: {}", path.display());

    Ok(())
}

/// Generate summary.csv (monthly P&L with annual summaries)
fn generate_summary(output_dir: &Path, data: &ReportData, year_filter: Option<i32>) -> Result<()> {
    let path = output_dir.join(constants::SUMMARY_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    // Aggregate by month
    let mut monthly: HashMap<String, MonthlyData> = HashMap::new();

    // Commission
    for reward in data.rewards {
        if let Some(date) = &reward.date {
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.commission_sol += reward.amount_sol;
            entry.commission_usd += reward.amount_sol * price;
        }
    }

    // SFDP reimbursements
    for transfer in &data.categorized.sfdp_reimbursements {
        if let Some(date) = &transfer.date {
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.sfdp_sol += transfer.amount_sol;
            entry.sfdp_usd += transfer.amount_sol * price;
        }
    }

    // MEV: Use Jito API claims as source of truth to avoid double-counting.
    // Only use mev_deposits as fallback when mev_claims is empty.
    if data.mev_claims.is_empty() {
        // Fallback: use transfer detection when no Jito API data
        for transfer in &data.categorized.mev_deposits {
            if let Some(date) = &transfer.date {
                let month = &date[..7];
                let price = get_price(data.prices, date);
                let entry = monthly.entry(month.to_string()).or_default();
                entry.mev_sol += transfer.amount_sol;
                entry.mev_usd += transfer.amount_sol * price;
            }
        }
    } else {
        // Primary: use Jito API data (per-epoch, accurate)
        for claim in data.mev_claims {
            if let Some(date) = &claim.date {
                let month = &date[..7];
                let price = get_price(data.prices, date);
                let entry = monthly.entry(month.to_string()).or_default();
                entry.mev_sol += claim.amount_sol;
                entry.mev_usd += claim.amount_sol * price;
            }
        }
    }

    // Leader fees from block production
    for fees in data.leader_fees {
        if let Some(date) = &fees.date {
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.leader_fees_sol += fees.total_fees_sol;
            entry.leader_fees_usd += fees.total_fees_sol * price;
        }
    }

    // Vote costs by month (with SFDP coverage calculation)
    for cost in data.vote_costs {
        if let Some(date) = &cost.date {
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let gross_usd = cost.total_fee_sol * price;

            // Calculate SFDP coverage for net cost
            let parsed_date =
                chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap_or_else(|_| {
                    chrono::NaiveDate::parse_from_str(constants::FALLBACK_DATE, "%Y-%m-%d").unwrap()
                });
            let coverage = data.config.sfdp_coverage_percent(&parsed_date);
            let net_usd = gross_usd * (1.0 - coverage);

            let entry = monthly.entry(month.to_string()).or_default();
            entry.vote_costs_sol += cost.total_fee_sol;
            entry.vote_costs_gross_usd += gross_usd;
            entry.vote_costs_net_usd += net_usd;
        }
    }

    // Expenses by month
    for expense in data.expenses {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&expense.date, "%Y-%m-%d") {
            let month = date.format("%Y-%m").to_string();
            let entry = monthly.entry(month).or_default();
            entry.other_expenses_usd += expense.amount_usd;
        }
    }

    // Header
    wtr.write_record([
        "Month",
        "Commission_SOL",
        "Commission_USD",
        "Leader_Fees_SOL",
        "Leader_Fees_USD",
        "MEV_SOL",
        "MEV_USD",
        "Total_Revenue_USD",
        "Vote_Costs_SOL",
        "Vote_Costs_Gross_USD",
        "SFDP_Offset_USD",
        "Vote_Costs_Net_USD",
        "Other_Expenses_USD",
        "Total_Expenses_USD",
        "Net_Profit_USD",
        "YTD_Profit_USD",
    ])?;

    let mut months: Vec<_> = monthly.keys().cloned().collect();
    months.sort();

    // Filter by year if specified
    let months: Vec<_> = if let Some(year) = year_filter {
        let year_prefix = format!("{}-", year);
        months
            .into_iter()
            .filter(|m| m.starts_with(&year_prefix))
            .collect()
    } else {
        months
    };

    // Track annual totals for summary rows
    let mut annual_totals: HashMap<String, MonthlyData> = HashMap::new();
    let mut ytd = 0.0;
    let mut current_year: Option<String> = None;

    for month in &months {
        let year = &month[..4];
        let data = &monthly[month];
        // SFDP is expense offset, not revenue
        let total_revenue = data.commission_usd + data.leader_fees_usd + data.mev_usd;
        let total_expenses = data.vote_costs_net_usd + data.other_expenses_usd;
        let net_profit = total_revenue - total_expenses;

        // Reset YTD at year boundary
        if current_year.as_deref() != Some(year) {
            current_year = Some(year.to_string());
            ytd = 0.0;
        }
        ytd += net_profit;

        // Accumulate annual totals
        let annual = annual_totals.entry(year.to_string()).or_default();
        annual.commission_sol += data.commission_sol;
        annual.commission_usd += data.commission_usd;
        annual.leader_fees_sol += data.leader_fees_sol;
        annual.leader_fees_usd += data.leader_fees_usd;
        annual.mev_sol += data.mev_sol;
        annual.mev_usd += data.mev_usd;
        annual.sfdp_sol += data.sfdp_sol;
        annual.sfdp_usd += data.sfdp_usd;
        annual.vote_costs_sol += data.vote_costs_sol;
        annual.vote_costs_gross_usd += data.vote_costs_gross_usd;
        annual.vote_costs_net_usd += data.vote_costs_net_usd;
        annual.other_expenses_usd += data.other_expenses_usd;

        let sfdp_offset = data.vote_costs_gross_usd - data.vote_costs_net_usd;
        wtr.write_record([
            month,
            &format!("{:.4}", data.commission_sol),
            &format!("{:.2}", data.commission_usd),
            &format!("{:.4}", data.leader_fees_sol),
            &format!("{:.2}", data.leader_fees_usd),
            &format!("{:.4}", data.mev_sol),
            &format!("{:.2}", data.mev_usd),
            &format!("{:.2}", total_revenue),
            &format!("{:.4}", data.vote_costs_sol),
            &format!("{:.2}", data.vote_costs_gross_usd),
            &format!("{:.2}", sfdp_offset),
            &format!("{:.2}", data.vote_costs_net_usd),
            &format!("{:.2}", data.other_expenses_usd),
            &format!("{:.2}", total_expenses),
            &format!("{:.2}", net_profit),
            &format!("{:.2}", ytd),
        ])?;
    }

    // Write annual summary rows
    let mut years: Vec<_> = annual_totals.keys().cloned().collect();
    years.sort();

    for year in &years {
        let data = &annual_totals[year];
        // SFDP is expense offset, not revenue
        let total_revenue = data.commission_usd + data.leader_fees_usd + data.mev_usd;
        let total_expenses = data.vote_costs_net_usd + data.other_expenses_usd;
        let net_profit = total_revenue - total_expenses;

        let sfdp_offset = data.vote_costs_gross_usd - data.vote_costs_net_usd;
        wtr.write_record([
            &format!("{} TOTAL", year),
            &format!("{:.4}", data.commission_sol),
            &format!("{:.2}", data.commission_usd),
            &format!("{:.4}", data.leader_fees_sol),
            &format!("{:.2}", data.leader_fees_usd),
            &format!("{:.4}", data.mev_sol),
            &format!("{:.2}", data.mev_usd),
            &format!("{:.2}", total_revenue),
            &format!("{:.4}", data.vote_costs_sol),
            &format!("{:.2}", data.vote_costs_gross_usd),
            &format!("{:.2}", sfdp_offset),
            &format!("{:.2}", data.vote_costs_net_usd),
            &format!("{:.2}", data.other_expenses_usd),
            &format!("{:.2}", total_expenses),
            &format!("{:.2}", net_profit),
            "", // No YTD for annual rows
        ])?;
    }

    wtr.flush()?;
    println!("  Generated: {}", path.display());

    Ok(())
}

#[derive(Default)]
struct MonthlyData {
    commission_sol: f64,
    commission_usd: f64,
    leader_fees_sol: f64,
    leader_fees_usd: f64,
    mev_sol: f64,
    mev_usd: f64,
    sfdp_sol: f64,
    sfdp_usd: f64,
    vote_costs_sol: f64,
    vote_costs_gross_usd: f64,
    vote_costs_net_usd: f64,
    other_expenses_usd: f64,
}

/// Normalize -0.0 to 0.0 for cleaner display
fn normalize_zero(val: f64) -> f64 {
    if val == 0.0 {
        0.0
    } else {
        val
    }
}

/// Print summary to console
pub fn print_summary(data: &ReportData, year_filter: Option<i32>) {
    // Helper to check if a date matches the year filter
    let matches_year = |date: &str| -> bool {
        if let Some(year) = year_filter {
            date.starts_with(&format!("{}-", year))
        } else {
            true
        }
    };

    println!("\n============================================================");
    if let Some(year) = year_filter {
        println!("                FINANCIAL SUMMARY ({})", year);
    } else {
        println!("                    FINANCIAL SUMMARY");
    }
    println!("============================================================\n");

    // Calculate totals (filtered by year if specified)
    let total_commission_sol: f64 = data
        .rewards
        .iter()
        .filter(|r| r.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|r| r.amount_sol)
        .sum();
    let total_commission_usd: f64 = data
        .rewards
        .iter()
        .filter(|r| r.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|r| {
            let price = get_price(
                data.prices,
                r.date.as_deref().unwrap_or(constants::FALLBACK_DATE),
            );
            r.amount_sol * price
        })
        .sum();

    // MEV: Use Jito API claims as source of truth to avoid double-counting.
    // Only use mev_deposits as fallback when mev_claims is empty.
    let (total_mev_sol, total_mev_usd) = if data.mev_claims.is_empty() {
        // Fallback: use transfer detection
        let mev_sol: f64 = data
            .categorized
            .mev_deposits
            .iter()
            .filter(|t| t.date.as_deref().map(&matches_year).unwrap_or(false))
            .map(|t| t.amount_sol)
            .sum();
        let mev_usd: f64 = data
            .categorized
            .mev_deposits
            .iter()
            .filter(|t| t.date.as_deref().map(&matches_year).unwrap_or(false))
            .map(|t| {
                let price = get_price(
                    data.prices,
                    t.date.as_deref().unwrap_or(constants::FALLBACK_DATE),
                );
                t.amount_sol * price
            })
            .sum();
        (mev_sol, mev_usd)
    } else {
        // Primary: use Jito API data
        let mev_sol: f64 = data
            .mev_claims
            .iter()
            .filter(|c| c.date.as_deref().map(&matches_year).unwrap_or(false))
            .map(|c| c.amount_sol)
            .sum();
        let mev_usd: f64 = data
            .mev_claims
            .iter()
            .filter(|c| c.date.as_deref().map(&matches_year).unwrap_or(false))
            .map(|c| {
                let price = get_price(
                    data.prices,
                    c.date.as_deref().unwrap_or(constants::FALLBACK_DATE),
                );
                c.amount_sol * price
            })
            .sum();
        (mev_sol, mev_usd)
    };

    // Leader fees from block production
    let total_leader_fees_sol: f64 = data
        .leader_fees
        .iter()
        .filter(|f| f.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|f| f.total_fees_sol)
        .sum();
    let total_leader_fees_usd: f64 = data
        .leader_fees
        .iter()
        .filter(|f| f.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|f| {
            let price = get_price(
                data.prices,
                f.date.as_deref().unwrap_or(constants::FALLBACK_DATE),
            );
            f.total_fees_sol * price
        })
        .sum();

    // Note: SFDP is tracked as expense offset, not calculated separately for revenue

    let total_seeding_sol: f64 = data
        .categorized
        .seeding
        .iter()
        .filter(|t| t.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|t| t.amount_sol)
        .sum();

    // Vote costs (with SFDP coverage)
    let total_vote_costs_sol: f64 = data
        .vote_costs
        .iter()
        .filter(|c| c.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|c| c.total_fee_sol)
        .sum();
    let mut total_vote_costs_gross_usd = 0.0;
    let mut total_vote_costs_net_usd = 0.0;

    for cost in data.vote_costs {
        let date = cost.date.as_deref().unwrap_or(constants::FALLBACK_DATE);
        if !matches_year(date) {
            continue;
        }
        let price = get_price(data.prices, date);
        let gross_usd = cost.total_fee_sol * price;

        // Calculate SFDP coverage
        let parsed_date =
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap_or_else(|_| {
                chrono::NaiveDate::parse_from_str(constants::FALLBACK_DATE, "%Y-%m-%d").unwrap()
            });
        let coverage = data.config.sfdp_coverage_percent(&parsed_date);
        let net_usd = gross_usd * (1.0 - coverage);

        total_vote_costs_gross_usd += gross_usd;
        total_vote_costs_net_usd += net_usd;
    }

    // Other expenses (hosting, contractors, etc.)
    let total_other_expenses: f64 = data
        .expenses
        .iter()
        .filter(|e| matches_year(&e.date))
        .map(|e| e.amount_usd)
        .sum();
    let hosting_expenses: f64 = data
        .expenses
        .iter()
        .filter(|e| e.category == ExpenseCategory::Hosting && matches_year(&e.date))
        .map(|e| e.amount_usd)
        .sum();
    let contractor_expenses: f64 = data
        .expenses
        .iter()
        .filter(|e| e.category == ExpenseCategory::Contractor && matches_year(&e.date))
        .map(|e| e.amount_usd)
        .sum();

    // SFDP is an expense offset, not revenue
    let total_revenue_usd = total_commission_usd + total_leader_fees_usd + total_mev_usd;
    let total_expenses_usd = total_vote_costs_net_usd + total_other_expenses;
    let net_profit = total_revenue_usd - total_expenses_usd;

    // Normalize values to avoid displaying -0.0
    let total_commission_sol = normalize_zero(total_commission_sol);
    let total_commission_usd = normalize_zero(total_commission_usd);
    let total_leader_fees_sol = normalize_zero(total_leader_fees_sol);
    let total_leader_fees_usd = normalize_zero(total_leader_fees_usd);
    let total_mev_sol = normalize_zero(total_mev_sol);
    let total_mev_usd = normalize_zero(total_mev_usd);
    let total_seeding_sol = normalize_zero(total_seeding_sol);

    println!("REVENUE:");
    println!(
        "  Commission:         {:>10.4} SOL  ${:>10.2}",
        total_commission_sol, total_commission_usd
    );
    println!(
        "  Leader Fees:        {:>10.4} SOL  ${:>10.2}",
        total_leader_fees_sol, total_leader_fees_usd
    );
    println!(
        "  Jito MEV:           {:>10.4} SOL  ${:>10.2}",
        total_mev_sol, total_mev_usd
    );
    println!("  ─────────────────────────────────────────────");
    println!(
        "  Total Revenue:      {:>10.4} SOL  ${:>10.2}",
        total_commission_sol + total_leader_fees_sol + total_mev_sol,
        total_revenue_usd
    );

    println!("\nEXPENSES:");
    println!(
        "  Vote Fees (gross):  {:>10.4} SOL  ${:>10.2}",
        total_vote_costs_sol, total_vote_costs_gross_usd
    );
    println!(
        "  SFDP Offset:                   -${:>10.2}",
        total_vote_costs_gross_usd - total_vote_costs_net_usd
    );
    println!(
        "  Vote Fees (net):                ${:>10.2}",
        total_vote_costs_net_usd
    );
    println!(
        "  Hosting:                        ${:>10.2}",
        hosting_expenses
    );
    println!(
        "  Contractor:                     ${:>10.2}",
        contractor_expenses
    );
    println!("  ─────────────────────────────────────────────");
    println!(
        "  Total Expenses:                 ${:>10.2}",
        total_expenses_usd
    );

    println!("\nPROFIT/LOSS:");
    println!("  Net Profit:                     ${:>10.2}", net_profit);

    println!("\nCAPITAL:");
    println!("  Initial Seeding:    {:>10.4} SOL", total_seeding_sol);
    println!(
        "  Transfers found:    {}",
        data.categorized.seeding.len() + data.categorized.vote_funding.len()
    );

    println!("============================================================");
}
