use leptos::prelude::*;
use shared::CONFIG;

use crate::api::{
    JitoMevHistory, NetworkComparison, SfdpStatus, StakewizValidator, format_lamports_to_sol, format_percent,
    format_stake, get_jito_mev_history, get_network_comparison, get_sfdp_status, get_validator_data,
};

/// All data needed for metrics display
#[derive(Clone)]
struct MetricsData {
    validator: StakewizValidator,
    mev_history: Option<JitoMevHistory>,
    network_comp: Option<NetworkComparison>,
    sfdp_status: Option<SfdpStatus>,
}

/// Fetch all metrics data
async fn fetch_all_metrics() -> Option<MetricsData> {
    // Fetch Stakewiz data first (required)
    let validator = get_validator_data().await?;

    // Fetch additional data - each can fail independently
    let (mev_result, sfdp_result, network_result) = futures::join!(
        get_jito_mev_history(5),
        get_sfdp_status(),
        get_network_comparison(validator.skip_rate, validator.vote_success, validator.activated_stake),
    );

    Some(MetricsData {
        validator,
        mev_history: mev_result,
        network_comp: network_result,
        sfdp_status: sfdp_result,
    })
}

/// Metrics component - displays validator stats
/// Ported from Metrics.tsx
#[component]
pub fn Metrics() -> impl IntoView {
    let metrics = LocalResource::new(fetch_all_metrics);

    view! {
        <Suspense fallback=move || view! {
            <div class="text-[var(--ink-light)]">"Loading metrics..."</div>
        }>
            {move || {
                metrics.get().map(|result| {
                    // Dereference SendWrapper to access inner Option
                    match &*result {
                        Some(data) => view! { <MetricsContent data=data.clone() /> }.into_any(),
                        None => view! {
                            <div class="text-[var(--ink-light)]">
                                "Live metrics unavailable. See "
                                <a href=CONFIG.links.stakewiz>"Stakewiz"</a>
                                " for current data."
                            </div>
                        }.into_any(),
                    }
                })
            }}
        </Suspense>
    }
}

#[component]
fn MetricsContent(data: MetricsData) -> impl IntoView {
    let v = data.validator.clone();
    let status_icon = if v.delinquent { "✗" } else { "✓" };
    let status_text = if v.delinquent { "DELINQUENT" } else { "ACTIVE" };

    let version = v.version.clone();
    let ip_city = v.ip_city.clone().unwrap_or_default();
    let ip_country = v.ip_country.clone().unwrap_or_default();
    let ip_org = v.ip_org.clone().unwrap_or_default();
    let asn = v.asn.clone().unwrap_or_default();
    let client = if v.is_jito { "jito-solana" } else { "agave" };

    let has_sfdp = data.sfdp_status.as_ref().map(|s| s.is_participant).unwrap_or(false);
    let is_jito = v.is_jito;
    let mev_history = data.mev_history.clone();
    let network_comp = data.network_comp.clone();

    view! {
        <div class="space-y-4">
            // Status Line
            <div>
                <strong>{status_icon} " " {status_text}</strong>
                " · v" {version}
                " · rank #" {v.rank}
                " · wiz " {format!("{:.0}", v.wiz_score)} "/100"
            </div>

            // Badges / Trust Indicators
            <div class="flex flex-wrap gap-2">
                {has_sfdp.then(|| view! {
                    <span class="inline-block px-2 py-0.5 text-sm border border-[var(--rule)] bg-[var(--paper)]">
                        "SFDP ✓"
                    </span>
                })}
                {is_jito.then(|| view! {
                    <span class="inline-block px-2 py-0.5 text-sm border border-[var(--rule)] bg-[var(--paper)]">
                        "JITO-BAM ✓"
                    </span>
                })}
            </div>

            // Stake & Commission
            <div>
                <strong>"STAKE"</strong> " " {format_stake(v.activated_stake)} " SOL"
                <br />
                <strong>"COMMISSION"</strong> " " {v.commission} "%"
                <br />
                <strong>"JITO MEV FEE"</strong> " " {v.jito_commission_bps / 100} "%"
            </div>

            // Performance
            <div>
                <strong>"VOTE SUCCESS"</strong> " " {format_percent(v.vote_success, 2)}
                <br />
                <strong>"SKIP RATE"</strong> " " {format_percent(v.skip_rate, 2)}
                <br />
                <strong>"UPTIME"</strong> " " {format_percent(v.uptime, 1)}
                <br />
                <strong>"CREDIT RATIO"</strong> " " {format_percent(v.credit_ratio, 2)}
            </div>

            // Network Comparison
            {network_comp.map(|nc| view! {
                <div class="text-[var(--ink-light)]">
                    <strong class="text-[var(--ink)]">"VS NETWORK"</strong>
                    " (" {nc.total_validators} " validators)"
                    <br />
                    "Skip rate: top " {nc.skip_rate_percentile} "%"
                    " · Stake: top " {nc.stake_percentile} "%"
                </div>
            })}

            // APY
            <div>
                <strong>"APY (staking)"</strong> " " {format_percent(v.staking_apy, 2)}
                <br />
                <strong>"APY (jito mev)"</strong> " " {format_percent(v.jito_apy, 2)}
                <br />
                <strong>"APY (total)"</strong> " " {format_percent(v.total_apy, 2)}
            </div>

            // MEV Rewards History
            <div>
                <strong>"MEV REWARDS"</strong>
                {match mev_history {
                    Some(mh) if !mh.epochs.is_empty() => {
                        let epochs = mh.epochs.clone();
                        let count = epochs.len();
                        view! {
                            " (last " {count} " epochs)"
                            <div class="mt-1 text-sm font-mono">
                                {epochs.into_iter().rev().take(5).collect::<Vec<_>>().into_iter().rev().map(|e| {
                                    let epoch = e.epoch;
                                    let rewards = format_lamports_to_sol(e.get_mev_rewards(), 4);
                                    view! {
                                        <div class="text-[var(--ink-light)]">
                                            "E" {epoch} ": " {rewards} " SOL"
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    },
                    _ => view! {
                        <div class="mt-1 text-sm text-[var(--ink-light)]">
                            "See "
                            <a href=CONFIG.links.jito>"Jito"</a>
                            " for MEV reward details"
                        </div>
                    }.into_any(),
                }}
            </div>

            // Infrastructure
            <div class="text-[var(--ink-light)]">
                {ip_city} ", " {ip_country} " · " {ip_org}
                <br />
                {client} " · ASN " {asn} " · epoch " {v.epoch}
            </div>
        </div>
    }
}
