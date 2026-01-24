use leptos::prelude::*;
use leptos_router::components::A;
use shared::CONFIG;

use crate::components::{AnimatedGradientDashBorder, Metrics, Section};

#[component]
pub fn HomePage() -> impl IntoView {
    let title = format!("{} \u{1F989}", CONFIG.name); // owl emoji

    view! {
        <main class="max-w-[80ch] mx-auto px-4 py-8 md:py-12">
            // Header with animated border
            <header class="mb-8 text-center">
                <AnimatedGradientDashBorder title=title />
                <div class="text-[var(--ink-light)] mt-2">{CONFIG.tagline}</div>
            </header>

            // Addresses - prominent at top
            <div class="mb-6 border border-dashed border-[var(--rule)] p-4">
                <div>
                    <strong>"VOTE"</strong> "     " {CONFIG.vote_account}
                </div>
                <div>
                    <strong>"IDENTITY"</strong> "  " {CONFIG.identity}
                </div>
                <div>
                    <strong>"NETWORK"</strong> "   mainnet-beta"
                </div>
            </div>

            // About
            <Section id="about" title="About">
                <p>
                    "Operated by " <strong>"Gabe Rodriguez"</strong> " ("
                    <a
                        href=format!("https://x.com/{}", CONFIG.contact.twitter)
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        "@" {CONFIG.contact.twitter}
                    </a>
                    "), a core contributor to Solana's Agave validator client and on-chain programs at Anza. A way to experience Solana from the operator's seat, not just the codebase."
                </p>
            </Section>

            // Metrics
            <Section id="metrics" title="Metrics">
                <Metrics />
            </Section>

            // Pages
            <Section id="pages" title="Pages">
                <div class="space-y-1">
                    <div>
                        <A href="/security">"security policy"</A>
                    </div>
                </div>
            </Section>

            // External Links
            <Section id="links" title="External Links">
                <div class="space-y-1">
                    <div>
                        <a href=CONFIG.links.stakewiz target="_blank" rel="noopener noreferrer">
                            "stakewiz ↗"
                        </a>
                    </div>
                    <div>
                        <a href=CONFIG.links.solscan target="_blank" rel="noopener noreferrer">
                            "solscan ↗"
                        </a>
                    </div>
                    <div>
                        <a href=CONFIG.links.validators_app target="_blank" rel="noopener noreferrer">
                            "validators.app ↗"
                        </a>
                    </div>
                    <div>
                        <a href=CONFIG.links.sfdp target="_blank" rel="noopener noreferrer">
                            "solana foundation delegation program ↗"
                        </a>
                    </div>
                    <div>
                        <a href=CONFIG.links.jito target="_blank" rel="noopener noreferrer">
                            "jito stakenet ↗"
                        </a>
                    </div>
                    <div>
                        <a href=CONFIG.links.ibrl target="_blank" rel="noopener noreferrer">
                            "ibrl ↗"
                        </a>
                    </div>
                </div>
            </Section>

            // Changelog
            <Section id="changelog" title="Changelog">
                <div>
                    {CONFIG.changelog.iter().map(|entry| view! {
                        <div>
                            <strong>{entry.date}</strong> "  " {entry.event}
                        </div>
                    }).collect_view()}
                </div>
            </Section>
        </main>
    }
}
