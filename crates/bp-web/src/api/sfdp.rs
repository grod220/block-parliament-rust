use gloo_net::http::Request;
use serde::Deserialize;
use shared::CONFIG;

const SFDP_API: &str = "https://api.solana.org/api/community/v1/sfdp_participants";

/// SFDP (Solana Foundation Delegation Program) status
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SfdpStatus {
    pub is_participant: bool,
    pub program_name: Option<String>,
    pub status: Option<String>,
    pub onboarding_date: Option<String>,
}

#[derive(Deserialize)]
struct SfdpParticipant {
    identity: Option<String>,
    vote_account: Option<String>,
    program_name: Option<String>,
    status: Option<String>,
    onboarding_date: Option<String>,
}

/// Check SFDP participation status
pub async fn get_sfdp_status() -> Option<SfdpStatus> {
    let response = Request::get(SFDP_API)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("SFDP API error: {}", response.status()).into());
        return None;
    }

    let participants: Vec<SfdpParticipant> = response.json().await.ok()?;

    // Find our entry
    let our_entry = participants.into_iter().find(|p| {
        p.identity.as_deref() == Some(CONFIG.identity) || p.vote_account.as_deref() == Some(CONFIG.vote_account)
    })?;

    Some(SfdpStatus {
        is_participant: true,
        program_name: our_entry.program_name,
        status: our_entry.status,
        onboarding_date: our_entry.onboarding_date,
    })
}
