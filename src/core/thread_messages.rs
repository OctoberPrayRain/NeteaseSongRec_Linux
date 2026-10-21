#[derive(Debug, Clone)]
pub struct DeviceListItem {
    pub inner_name: String,
    pub display_name: String,
    pub is_monitor: bool,
    pub is_default_monitor: bool,
}

#[derive(Debug)]
pub enum AudioCaptureCommand {
    Start(String, RecognitionSession),
    SetDevice(String),
    RefreshDevices,
    Stop(u64),
    ProcessingDone(u64),
    Shutdown,
}

#[derive(Debug)]
pub enum AudioCaptureEvent {
    DevicesList(Vec<DeviceListItem>),
    AudioError(String),
    RecordingStarted(u64),
    NoAudioDetected(u64),
}
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct RecognitionSession {
    pub id: u64,
    pub cancellation: CancellationToken,
}

#[derive(Debug)]
pub struct AudioSegment {
    pub session: RecognitionSession,
    pub samples: Vec<f32>,
}
