//! Off-chain expense tracking for hosting, contractors, and other costs
//!
//! Expenses are stored in the SQLite database and can be managed via CLI commands.

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Expense entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    /// Database ID (None for new expenses not yet saved)
    #[serde(skip)]
    pub id: Option<i64>,
    pub date: String,
    pub vendor: String,
    pub category: ExpenseCategory,
    pub description: String,
    pub amount_usd: f64,
    pub paid_with: String, // "USD", "SOL", "Credit Card"
    pub invoice_id: Option<String>,
}

/// Expense category
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ExpenseCategory {
    Hosting,
    Contractor,
    Hardware,
    Software,
    VoteFees,
    Other,
}

impl std::fmt::Display for ExpenseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpenseCategory::Hosting => write!(f, "Hosting"),
            ExpenseCategory::Contractor => write!(f, "Contractor"),
            ExpenseCategory::Hardware => write!(f, "Hardware"),
            ExpenseCategory::Software => write!(f, "Software"),
            ExpenseCategory::VoteFees => write!(f, "Vote Fees"),
            ExpenseCategory::Other => write!(f, "Other"),
        }
    }
}

/// Load expenses from a CSV file (for importing/migration)
pub fn load_from_csv(path: &Path) -> Result<Vec<Expense>> {
    let mut rdr = csv::Reader::from_path(path)?;
    let mut expenses = Vec::new();
    for result in rdr.deserialize() {
        let mut expense: Expense = result?;
        expense.id = None; // CSV imports don't have IDs
        expenses.push(expense);
    }
    Ok(expenses)
}

/// Export expenses to CSV (for backup)
pub fn export_to_csv(expenses: &[Expense], path: &Path) -> Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;
    for expense in expenses {
        wtr.serialize(expense)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Calculate total expenses by category
#[allow(dead_code)]
pub fn expenses_by_category(expenses: &[Expense]) -> Vec<(ExpenseCategory, f64)> {
    use std::collections::HashMap;
    let mut totals: HashMap<ExpenseCategory, f64> = HashMap::new();

    for expense in expenses {
        *totals.entry(expense.category).or_insert(0.0) += expense.amount_usd;
    }

    let mut result: Vec<_> = totals.into_iter().collect();
    result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    result
}

/// Calculate total expenses by month
#[allow(dead_code)]
pub fn expenses_by_month(expenses: &[Expense]) -> Vec<(String, f64)> {
    use std::collections::HashMap;
    let mut totals: HashMap<String, f64> = HashMap::new();

    for expense in expenses {
        if let Ok(date) = NaiveDate::parse_from_str(&expense.date, "%Y-%m-%d") {
            let month = date.format("%Y-%m").to_string();
            *totals.entry(month).or_insert(0.0) += expense.amount_usd;
        }
    }

    let mut result: Vec<_> = totals.into_iter().collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

/// Get total expenses
pub fn total_expenses(expenses: &[Expense]) -> f64 {
    expenses.iter().map(|e| e.amount_usd).sum()
}

/// Recurring expense template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringExpense {
    /// Database ID (None for new recurring expenses not yet saved)
    #[serde(skip)]
    pub id: Option<i64>,
    pub vendor: String,
    pub category: ExpenseCategory,
    pub description: String,
    pub amount_usd: f64,
    pub paid_with: String,
    /// First month this expense applies (YYYY-MM-DD, day is used for billing day)
    pub start_date: String,
    /// Last month this expense applies (None = ongoing)
    pub end_date: Option<String>,
}

impl RecurringExpense {
    /// Get the billing day from start_date
    pub fn billing_day(&self) -> u32 {
        NaiveDate::parse_from_str(&self.start_date, "%Y-%m-%d")
            .map(|d| d.day())
            .unwrap_or(1)
    }
}

/// Expand recurring expenses into individual expense entries for a date range
pub fn expand_recurring_expenses(
    recurring: &[RecurringExpense],
    start_month: &str, // YYYY-MM
    end_month: &str,   // YYYY-MM
) -> Vec<Expense> {
    let mut expenses = Vec::new();

    let start = NaiveDate::parse_from_str(&format!("{}-01", start_month), "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    let end = NaiveDate::parse_from_str(&format!("{}-01", end_month), "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());

    for rec in recurring {
        let rec_start = NaiveDate::parse_from_str(&rec.start_date, "%Y-%m-%d")
            .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        let rec_end = rec
            .end_date
            .as_ref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());

        let billing_day = rec.billing_day();

        // Iterate through each month in the range
        let mut current = start;
        while current <= end {
            // Check if this month falls within the recurring expense's active period
            let current_month_start = current;
            let rec_start_month = NaiveDate::from_ymd_opt(rec_start.year(), rec_start.month(), 1).unwrap();

            if current_month_start >= rec_start_month {
                // Check end date if present
                let within_end = rec_end.map_or(true, |end_date| {
                    let end_month_start = NaiveDate::from_ymd_opt(end_date.year(), end_date.month(), 1).unwrap();
                    current_month_start <= end_month_start
                });

                if within_end {
                    // Generate expense for this month
                    // Handle months where billing_day doesn't exist (e.g., Feb 30 -> Feb 28)
                    let days_in_month = days_in_month(current.year(), current.month());
                    let actual_day = billing_day.min(days_in_month);
                    let expense_date = NaiveDate::from_ymd_opt(current.year(), current.month(), actual_day).unwrap();

                    expenses.push(Expense {
                        id: None,
                        date: expense_date.format("%Y-%m-%d").to_string(),
                        vendor: rec.vendor.clone(),
                        category: rec.category,
                        description: rec.description.clone(),
                        amount_usd: rec.amount_usd,
                        paid_with: rec.paid_with.clone(),
                        invoice_id: None,
                    });
                }
            }

            // Move to next month
            current = if current.month() == 12 {
                NaiveDate::from_ymd_opt(current.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(current.year(), current.month() + 1, 1).unwrap()
            };
        }
    }

    expenses
}

/// Get the number of days in a month
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}
