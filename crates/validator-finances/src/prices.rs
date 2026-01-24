//! Historical price fetching from CoinGecko API

use anyhow::Result;
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

use crate::constants;
use crate::transactions::{EpochReward, SolTransfer};

/// Price cache mapping date strings to USD prices
pub type PriceCache = HashMap<String, f64>;

/// CoinGecko market chart response
#[derive(Debug, Deserialize)]
struct MarketChartResponse {
    prices: Vec<[f64; 2]>, // [timestamp_ms, price]
}

/// CoinGecko simple price response
#[derive(Debug, Deserialize)]
struct SimplePriceResponse {
    solana: Option<SolanaPrice>,
}

#[derive(Debug, Deserialize)]
struct SolanaPrice {
    usd: f64,
}

/// Fetch historical prices for all dates in rewards and transfers.
/// If `existing_prices` is provided, skip dates that are already cached.
pub async fn fetch_historical_prices(
    rewards: &[EpochReward],
    transfers: &[SolTransfer],
    api_key: &str,
) -> Result<PriceCache> {
    fetch_historical_prices_with_cache(rewards, transfers, api_key, None).await
}

/// Fetch historical prices, skipping dates already in `existing_prices`.
pub async fn fetch_historical_prices_with_cache(
    rewards: &[EpochReward],
    transfers: &[SolTransfer],
    api_key: &str,
    existing_prices: Option<&PriceCache>,
) -> Result<PriceCache> {
    let mut cache = PriceCache::new();

    // Collect all unique dates we need prices for
    let mut dates: Vec<NaiveDate> = Vec::new();

    for reward in rewards {
        if let Some(date) = &reward.date {
            if let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                // Skip if already in existing cache
                if existing_prices.is_some_and(|p| p.contains_key(date)) {
                    continue;
                }
                if !dates.contains(&d) {
                    dates.push(d);
                }
            }
        }
    }

    // Only include dates from November 2025 onwards (validator bootstrap date)
    let min_valid_date = NaiveDate::from_ymd_opt(2025, 11, 1).unwrap();

    for transfer in transfers {
        if let Some(date) = &transfer.date {
            if let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                // Skip if already in existing cache
                if existing_prices.is_some_and(|p| p.contains_key(date)) {
                    continue;
                }
                if d >= min_valid_date && !dates.contains(&d) {
                    dates.push(d);
                }
            }
        }
    }

    if dates.is_empty() {
        // No dates to fetch, get current price if not cached
        let today = Utc::now().format("%Y-%m-%d").to_string();
        if existing_prices.is_none_or(|p| !p.contains_key(&today)) {
            if let Ok(price) = fetch_current_price(api_key).await {
                cache.insert(today, price);
            }
        }
        return Ok(cache);
    }

    // Sort dates to find range
    dates.sort();
    let min_date = dates.first().unwrap();
    let max_date = dates.last().unwrap();

    // Fetch historical prices from CoinGecko
    println!("    Fetching prices from {} to {}", min_date, max_date);

    match fetch_price_range(*min_date, *max_date, api_key).await {
        Ok(prices) => {
            for (date, price) in prices {
                cache.insert(date, price);
            }
        }
        Err(e) => {
            eprintln!("    ⚠️  WARNING: Failed to fetch historical prices: {}", e);
            eprintln!(
                "    ⚠️  Using fallback price of ${:.2} for {} dates",
                constants::FALLBACK_SOL_PRICE,
                dates.len()
            );
            eprintln!("    ⚠️  Financial reports may be inaccurate!");
            // Use fallback price
            for date in &dates {
                cache.insert(
                    date.format("%Y-%m-%d").to_string(),
                    constants::FALLBACK_SOL_PRICE,
                );
            }
        }
    }

    // Ensure current price is available
    if let Ok(price) = fetch_current_price(api_key).await {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        cache.insert(today, price);
    }

    Ok(cache)
}

/// Fetch price range from CoinGecko
async fn fetch_price_range(
    from: NaiveDate,
    to: NaiveDate,
    api_key: &str,
) -> Result<Vec<(String, f64)>> {
    let client = reqwest::Client::new();

    // Convert dates to Unix timestamps
    let from_ts = from.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
    // Add one day to 'to' to ensure we get the last day
    let to_ts = (to + ChronoDuration::days(1))
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp();

    let url = format!(
        "{}{}&from={}&to={}",
        constants::COINGECKO_API_BASE,
        constants::COINGECKO_MARKET_CHART,
        from_ts,
        to_ts
    );

    // Retry with exponential backoff
    let max_retries = 3;
    let mut last_error = None;
    let mut data: Option<MarketChartResponse> = None;

    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt as u32));
            sleep(delay).await;
        }

        match client
            .get(&url)
            .header("Accept", "application/json")
            .header("x-cg-demo-api-key", api_key)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<MarketChartResponse>().await {
                        Ok(d) => {
                            data = Some(d);
                            break;
                        }
                        Err(e) => {
                            last_error = Some(anyhow::anyhow!("Parse error: {}", e));
                        }
                    }
                } else if response.status().as_u16() == 429 {
                    // Rate limited - always retry
                    last_error = Some(anyhow::anyhow!("Rate limited (429)"));
                    continue;
                } else {
                    last_error = Some(anyhow::anyhow!(
                        "CoinGecko API returned status: {}",
                        response.status()
                    ));
                }
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("Request failed: {}", e));
            }
        }
    }

    let data = data.ok_or_else(|| {
        last_error.unwrap_or_else(|| anyhow::anyhow!("Failed after {} retries", max_retries))
    })?;

    // Convert to date -> price map (use daily close price)
    let mut daily_prices: HashMap<String, f64> = HashMap::new();

    for [timestamp_ms, price] in data.prices {
        let timestamp = timestamp_ms as i64 / 1000;
        if let Some(dt) = chrono::DateTime::from_timestamp(timestamp, 0) {
            let date_str = dt.format("%Y-%m-%d").to_string();
            // Keep the latest price for each day (close price)
            daily_prices.insert(date_str, price);
        }
    }

    Ok(daily_prices.into_iter().collect())
}

/// Fetch current SOL price with retry logic
pub async fn fetch_current_price(api_key: &str) -> Result<f64> {
    let client = reqwest::Client::new();

    let url = format!(
        "{}{}",
        constants::COINGECKO_API_BASE,
        constants::COINGECKO_SIMPLE_PRICE
    );

    // Retry with exponential backoff
    let max_retries = 3;
    let mut last_error = None;

    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt as u32));
            sleep(delay).await;
        }

        match client
            .get(&url)
            .header("Accept", "application/json")
            .header("x-cg-demo-api-key", api_key)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<SimplePriceResponse>().await {
                        Ok(data) => {
                            return data
                                .solana
                                .map(|s| s.usd)
                                .ok_or_else(|| anyhow::anyhow!("No SOL price in response"));
                        }
                        Err(e) => {
                            last_error = Some(anyhow::anyhow!("Parse error: {}", e));
                        }
                    }
                } else if response.status().as_u16() == 429 {
                    last_error = Some(anyhow::anyhow!("Rate limited (429)"));
                    continue;
                } else {
                    last_error = Some(anyhow::anyhow!(
                        "API returned status: {}",
                        response.status()
                    ));
                }
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("Request failed: {}", e));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed after {} retries", max_retries)))
}

/// Get price for a specific date from cache, with fallback
pub fn get_price(cache: &PriceCache, date: &str) -> f64 {
    cache.get(date).copied().unwrap_or_else(|| {
        // Try to find closest date
        if let Ok(target) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            let mut closest_price = constants::FALLBACK_SOL_PRICE;
            let mut closest_diff = i64::MAX;

            for (d, p) in cache {
                if let Ok(cached_date) = NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                    let diff = (target - cached_date).num_days().abs();
                    if diff < closest_diff {
                        closest_diff = diff;
                        closest_price = *p;
                    }
                }
            }

            closest_price
        } else {
            constants::FALLBACK_SOL_PRICE
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_price_cache_type() {
        // Basic type check - actual API tests require credentials
        use super::PriceCache;
        let cache: PriceCache = Default::default();
        assert!(cache.is_empty());
    }
}
