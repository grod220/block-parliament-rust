//! Notion API integration for contractor hours tracking
//!
//! Fetches hours log entries from a Notion database and converts them
//! to expense records for the P&L report.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::NotionConfig;
use crate::expenses::{Expense, ExpenseCategory};

const NOTION_API_BASE: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";

/// Hours log entry from Notion database
#[derive(Debug, Clone)]
pub struct HoursLogEntry {
    pub page_id: String,
    pub description: String,
    pub date: String,
    pub hours: f64,
    pub amount_usd: f64,
    pub paid: bool,
}

// =============================================================================
// Notion API Response Types
// =============================================================================

#[derive(Debug, Deserialize)]
struct QueryResponse {
    results: Vec<PageResult>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PageResult {
    id: String,
    properties: PageProperties,
}

#[derive(Debug, Deserialize)]
struct PageProperties {
    #[serde(rename = "Description")]
    description: TitleProperty,
    #[serde(rename = "Date")]
    date: DateProperty,
    #[serde(rename = "Hours worked")]
    hours_worked: NumberProperty,
    #[serde(rename = "Paid")]
    paid: CheckboxProperty,
    #[serde(rename = "Amount earned")]
    amount_earned: FormulaProperty,
}

#[derive(Debug, Deserialize)]
struct TitleProperty {
    title: Vec<RichText>,
}

#[derive(Debug, Deserialize)]
struct RichText {
    plain_text: String,
}

#[derive(Debug, Deserialize)]
struct DateProperty {
    date: Option<DateValue>,
}

#[derive(Debug, Deserialize)]
struct DateValue {
    start: String,
}

#[derive(Debug, Deserialize)]
struct NumberProperty {
    number: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CheckboxProperty {
    checkbox: bool,
}

#[derive(Debug, Deserialize)]
struct FormulaProperty {
    formula: FormulaValue,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum FormulaValue {
    #[serde(rename = "string")]
    String { string: Option<String> },
    #[serde(rename = "number")]
    Number { number: Option<f64> },
}

// =============================================================================
// API Functions
// =============================================================================

/// Fetch all hours log entries from Notion
pub async fn fetch_hours_log(config: &NotionConfig) -> Result<Vec<HoursLogEntry>> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/databases/{}/query",
        NOTION_API_BASE, config.hours_database_id
    );

    let mut all_entries = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut body = serde_json::json!({});
        if let Some(ref c) = cursor {
            body["start_cursor"] = serde_json::json!(c);
        }

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_token))
            .header("Notion-Version", NOTION_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to query Notion database")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Notion API error {}: {}", status, text);
        }

        let data: QueryResponse = response
            .json()
            .await
            .context("Failed to parse Notion response")?;

        for page in data.results {
            if let Some(entry) = parse_page_to_entry(&page) {
                all_entries.push(entry);
            }
        }

        if data.has_more {
            cursor = data.next_cursor;
        } else {
            break;
        }
    }

    // Sort by date (newest first)
    all_entries.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(all_entries)
}

/// Parse a Notion page into a HoursLogEntry
fn parse_page_to_entry(page: &PageResult) -> Option<HoursLogEntry> {
    let description = page
        .properties
        .description
        .title
        .first()
        .map(|t| t.plain_text.clone())
        .unwrap_or_default();

    let date = page
        .properties
        .date
        .date
        .as_ref()
        .map(|d| d.start.clone())
        .unwrap_or_default();

    let hours = page.properties.hours_worked.number.unwrap_or(0.0);
    let paid = page.properties.paid.checkbox;

    // Get amount from Notion formula field
    let amount_usd = match &page.properties.amount_earned.formula {
        FormulaValue::String { string: Some(s) } => {
            // Parse "$45.00" format
            s.trim_start_matches('$')
                .replace(',', "")
                .parse::<f64>()
                .unwrap_or(0.0)
        }
        FormulaValue::Number { number: Some(n) } => *n,
        _ => 0.0,
    };

    if date.is_empty() {
        return None;
    }

    Some(HoursLogEntry {
        page_id: page.id.clone(),
        description,
        date,
        hours,
        amount_usd,
        paid,
    })
}

/// Convert hours log entries to expenses
pub fn hours_to_expenses(entries: &[HoursLogEntry]) -> Vec<Expense> {
    entries
        .iter()
        .map(|entry| Expense {
            id: None,
            date: entry.date.clone(),
            vendor: "Contractor".to_string(),
            category: ExpenseCategory::Contractor,
            description: format!("{} ({:.1}h)", entry.description, entry.hours),
            amount_usd: entry.amount_usd,
            paid_with: if entry.paid { "Paid" } else { "Unpaid" }.to_string(),
            invoice_id: Some(entry.page_id.clone()),
        })
        .collect()
}

/// Get summary statistics for hours log
pub fn hours_summary(entries: &[HoursLogEntry]) -> HoursSummary {
    let total_hours: f64 = entries.iter().map(|e| e.hours).sum();
    let total_amount: f64 = entries.iter().map(|e| e.amount_usd).sum();
    let unpaid_hours: f64 = entries.iter().filter(|e| !e.paid).map(|e| e.hours).sum();
    let unpaid_amount: f64 = entries
        .iter()
        .filter(|e| !e.paid)
        .map(|e| e.amount_usd)
        .sum();

    HoursSummary {
        total_entries: entries.len(),
        total_hours,
        total_amount,
        unpaid_hours,
        unpaid_amount,
    }
}

#[derive(Debug)]
pub struct HoursSummary {
    pub total_entries: usize,
    pub total_hours: f64,
    pub total_amount: f64,
    pub unpaid_hours: f64,
    pub unpaid_amount: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hours_to_expenses() {
        let entries = vec![HoursLogEntry {
            page_id: "abc123".to_string(),
            description: "Setup work".to_string(),
            date: "2026-01-15".to_string(),
            hours: 2.5,
            amount_usd: 37.50,
            paid: false,
        }];

        let expenses = hours_to_expenses(&entries);
        assert_eq!(expenses.len(), 1);
        assert_eq!(expenses[0].category, ExpenseCategory::Contractor);
        assert_eq!(expenses[0].amount_usd, 37.50);
        assert!(expenses[0].description.contains("2.5h"));
    }
}
