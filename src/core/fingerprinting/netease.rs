use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use async_channel::{Receiver, Sender};
use rodio::buffer::SamplesBuffer;
use rodio::nz;
use rodio::source::UniformSourceIterator;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::core::thread_messages::{AudioCaptureCommand, AudioSegment};

const CAPTURE_SAMPLE_RATE: usize = 48_000;
const FINGERPRINT_SAMPLE_RATE: usize = 8_000;
const DURATION_SECONDS: usize = 6;
const CAPTURE_SAMPLE_COUNT: usize = CAPTURE_SAMPLE_RATE * DURATION_SECONDS;
const FINGERPRINT_SAMPLE_COUNT: usize = FINGERPRINT_SAMPLE_RATE * DURATION_SECONDS;

#[derive(Debug)]
pub struct NeteaseFingerprint {
    pub rawdata: Vec<u8>,
    pub times: u32,
    pub duration: u32,
    pub session_id: String,
}

#[derive(Serialize)]
struct FingerprintRequest<'a> {
    pcm_f32le: &'a str,
}

#[derive(Deserialize)]
struct FingerprintResponse {
    fingerprint_b64: Option<String>,
    times: Option<u32>,
    duration: Option<u32>,
    session_id: Option<String>,
    error: Option<String>,
}

/// Keeps the NetEase Emscripten module alive in a Node.js child process.
/// Call `fingerprint` once for each six-second mono 8 kHz audio segment.
pub struct NeteaseFingerprintClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl NeteaseFingerprintClient {
    pub fn start() -> Result<Self, String> {
        let bridge = crate::core::resources::data_dir().join("netease/fingerprint_bridge.js");
        let node = std::env::var_os("NETEASE_SONGREC_NODE").unwrap_or_else(|| "node".into());
        let mut child = Command::new(node)
            .arg(bridge)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("Could not start Node.js fingerprint bridge: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Could not open Node.js bridge input".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Could not open Node.js bridge output".to_string())?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub fn fingerprint(&mut self, samples: &[f32]) -> Result<NeteaseFingerprint, String> {
        if samples.len() != FINGERPRINT_SAMPLE_COUNT {
            return Err(format!(
                "Expected {FINGERPRINT_SAMPLE_COUNT} samples (6 seconds at 8 kHz), got {}",
                samples.len()
            ));
        }

        let mut pcm_bytes = Vec::with_capacity(samples.len() * 4);
        for sample in samples {
            pcm_bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let encoded_pcm = base64::engine::general_purpose::STANDARD.encode(pcm_bytes);
        let request = serde_json::to_vec(&FingerprintRequest {
            pcm_f32le: &encoded_pcm,
        })
        .map_err(|error| format!("Could not encode fingerprint request: {error}"))?;
        self.stdin
            .write_all(&request)
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|error| format!("Could not write to Node.js fingerprint bridge: {error}"))?;

        let mut response_line = String::new();
        self.stdout
            .read_line(&mut response_line)
            .map_err(|error| format!("Could not read Node.js fingerprint response: {error}"))?;
        if response_line.is_empty() {
            return Err(format!(
                "Node.js fingerprint bridge exited with status {}",
                self.child
                    .try_wait()
                    .ok()
                    .flatten()
                    .map_or_else(|| "unknown".to_string(), |status| status.to_string())
            ));
        }
        let response: FingerprintResponse = serde_json::from_str(&response_line)
            .map_err(|error| format!("Invalid fingerprint bridge response: {error}"))?;
        if let Some(error) = response.error {
            return Err(format!("NetEase fingerprint generation failed: {error}"));
        }

        let rawdata = response
            .fingerprint_b64
            .ok_or_else(|| "Fingerprint bridge response has no fingerprint".to_string())?;
        Ok(NeteaseFingerprint {
            rawdata: base64::engine::general_purpose::STANDARD
                .decode(rawdata)
                .map_err(|error| format!("Invalid fingerprint Base64: {error}"))?,
            times: response
                .times
                .ok_or_else(|| "Fingerprint bridge response has no times value".to_string())?,
            duration: response
                .duration
                .ok_or_else(|| "Fingerprint bridge response has no duration value".to_string())?,
            session_id: response
                .session_id
                .ok_or_else(|| "Fingerprint bridge response has no session ID".to_string())?,
        })
    }
}

impl Drop for NeteaseFingerprintClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Receives captured segments, generates NetEase fingerprints, and releases
/// the capture thread's one-segment backpressure after each attempt.
pub fn fingerprinting_thread(
    audio_rx: Receiver<AudioSegment>,
    command_tx: Sender<AudioCaptureCommand>,
    result_tx: Sender<(AudioSegment, Result<NeteaseFingerprint, String>)>,
) {
    let mut client = NeteaseFingerprintClient::start();
    while let Ok(segment) = audio_rx.recv_blocking() {
        let session_id = segment.session.id;
        if segment.session.cancellation.is_cancelled() { continue; }
        let samples = segment.samples.clone();
        if samples.len() != CAPTURE_SAMPLE_COUNT {
            let error = format!(
                "Expected {CAPTURE_SAMPLE_COUNT} capture samples (6 seconds at 48 kHz), got {}",
                samples.len()
            );
            let session_id = segment.session.id;
            let result_channel_closed = result_tx.send_blocking((segment, Err(error))).is_err();
            let _ = command_tx.send_blocking(AudioCaptureCommand::ProcessingDone(session_id));
            if result_channel_closed {
                break;
            }
            continue;
        }
        let fingerprint_samples: Vec<f32> = UniformSourceIterator::new(
            SamplesBuffer::new(nz!(1), nz!(48_000), samples),
            nz!(1),
            nz!(8_000),
        )
        .take(FINGERPRINT_SAMPLE_COUNT)
        .collect();
        let result = match &mut client {
            Ok(client) => client.fingerprint(&fingerprint_samples),
            Err(error) => Err(error.clone()),
        };
        let result_channel_closed = if !segment.session.cancellation.is_cancelled() {
            result_tx.send_blocking((segment, result)).is_err()
        } else {
            false
        };
        let _ = command_tx.send_blocking(AudioCaptureCommand::ProcessingDone(session_id));
        if result_channel_closed {
            break;
        }
    }
}
