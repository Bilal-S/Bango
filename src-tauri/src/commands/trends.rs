use serde::Serialize;

/// Outcome of a preflight HTTP probe against a Google Trends embed URL.
///
/// The frontend uses this to decide whether to render the embed iframe at all
/// (avoiding a guaranteed 429) or to surface a fallback UI immediately.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendsStatus {
    pub ok: bool,
    pub status_code: u16,
    /// One of: "ok" | "429" | "http" | "network" | "timeout"
    pub reason: String,
}

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/124.0.0.0 Safari/537.36";

/// Probes a Google Trends embed URL with a small ranged GET request.
///
/// We use `Range: bytes=0-0` rather than HEAD because Google's CDN responds
/// more reliably to GET, and we only need the status code — not the body.
///
/// The request is issued from the Rust side (via `reqwest`) which is *not*
/// subject to Tauri's HTTP capability system, so no capability grant is
/// required. This keeps the probe invisible to Google's iframe anti-bot logic
/// while still exercising the same rate-limit counters.
#[tauri::command]
pub async fn check_trends_url(url: String) -> Result<TrendsStatus, String> {
    if url.trim().is_empty() {
        return Err("URL must not be empty".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .connect_timeout(std::time::Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let response = client
        .get(&url)
        .header("Range", "bytes=0-0")
        .header("Accept", "text/html,*/*")
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status = resp.status().as_u16();
            // Normalize: 3xx are fine (redirects disabled, but defensive).
            if status == 429 {
                Ok(TrendsStatus { ok: false, status_code: status, reason: "429".into() })
            } else if status >= 400 {
                Ok(TrendsStatus { ok: false, status_code: status, reason: "http".into() })
            } else {
                Ok(TrendsStatus { ok: true, status_code: status, reason: "ok".into() })
            }
        }
        Err(e) => {
            if e.is_timeout() {
                Ok(TrendsStatus { ok: false, status_code: 0, reason: "timeout".into() })
            } else {
                Ok(TrendsStatus { ok: false, status_code: 0, reason: "network".into() })
            }
        }
    }
}
