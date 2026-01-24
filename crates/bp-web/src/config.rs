/// Static validator configuration
pub struct Config {
    pub name: &'static str,
    pub tagline: &'static str,

    // Pubkeys - everything else is fetched from APIs using these
    pub identity: &'static str,
    pub vote_account: &'static str,
    pub withdraw_authority: &'static str,

    pub contact: Contact,
    pub links: Links,
    pub changelog: &'static [ChangelogEntry],
}

pub struct Contact {
    pub twitter: &'static str,
}

pub struct Links {
    pub validators_app: &'static str,
    pub stakewiz: &'static str,
    pub solscan: &'static str,
    pub sfdp: &'static str,
    pub jito: &'static str,
    pub ibrl: &'static str,
}

pub struct ChangelogEntry {
    pub date: &'static str,
    pub event: &'static str,
}

pub static CONFIG: Config = Config {
    name: "Block Parliament",
    tagline: "Anza core dev validator",

    identity: "mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e",
    vote_account: "4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg",
    withdraw_authority: "AN58nFDFdehKbP7d3KALhnCJAsWNE7cWpCR6dLVAj9xm",

    contact: Contact { twitter: "grod220" },

    links: Links {
        validators_app: "https://www.validators.app/validators/mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e?locale=en&network=mainnet",
        stakewiz: "https://stakewiz.com/validator/4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg",
        solscan: "https://solscan.io/account/4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg",
        sfdp: "https://solana.org/sfdp-validators/mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e",
        jito: "https://www.jito.network/stakenet/steward/4PL2ZFoZJHgkbZ54US4qNC58X69Fa1FKtY4CaVKeuQPg/",
        ibrl: "https://ibrl.wtf/validator/mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e/",
    },

    // Changelog entries - newest first
    changelog: &[
        ChangelogEntry {
            date: "2026-01-22",
            event: "Added security policy page",
        },
        ChangelogEntry {
            date: "2026-01-13",
            event: "Site launch",
        },
        ChangelogEntry {
            date: "2026-01-10",
            event: "Upgraded to jito-BAM v3.0.14",
        },
        ChangelogEntry {
            date: "2026-01-01",
            event: "First MEV rewards earned (epoch 904)",
        },
        ChangelogEntry {
            date: "2025-12-30",
            event: "Received Solana Foundation delegation (epoch 903)",
        },
        ChangelogEntry {
            date: "2025-12-23",
            event: "Upgraded to jito v3.0.13",
        },
        ChangelogEntry {
            date: "2025-12-22",
            event: "First epoch with stake (epoch 899)",
        },
        ChangelogEntry {
            date: "2025-12-16",
            event: "Accepted into Solana Foundation Delegation Program (epoch 896)",
        },
        ChangelogEntry {
            date: "2025-11-19",
            event: "Bootstrapped validator with Agave client",
        },
    ],
};
