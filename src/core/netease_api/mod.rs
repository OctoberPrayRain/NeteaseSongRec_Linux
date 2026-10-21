use async_channel::{Receiver, Sender};
use base64::Engine;
use reqwest::Client;
use reqwest::header::COOKIE;
use serde_json::Value;

use crate::core::fingerprinting::netease::NeteaseFingerprint;

const MATCH_ENDPOINT: &str = "https://interface.music.163.com/api/music/audio/match";

#[derive(Debug, Clone)]
pub struct NeteaseSongMatch {
    pub id: Option<u64>,
    pub name: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub start_time_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct NeteaseMatchResult {
    pub code: Option<i64>,
    pub message: Option<String>,
    pub no_match_reason: Option<i64>,
    pub matches: Vec<NeteaseSongMatch>,
}

/// Sends a NetEase fingerprint using the same form fields as the Chrome plugin.
pub struct NeteaseMatchClient {
    client: Client,
    cookie: Option<String>,
}

impl NeteaseMatchClient {
    pub fn from_environment() -> Result<Self, String> {
        let cookie = std::env::var("NETEASE_COOKIE")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let client = Client::builder()
            // NetEase may close idle keep-alive sockets; avoid reusing a stale one.
            .pool_max_idle_per_host(0)
            .build()
            .map_err(|error| format!("Could not create NetEase HTTP client: {error}"))?;
        Ok(Self { client, cookie })
    }

    pub async fn match_fingerprint(
        &self,
        fingerprint: &NeteaseFingerprint,
    ) -> Result<NeteaseMatchResult, String> {
        let rawdata = base64::engine::general_purpose::STANDARD.encode(&fingerprint.rawdata);
        let duration = fingerprint.duration.to_string();
        let times = fingerprint.times.to_string();
        let mut request = self.client.post(MATCH_ENDPOINT).form(&[
            ("sessionId", fingerprint.session_id.as_str()),
            ("algorithmCode", "shazam_v2"),
            ("duration", duration.as_str()),
            ("rawdata", rawdata.as_str()),
            ("times", times.as_str()),
            ("decrypt", "1"),
        ]);
        if let Some(cookie) = &self.cookie {
            request = request.header(COOKIE, cookie);
        }

        let response = request
            .send()
            .await
            .map_err(|error| format!("NetEase match request failed: {error}"))?;
        let response = response
            .error_for_status()
            .map_err(|error| format!("NetEase match API returned an HTTP error: {error}"))?;
        let body = response
            .json::<Value>()
            .await
            .map_err(|error| format!("Could not decode NetEase match response: {error}"))?;
        parse_match_response(&body)
    }
}

fn parse_match_response(response: &Value) -> Result<NeteaseMatchResult, String> {
    let data = response
        .get("data")
        .ok_or_else(|| "NetEase match response has no data field".to_string())?;
    let matches = data
        .get("result")
        .and_then(Value::as_array)
        .map(|candidates| {
            candidates
                .iter()
                .filter_map(|candidate| {
                    let song = candidate.get("song").unwrap_or(candidate);
                    let name = song.get("name")?.as_str()?.to_string();
                    let artists = song
                        .get("artists")
                        .and_then(Value::as_array)
                        .map(|artists| {
                            artists
                                .iter()
                                .filter_map(|artist| {
                                    artist.get("name")?.as_str().map(str::to_string)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(NeteaseSongMatch {
                        id: song.get("id").and_then(Value::as_u64),
                        name,
                        artists,
                        album: song
                            .get("album")
                            .and_then(|album| album.get("name"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        start_time_ms: candidate.get("startTime").and_then(Value::as_u64),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(NeteaseMatchResult {
        code: response.get("code").and_then(Value::as_i64),
        message: response
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string),
        no_match_reason: data.get("noMatchReason").and_then(Value::as_i64),
        matches,
    })
}

/// Sends generated fingerprints to NetEase and forwards each parsed result.
pub fn match_thread(
    fingerprint_rx: Receiver<(crate::core::thread_messages::AudioSegment, Result<NeteaseFingerprint, String>)>,
    result_tx: Sender<(u64, Result<NeteaseMatchResult, String>)>,
) {
    let client = match NeteaseMatchClient::from_environment() {
        Ok(client) => client,
        Err(error) => {
            while let Ok((segment, _)) = fingerprint_rx.recv_blocking() {
                if !segment.session.cancellation.is_cancelled() {
                    let _ = result_tx.send_blocking((segment.session.id, Err(error.clone())));
                }
            }
            return;
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = result_tx.send_blocking((0, Err(format!("Could not start API runtime: {error}"))));
            return;
        }
    };

    while let Ok((segment, fingerprint)) = fingerprint_rx.recv_blocking() {
        let session_id = segment.session.id;
        if segment.session.cancellation.is_cancelled() { continue; }
        let token = segment.session.cancellation.clone();
        let result = match fingerprint {
            Ok(fingerprint) => runtime.block_on(async {
                tokio::select! {
                    _ = token.cancelled() => None,
                    response = client.match_fingerprint(&fingerprint) => Some(response),
                }
            }),
            Err(error) => Some(Err(error)),
        };
        if let Some(result) = result {
            if !token.is_cancelled() && result_tx.send_blocking((session_id, result)).is_err() { break; }
        }
    }
}
