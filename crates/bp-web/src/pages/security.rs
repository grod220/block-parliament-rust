use crate::config::CONFIG;
use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::Section;

#[component]
pub fn SecurityPage() -> impl IntoView {
    let twitter_url = format!("https://x.com/{}", CONFIG.contact.twitter);
    let twitter_url2 = twitter_url.clone();
    let twitter_url3 = twitter_url.clone();

    view! {
        <main class="max-w-[80ch] mx-auto px-4 py-8 md:py-12">
            // Header
            <header class="mb-8">
                <div class="text-center">
                    <h1 class="text-xl font-bold mb-2">
                        "┌─────────────────────────────────────────┐"
                        <br />
                        "│ " {CONFIG.name} " Security Policy │"
                        <br />
                        "└─────────────────────────────────────────┘"
                    </h1>
                    <div class="text-[var(--ink-light)]">
                        "Last updated: January 2026"
                    </div>
                </div>
                <div class="mt-4 text-center">
                    <A href="/" attr:class="text-sm">"← back to home"</A>
                </div>
            </header>

            // Overview
            <Section id="overview" title="Overview">
                <p class="mb-3">
                    "Block Parliament is a Solana mainnet validator operated by "
                    <strong>"Gabe Rodriguez"</strong>
                    ", a core contributor to the Agave validator client at Anza. This document describes the security measures and operational practices in place to protect delegator stake and maintain reliable validator operations."
                </p>
                <p>
                    <strong>"Important:"</strong>
                    " When you delegate, your SOL moves to a stake account that remains under your control. Validators cannot access, move, or withdraw your delegated stake."
                </p>
            </Section>

            // Key Management
            <Section id="keys" title="Key Management">
                <div class="space-y-3">
                    <div>
                        <strong>"Withdrawal Authority Separation"</strong>
                        <p class="mt-1">
                            "The validator's withdrawal authority key ("
                            <code class="text-sm bg-[var(--rule)] px-1">{CONFIG.withdraw_authority}</code>
                            ") is stored separately from the validator identity and vote account keys. This key is kept offline and never resides on the validator server, preventing unauthorized fund access even in the event of server compromise."
                        </p>
                    </div>
                    <div>
                        <strong>"Identity Key Protection"</strong>
                        <p class="mt-1">
                            "The validator identity key is stored on the server with restricted file permissions (owned by the "
                            <code>"sol"</code>
                            " user, mode 600). Administrative access (via the "
                            <code>"ubuntu"</code>
                            " account) is separate from the validator process account ("
                            <code>"sol"</code>
                            "), which has no sudo privileges."
                        </p>
                    </div>
                    <div>
                        <strong>"Hardware Wallet Backup"</strong>
                        <p class="mt-1">
                            "Critical keys are backed up to hardware wallets stored in secure physical locations. Seed phrases are never stored digitally or transmitted over networks."
                        </p>
                    </div>
                </div>
            </Section>

            // Infrastructure
            <Section id="infrastructure" title="Infrastructure">
                <div class="space-y-3">
                    <div>
                        <strong>"Dedicated Bare-Metal Server"</strong>
                        <p class="mt-1">
                            "The validator runs on dedicated bare-metal hardware (not shared cloud VMs) hosted in a professional data center with redundant power and network connectivity. Hardware specs: AMD EPYC 24-core CPU, 377 GB RAM, NVMe storage in RAID configuration."
                        </p>
                    </div>
                    <div>
                        <strong>"Hardened Operating System"</strong>
                        <p class="mt-1">
                            "Linux installation with only essential packages plus monitoring agents (Alloy for metrics, Prometheus exporters). The validator process runs under a dedicated unprivileged user account."
                        </p>
                    </div>
                    <div>
                        <strong>"Network Security"</strong>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"• Strict firewall rules: only necessary ports exposed (Solana gossip/turbine/repair, SSH, metrics exporter)"</li>
                            <li>"• SSH access via public-key authentication only (password auth disabled)"</li>
                            <li>"• fail2ban active with aggressive settings (5 attempts → 12hr ban)"</li>
                            <li>"• DDoS mitigation provided at the data center level"</li>
                        </ul>
                    </div>
                </div>
            </Section>

            // Access Control
            <Section id="access" title="Access Control">
                <div class="space-y-3">
                    <div>
                        <strong>"Limited Personnel"</strong>
                        <p class="mt-1">
                            "Server access is limited to the operator (Gabe Rodriguez) and one contractor (Christopher Vannelli). No other third-party vendors have access to validator infrastructure."
                        </p>
                    </div>
                    <div>
                        <strong>"User Isolation"</strong>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"• Administrator account (" <code>"ubuntu"</code> ") separate from validator process account (" <code>"sol"</code> ")"</li>
                            <li>"• SSH root login disabled"</li>
                            <li>"• Validator user cannot sudo or access admin functions"</li>
                        </ul>
                    </div>
                </div>
            </Section>

            // Monitoring
            <Section id="monitoring" title="Monitoring & Alerting">
                <div class="space-y-3">
                    <div>
                        <strong>"24/7 Automated Monitoring"</strong>
                        <p class="mt-1">
                            "A dedicated watchtower service monitors validator health from an independent location (separate from the validator itself). Alerts are sent via Telegram for:"
                        </p>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"• Validator delinquency (not voting)"</li>
                            <li>"• Health check failures"</li>
                            <li>"• Vote account issues"</li>
                        </ul>
                    </div>
                    <div>
                        <strong>"Metrics Collection"</strong>
                        <p class="mt-1">
                            "System and validator metrics (CPU, memory, disk, slot lag, vote performance) are collected and stored in Grafana Cloud for trend analysis and incident investigation."
                        </p>
                    </div>
                    <div>
                        <strong>"Public Performance Data"</strong>
                        <p class="mt-1">
                            "Validator performance is publicly verifiable via "
                            <a href=CONFIG.links.stakewiz target="_blank" rel="noopener noreferrer">"Stakewiz"</a>
                            ", "
                            <a href=CONFIG.links.validators_app target="_blank" rel="noopener noreferrer">"validators.app"</a>
                            ", and on-chain data."
                        </p>
                    </div>
                </div>
            </Section>

            // Software Updates
            <Section id="updates" title="Software & Updates">
                <div class="space-y-3">
                    <div>
                        <strong>"Validator Client"</strong>
                        <p class="mt-1">
                            "Running the Jito-enhanced Agave client for MEV rewards. As an Anza core developer, the operator has deep familiarity with the client codebase and can respond quickly to issues."
                        </p>
                    </div>
                    <div>
                        <strong>"Update Process"</strong>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"• New releases tracked via Solana Tech Discord"</li>
                            <li>"• Updates tested on testnet validator before mainnet"</li>
                            <li>"• Tower file backed up before any upgrade (prevents consensus issues)"</li>
                            <li>"• OS security patches applied regularly"</li>
                        </ul>
                    </div>
                    <div>
                        <strong>"No Unproven Modifications"</strong>
                        <p class="mt-1">
                            "The validator runs standard Jito-Agave releases without custom consensus modifications that could affect network behavior."
                        </p>
                    </div>
                </div>
            </Section>

            // MEV
            <Section id="mev" title="MEV & Jito Integration">
                <div class="space-y-3">
                    <p>
                        "Block Parliament runs Jito MEV infrastructure. MEV tips are distributed automatically by Jito's on-chain programs—the validator receives its configured commission, and Jito distributes the remainder to stakers."
                    </p>
                    <div>
                        <strong>"Configuration"</strong>
                        <ul class="mt-1 list-none space-y-1">
                            <li>"• Block Engine: Frankfurt (eu-frankfurt)"</li>
                            <li>"• Tip programs: Official Jito mainnet contracts"</li>
                            <li>
                                "• Current commission rates: "
                                <a href=CONFIG.links.solscan target="_blank" rel="noopener noreferrer">"view on Solscan ↗"</a>
                            </li>
                        </ul>
                    </div>
                </div>
            </Section>

            // Incident Response
            <Section id="incidents" title="Incident Response">
                <div class="space-y-3">
                    <p>
                        "In the event of a security incident or validator issue, the operator follows these procedures:"
                    </p>
                    <ul class="list-none space-y-1">
                        <li><strong>"1. Detection"</strong> " — Automated alerts or manual observation"</li>
                        <li><strong>"2. Assessment"</strong> " — Determine scope and severity"</li>
                        <li><strong>"3. Containment"</strong> " — Isolate affected systems if needed"</li>
                        <li><strong>"4. Resolution"</strong> " — Apply fixes, restore service"</li>
                        <li><strong>"5. Review"</strong> " — Document lessons learned, improve processes"</li>
                    </ul>
                    <p class="mt-3">
                        "For issues affecting delegators, updates will be posted via "
                        <a href=twitter_url2.clone() target="_blank" rel="noopener noreferrer">
                            "@" {CONFIG.contact.twitter}
                        </a>
                        " on X."
                    </p>
                </div>
            </Section>

            // Verify
            <Section id="verify" title="Verify On-Chain">
                <p>"All claims on this page can be verified independently:"</p>
                <ul class="mt-2 list-none space-y-1">
                    <li>
                        "• "
                        <a href=CONFIG.links.solscan target="_blank" rel="noopener noreferrer">"Vote account on Solscan ↗"</a>
                        " — commission, authority keys"
                    </li>
                    <li>
                        "• "
                        <a href=CONFIG.links.stakewiz target="_blank" rel="noopener noreferrer">"Performance on Stakewiz ↗"</a>
                        " — uptime, skip rate, APY"
                    </li>
                    <li>
                        "• Withdraw authority: "
                        <code class="text-sm bg-[var(--rule)] px-1">{CONFIG.withdraw_authority}</code>
                    </li>
                </ul>
            </Section>

            // Contact
            <Section id="contact" title="Security Contact">
                <p>
                    "To report security concerns or vulnerabilities related to Block Parliament validator operations:"
                </p>
                <ul class="mt-2 list-none space-y-1">
                    <li>
                        <strong>"X/Twitter:"</strong> " "
                        <a href=twitter_url3 target="_blank" rel="noopener noreferrer">
                            "@" {CONFIG.contact.twitter}
                        </a>
                    </li>
                    <li>
                        <strong>"Telegram:"</strong> " @grod220"
                    </li>
                </ul>
            </Section>

            // Footer
            <footer class="mt-12 pt-4 border-t border-dashed border-[var(--rule)] text-center text-[var(--ink-light)] text-sm">
                <A href="/">"← back to home"</A>
            </footer>
        </main>
    }
}
