use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;

use async_channel::{Receiver, Sender};

use crate::core::fingerprinting::netease::fingerprinting_thread;
use crate::core::microphone_thread::microphone_thread;
use crate::core::netease_api::{NeteaseMatchResult, match_thread};
use crate::core::thread_messages::{AudioCaptureCommand, AudioCaptureEvent, RecognitionSession};

/// Connects audio capture, NetEase fingerprint generation, and match requests.
/// The UI selects a device and consumes events/results through this handle.
pub struct RecognitionPipeline {
    command_tx: Sender<AudioCaptureCommand>,
    event_rx: Receiver<AudioCaptureEvent>,
    result_rx: Receiver<(u64, Result<NeteaseMatchResult, String>)>,
    active_session: Mutex<Option<RecognitionSession>>,
    next_session_id: AtomicU64,
}

impl RecognitionPipeline {
    pub fn new() -> Self {
        let (command_tx, command_rx) = async_channel::unbounded();
        let (audio_tx, audio_rx) = async_channel::bounded(1);
        let (event_tx, event_rx) = async_channel::unbounded();
        let (fingerprint_tx, fingerprint_rx) = async_channel::bounded(1);
        let (result_tx, result_rx) = async_channel::unbounded();

        let capture_command_tx = command_tx.clone();
        thread::spawn(move || {
            microphone_thread(command_rx, capture_command_tx, audio_tx, event_tx);
        });
        let fingerprint_command_tx = command_tx.clone();
        thread::spawn(move || {
            fingerprinting_thread(audio_rx, fingerprint_command_tx, fingerprint_tx);
        });
        thread::spawn(move || match_thread(fingerprint_rx, result_tx));

        Self {
            command_tx,
            event_rx,
            result_rx,
            active_session: Mutex::new(None),
            next_session_id: AtomicU64::new(1),
        }
    }

    pub fn refresh_devices(&self) -> Result<(), String> {
        self.send(AudioCaptureCommand::RefreshDevices)
    }

    pub fn start_capture(&self, device_name: impl Into<String>) -> Result<u64, String> {
        let session = RecognitionSession {
            id: self.next_session_id.fetch_add(1, Ordering::Relaxed),
            cancellation: tokio_util::sync::CancellationToken::new(),
        };
        let id = session.id;
        let device_name = device_name.into();
        self.send(AudioCaptureCommand::Start(device_name, session.clone()))?;
        *self.active_session.lock().map_err(|_| "Recognition session state is unavailable".to_string())? = Some(session);
        Ok(id)
    }

    pub fn set_device(&self, device_name: impl Into<String>) -> Result<(), String> {
        self.send(AudioCaptureCommand::SetDevice(device_name.into()))
    }

    pub fn stop_capture(&self) -> Result<(), String> {
        let session = self.active_session.lock().map_err(|_| "Recognition session state is unavailable".to_string())?.take();
        if let Some(session) = session {
            session.cancellation.cancel();
            self.send(AudioCaptureCommand::Stop(session.id))?;
        }
        Ok(())
    }

    pub fn events(&self) -> Receiver<AudioCaptureEvent> {
        self.event_rx.clone()
    }

    pub fn results(&self) -> Receiver<(u64, Result<NeteaseMatchResult, String>)> {
        self.result_rx.clone()
    }

    fn send(&self, command: AudioCaptureCommand) -> Result<(), String> {
        self.command_tx
            .send_blocking(command)
            .map_err(|error| format!("Recognition pipeline is not available: {error}"))
    }
}

impl Default for RecognitionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RecognitionPipeline {
    fn drop(&mut self) {
        if let Ok(active) = self.active_session.get_mut() {
            if let Some(session) = active.take() { session.cancellation.cancel(); }
        }
        let _ = self.command_tx.send_blocking(AudioCaptureCommand::Shutdown);
    }
}
