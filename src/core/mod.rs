pub mod microphone_thread;
pub mod netease_api;
pub mod recognition_pipeline;
pub mod resources;
pub mod fingerprinting {
    pub mod netease;
}
pub mod thread_messages;

pub mod audio_controllers {
    pub mod audio_backend;
    pub mod cpal;
    #[cfg(all(target_os = "linux", feature = "pulse"))]
    pub mod pulseaudio;
}
