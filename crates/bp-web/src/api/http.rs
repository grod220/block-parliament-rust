//! HTTP client abstraction for SSR and client-side
//! Uses reqwest on server, gloo-net on client

use serde::de::DeserializeOwned;

#[cfg(feature = "ssr")]
pub async fn get_json<T: DeserializeOwned>(url: &str) -> Option<T> {
    let response = reqwest::Client::new()
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        eprintln!("HTTP error: {}", response.status());
        return None;
    }

    response.json().await.ok()
}

#[cfg(feature = "ssr")]
pub async fn get_text(url: &str) -> Option<String> {
    let response = reqwest::Client::new()
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        eprintln!("HTTP error: {}", response.status());
        return None;
    }

    response.text().await.ok()
}

#[cfg(feature = "ssr")]
pub async fn post_json<T: DeserializeOwned>(url: &str, body: &str) -> Option<T> {
    let response = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        eprintln!("HTTP error: {}", response.status());
        return None;
    }

    response.json().await.ok()
}

#[cfg(feature = "hydrate")]
pub async fn get_json<T: DeserializeOwned>(url: &str) -> Option<T> {
    let response = gloo_net::http::Request::get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("HTTP error: {}", response.status()).into());
        return None;
    }

    response.json().await.ok()
}

#[cfg(feature = "hydrate")]
pub async fn get_text(url: &str) -> Option<String> {
    let response = gloo_net::http::Request::get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("HTTP error: {}", response.status()).into());
        return None;
    }

    response.text().await.ok()
}

#[cfg(feature = "hydrate")]
pub async fn post_json<T: DeserializeOwned>(url: &str, body: &str) -> Option<T> {
    let response = gloo_net::http::Request::post(url)
        .header("Content-Type", "application/json")
        .body(body)
        .ok()?
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("HTTP error: {}", response.status()).into());
        return None;
    }

    response.json().await.ok()
}

// Fallback for when neither feature is enabled (cargo check)
#[cfg(not(any(feature = "ssr", feature = "hydrate")))]
pub async fn get_json<T: DeserializeOwned>(_url: &str) -> Option<T> {
    None
}

#[cfg(not(any(feature = "ssr", feature = "hydrate")))]
pub async fn get_text(_url: &str) -> Option<String> {
    None
}

#[cfg(not(any(feature = "ssr", feature = "hydrate")))]
pub async fn post_json<T: DeserializeOwned>(_url: &str, _body: &str) -> Option<T> {
    None
}
