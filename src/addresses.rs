//! Known address labels for transaction categorization
//!
//! This module contains mappings of known Solana addresses to human-readable labels.
//! These are used to automatically categorize transactions.

use serde::Serialize;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

/// Address category for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(dead_code)]
pub enum AddressCategory {
    /// Solana Foundation (SFDP reimbursements, delegations)
    SolanaFoundation,
    /// Jito tip distribution / MEV-related
    JitoMev,
    /// Known exchange (Coinbase, Binance, etc.)
    Exchange,
    /// Block Parliament validator account (vote, identity)
    ValidatorSelf,
    /// Personal wallet (for seeding detection)
    PersonalWallet,
    /// System program
    SystemProgram,
    /// Stake program
    StakeProgram,
    /// Vote program
    VoteProgram,
    /// Unknown address
    Unknown,
}

/// Label information for an address
#[derive(Debug, Clone)]
pub struct AddressLabel {
    pub category: AddressCategory,
    pub name: String,
    #[allow(dead_code)]
    pub description: Option<String>,
}

/// Static map of known addresses
/// Sources: Solscan labels, Solana documentation, Jito documentation
pub static KNOWN_ADDRESSES: LazyLock<HashMap<Pubkey, AddressLabel>> = LazyLock::new(|| {
    let mut map = HashMap::new();

    // =========================================================================
    // Solana Foundation Addresses
    // These are used for SFDP reimbursements and delegations
    // =========================================================================

    // Solana Foundation main addresses (from Solscan labels)
    add_address(
        &mut map,
        "mpa4abUkjQoAvPzREkh5Mo75hZhPFQ2FSH6w7dWKuQ5",
        AddressCategory::SolanaFoundation,
        "Solana Foundation",
        Some("Main SF wallet for SFDP operations"),
    );

    add_address(
        &mut map,
        "7K8DVxtNJGnMtUY1CQJT5jcs8sFGSZTDiG7kowvFpECh",
        AddressCategory::SolanaFoundation,
        "Solana Foundation Stake Authority",
        Some("SF stake authority for delegations"),
    );

    // Common SF delegation/reimbursement wallets
    add_address(
        &mut map,
        "DRpbCBMxVnDK7maPM5tGv6MvB3v1sRMC86PZ8okm21hy",
        AddressCategory::SolanaFoundation,
        "SF Delegation Program",
        Some("SFDP delegation operations"),
    );

    add_address(
        &mut map,
        "4ZJhPQAgUseCsWhKvJLTmmRRUV74fdoTpQLNfKoHtFSP",
        AddressCategory::SolanaFoundation,
        "Solana Foundation Operations",
        Some("SF operational wallet"),
    );

    // SFDP Vote Cost Reimbursement wallet (confirmed from on-chain transfers)
    add_address(
        &mut map,
        "DtZWL3BPKa5hw7yQYvaFR29PcXThpLHVU2XAAZrcLiSe",
        AddressCategory::SolanaFoundation,
        "SFDP Vote Reimbursement",
        Some("Solana Foundation vote cost reimbursements"),
    );

    // =========================================================================
    // Jito MEV Addresses (Mainnet - Frankfurt)
    // =========================================================================

    add_address(
        &mut map,
        "T1pyyaTNZsKv2WcRAB8oVnk93mLJw2XzjtVYqCsaHqt",
        AddressCategory::JitoMev,
        "Jito Tip Payment Program",
        Some("Program ID for tip payments"),
    );

    add_address(
        &mut map,
        "4R3gSG8BpU4t19KYj8CfnbtRpnT8gtk4dvTHxVRwc2r7",
        AddressCategory::JitoMev,
        "Jito Tip Distribution Program",
        Some("Program ID for tip distribution"),
    );

    add_address(
        &mut map,
        "8F4jGUmxF36vQ6yabnsxX6AQVXdKBhs8kGSUuRKSg8Xt",
        AddressCategory::JitoMev,
        "Jito Merkle Root Upload Authority",
        Some("Authority for merkle root uploads"),
    );

    // Jito tip accounts (the 8 tip payment accounts)
    add_address(
        &mut map,
        "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
        AddressCategory::JitoMev,
        "Jito Tip Account 1",
        None,
    );
    add_address(
        &mut map,
        "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
        AddressCategory::JitoMev,
        "Jito Tip Account 2",
        None,
    );
    add_address(
        &mut map,
        "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
        AddressCategory::JitoMev,
        "Jito Tip Account 3",
        None,
    );
    add_address(
        &mut map,
        "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
        AddressCategory::JitoMev,
        "Jito Tip Account 4",
        None,
    );
    add_address(
        &mut map,
        "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
        AddressCategory::JitoMev,
        "Jito Tip Account 5",
        None,
    );
    add_address(
        &mut map,
        "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
        AddressCategory::JitoMev,
        "Jito Tip Account 6",
        None,
    );
    add_address(
        &mut map,
        "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
        AddressCategory::JitoMev,
        "Jito Tip Account 7",
        None,
    );
    add_address(
        &mut map,
        "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
        AddressCategory::JitoMev,
        "Jito Tip Account 8",
        None,
    );

    // =========================================================================
    // Exchanges
    // =========================================================================

    add_address(
        &mut map,
        "H8sMJSCQxfKiFTCfDR3DUMLPwcRbM61LGFJ8N4dK3WjS",
        AddressCategory::Exchange,
        "Coinbase",
        Some("Coinbase main wallet"),
    );

    add_address(
        &mut map,
        "2AQdpHJ2JpcEgPiATUXjQxA8QmafFegfQwSLWSprPicm",
        AddressCategory::Exchange,
        "Binance",
        Some("Binance hot wallet"),
    );

    add_address(
        &mut map,
        "5tzFkiKscXHK5ZXCGbXZxdw7gTjjD1mBwuoFbhUvuAi9",
        AddressCategory::Exchange,
        "Kraken",
        Some("Kraken wallet"),
    );

    // =========================================================================
    // System Programs
    // =========================================================================

    add_address(
        &mut map,
        "11111111111111111111111111111111",
        AddressCategory::SystemProgram,
        "System Program",
        None,
    );

    add_address(
        &mut map,
        "Stake11111111111111111111111111111111111111",
        AddressCategory::StakeProgram,
        "Stake Program",
        None,
    );

    add_address(
        &mut map,
        "Vote111111111111111111111111111111111111111",
        AddressCategory::VoteProgram,
        "Vote Program",
        None,
    );

    map
});

/// Helper to add an address to the map
fn add_address(
    map: &mut HashMap<Pubkey, AddressLabel>,
    address: &str,
    category: AddressCategory,
    name: &str,
    description: Option<&str>,
) {
    if let Ok(pubkey) = Pubkey::from_str(address) {
        map.insert(
            pubkey,
            AddressLabel {
                category,
                name: name.to_string(),
                description: description.map(|s| s.to_string()),
            },
        );
    }
}

/// Get label for an address, or return "Unknown" with the address
pub fn get_label(pubkey: &Pubkey) -> AddressLabel {
    KNOWN_ADDRESSES
        .get(pubkey)
        .cloned()
        .unwrap_or_else(|| AddressLabel {
            category: AddressCategory::Unknown,
            name: format!(
                "{}...{}",
                &pubkey.to_string()[..4],
                &pubkey.to_string()[40..]
            ),
            description: None,
        })
}

/// Get category for an address
pub fn get_category(pubkey: &Pubkey) -> AddressCategory {
    KNOWN_ADDRESSES
        .get(pubkey)
        .map(|l| l.category)
        .unwrap_or(AddressCategory::Unknown)
}

/// Check if address is from Solana Foundation
pub fn is_solana_foundation(pubkey: &Pubkey) -> bool {
    matches!(get_category(pubkey), AddressCategory::SolanaFoundation)
}

/// Check if address is Jito-related
pub fn is_jito(pubkey: &Pubkey) -> bool {
    matches!(get_category(pubkey), AddressCategory::JitoMev)
}

/// Check if address is an exchange
pub fn is_exchange(pubkey: &Pubkey) -> bool {
    matches!(get_category(pubkey), AddressCategory::Exchange)
}
