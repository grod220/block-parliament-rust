//! Off-chain expense tracking for hosting, contractors, and other costs
//!
//! Expenses are stored in the SQLite database and can be managed via CLI commands.

use anyhow::Result;
use chrono::NaiveDate;
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
