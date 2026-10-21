// Based on SongRec (marin-m and contributors), GPL-3.0-or-later.
// Modified for NeteaseSongRec_Linux on 2026-10-21; see THIRD_PARTY_NOTICES.md.
use std::iter::Copied;
use std::num::NonZero;
use std::slice::Iter;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::thread_messages::{AudioCaptureCommand::*, AudioCaptureEvent, *};

use cpal::platform::Device;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{debug, warn};
use rodio::conversions::SampleTypeConverter;
use rodio::nz;

use crate::core::audio_controllers::audio_backend::get_any_backend;

const OUTPUT_SAMPLE_RATE: usize = 48_000;
const SEGMENT_DURATION_SECS: usize = 6;
const BUFFER_SIZE_SECS: usize = SEGMENT_DURATION_SECS;
const MIN_AUDIO_RMS_FOR_SIGNAL: f32 = 0.003;

struct ProcessingState<'a> {
    input_samples: Vec<f32>,
    audio_tx: &'a async_channel::Sender<AudioSegment>,
    event_tx: async_channel::Sender<AudioCaptureEvent>,
    channels: u16,
    sample_rate: u32,
    audio_segment_buffer: &'a mut [f32; OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS],
    number_unprocessed_samples: &'a mut usize,
    number_unmeasured_samples: &'a mut usize,
    audio_signal_detected: &'a mut bool,
    processing_already_ongoing: &'a AtomicBool,
    session: &'a RecognitionSession,
}

pub fn microphone_thread(
    command_rx: async_channel::Receiver<AudioCaptureCommand>,
    command_tx: async_channel::Sender<AudioCaptureCommand>,
    audio_tx: async_channel::Sender<AudioSegment>,
    event_tx: async_channel::Sender<AudioCaptureEvent>,
) {
    let _enable_pipewire_cli = true;

    #[cfg(all(target_os = "linux", feature = "pipewire"))]
    let preference_order: [bool; 2] = if _enable_pipewire_cli {
        [true, false]
    } else {
        [false, true]
    };

    #[cfg(all(target_os = "linux", not(feature = "pipewire")))]
    let preference_order: [bool; 2] = [false, true];

    #[cfg(not(target_os = "linux"))]
    let preference_order: [bool; 1] = [false];

    let event_tx_2 = event_tx.clone();
    let command_tx_2 = command_tx.clone();

    let err_fn = move |location: &'static str, error: cpal::Error, mut popup: bool| {
        if error.kind() == cpal::ErrorKind::DeviceChanged {
            command_tx_2
                .try_send(AudioCaptureCommand::RefreshDevices)
                .unwrap();
        } else if error.kind() != cpal::ErrorKind::RealtimeDenied
            && error.kind() != cpal::ErrorKind::Xrun
        {
            let error_string = format!(
                "{} {}: {:?} - {} - {}",
                "Audio error:",
                location,
                error.kind(),
                error.message().unwrap_or_default(),
                error.kind()
            );

            if error.message().unwrap_or_default() == "no target node available" {
                popup = false;
            }

            if popup {
                event_tx_2
                    .try_send(AudioCaptureEvent::AudioError(error_string))
                    .unwrap();
            } else {
                warn!("{}", error_string);
            }
        }
    };

    '_pipewire_switch: for _prefer_pipewire in preference_order {
        // Use the default host for working with audio devices.

        debug!("Trying to initialize CPAL...");
        #[cfg(target_os = "linux")]
        let host: cpal::Host = if _prefer_pipewire {
            cpal::default_host()
        } else {
            match cpal::host_from_id(cpal::HostId::Alsa) {
                Ok(host) => host,
                Err(err) => {
                    if _prefer_pipewire == preference_order[0] {
                        err_fn("ALSA driver not available", err, false);
                    } else {
                        err_fn("ALSA driver not available", err, true);
                    }
                    continue;
                }
            }
        };
        #[cfg(not(target_os = "linux"))]
        let host = cpal::default_host();
        debug!("Using audio playback backend: {:?}", host.id());
        debug!("CPAL initialized");

        let mut backend = get_any_backend();

        // Run the input stream on a separate thread.

        let mut stream: Option<cpal::Stream> = None;
        let mut active_session_id: Option<u64> = None;

        // Whether our data is already being processed in
        // other threads (pointer to a bool shared between
        // this thread and the CPAL thread, hence the Arc<AtomicBool>)

        let mut processing_already_ongoing: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

        // Publish available audio devices to the application layer.

        let device_names: Vec<DeviceListItem> = backend.list_devices(&host);

        event_tx
            .try_send(AudioCaptureEvent::DevicesList(device_names))
            .unwrap();

        // Process ingress inter-thread messages (stopping or starting
        // recording from the microphone, and knowing from which device
        // in particular)

        while let Ok(message) = command_rx.recv_blocking() {
            match message {
                Start(device_name, session) => {
                    let session_id = session.id;
                    if let Some(old_stream) = stream.take() { drop(old_stream); }
                    processing_already_ongoing = Arc::new(AtomicBool::new(false));
                    active_session_id = Some(session.id);
                    let audio_tx_2 = audio_tx.clone();
                    let event_tx_3 = event_tx.clone();
                    let event_tx_4 = event_tx.clone();

                    let err_fn_2 = err_fn.clone();
                    let err_fn_cb = move |error: cpal::Error| {
                        err_fn_2("stream error", error, true);
                    };

                    if host.default_input_device().is_none() {
                        #[cfg(target_os = "linux")]
                        if _prefer_pipewire == preference_order[0] {
                            warn!("Audio error: No input device available");
                            continue '_pipewire_switch;
                        }
                        event_tx
                            .try_send(AudioCaptureEvent::AudioError(
                                "Audio error: No input device available".to_string(),
                            ))
                            .unwrap();
                        return;
                    };

                    let device: Device = backend.set_device(&host, &device_name);

                    let config = match device.default_input_config() {
                        Ok(res) => res,
                        Err(err) => {
                            #[cfg(target_os = "linux")]
                            if _prefer_pipewire == preference_order[0] {
                                err_fn("default_input_config", err, false);
                                continue '_pipewire_switch;
                            }
                            err_fn("default_input_config", err, true);
                            return;
                        }
                    };
                    let channels = config.channels();
                    let sample_rate = config.sample_rate();

                    let mut audio_segment_buffer =
                        Box::new([0.0f32; OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS]);
                    let mut number_unprocessed_samples: usize = 0;
                    let mut number_unmeasured_samples: usize = 0;
                    let mut audio_signal_detected = false;

                    let processing_already_ongoing_2 = processing_already_ongoing.clone();

                    macro_rules! build_input_streams {
                        ($($sample_format:tt, $generic:ty);+) => {
                            match config.sample_format() {

                                // See https://github.com/RustAudio/rodio/blob/a352fb53846b47523d828b276b6d625f251aabb2/src/microphone.rs#L280
                                // See https://dev.to/sgchris/returning-iterators-from-functions-4cbh

                                cpal::SampleFormat::F32 => match device.build_input_stream(
                                    config.into(),
                                    move |data, _: &_| {
                                        write_data(ProcessingState {
                                            input_samples: data.into_iter().copied().collect(),
                                            audio_tx: &audio_tx_2,
                                            event_tx: event_tx_3.clone(),
                                            channels,
                                            sample_rate,
                                            audio_segment_buffer: &mut audio_segment_buffer,
                                            number_unprocessed_samples: &mut number_unprocessed_samples,
                                            number_unmeasured_samples: &mut number_unmeasured_samples,
                                            audio_signal_detected: &mut audio_signal_detected,
                                            processing_already_ongoing: &processing_already_ongoing_2,
                                            session: &session,
                                        })
                                    },
                                    err_fn_cb,
                                    None,
                                ) {
                                    Ok(res) => {
                                        // Re-call the function in the case the backend is PulseBackend,
                                        // because we may have appeared in the list of PulseAudio's
                                        // source outputs now
                                        let command_tx = command_tx.clone();
                                        let device_name = device_name.clone();
                                        std::thread::spawn(move || {
                                            std::thread::sleep(std::time::Duration::from_millis(50));
                                            command_tx
                                                .try_send(AudioCaptureCommand::SetDevice(
                                                    device_name
                                                ))
                                                .unwrap();
                                        });

                                        res
                                    },
                                    Err(err) => {
                                        #[cfg(all(target_os = "linux"))]
                                        if _prefer_pipewire == preference_order[0] {
                                            err_fn("build_input_stream", err, false);
                                            continue '_pipewire_switch;
                                        }
                                        err_fn("build_input_stream", err, true);
                                        return;
                                    }
                                },
                                $(
                                    cpal::SampleFormat::$sample_format => match device.build_input_stream(
                                        config.into(),
                                        move |data, _: &_| {
                                            write_data(ProcessingState {
                                                input_samples: SampleTypeConverter::<Copied<Iter<$generic>>, f32>::new(data.into_iter().copied()).collect(),
                                                audio_tx: &audio_tx_2,
                                                event_tx: event_tx_3.clone(),
                                                channels,
                                                sample_rate,
                                                audio_segment_buffer: &mut audio_segment_buffer,
                                                number_unprocessed_samples: &mut number_unprocessed_samples,
                                                number_unmeasured_samples: &mut number_unmeasured_samples,
                                                audio_signal_detected: &mut audio_signal_detected,
                                                processing_already_ongoing: &processing_already_ongoing_2,
                                                session: &session,

                                            })
                                        },
                                        err_fn_cb,
                                        None,
                                    ) {
                                        Ok(res) => {
                                            // Re-call the function in the case the backend is PulseBackend,
                                            // because we may have appeared in the list of PulseAudio's
                                            // source outputs now
                                            let command_tx = command_tx.clone();
                                            let device_name = device_name.clone();
                                            std::thread::spawn(move || {
                                            std::thread::sleep(std::time::Duration::from_millis(50));
                                                command_tx
                                                    .try_send(AudioCaptureCommand::SetDevice(
                                                        device_name
                                                    ))
                                                    .unwrap();
                                            });

                                            res
                                        },
                                        Err(err) => {
                                            #[cfg(all(target_os = "linux"))]
                                            if _prefer_pipewire == preference_order[0] {
                                                err_fn("build_input_stream", err, false);
                                                continue '_pipewire_switch;
                                            }
                                            err_fn("build_input_stream", err, true);
                                            return;
                                        }
                                    },
                                )+
                                _ => unreachable!(),
                            }
                        };
                    }

                    stream = Some(build_input_streams!(
                        F64, f64;
                        I8, i8;
                        I16, i16;
                        I24, cpal::I24;
                        I32, i32;
                        I64, i64;
                        U8, u8;
                        U16, u16;
                        U24, cpal::U24;
                        U32, u32;
                        U64, u64
                    ));

                    stream.as_ref().unwrap().play().unwrap();

                    event_tx_4
                        .try_send(AudioCaptureEvent::RecordingStarted(session_id))
                        .unwrap();
                }

                SetDevice(device_name) => {
                    backend.set_device(&host, &device_name);
                }

                RefreshDevices => {
                    debug!("Refreshing audio devices...");

                    let device_names: Vec<DeviceListItem> = backend.list_devices(&host);

                    event_tx
                        .try_send(AudioCaptureEvent::DevicesList(device_names))
                        .unwrap();
                }

                Stop(session_id) => {
                    if active_session_id == Some(session_id) {
                        if let Some(some_stream) = stream.take() {
                            drop(some_stream);
                        }
                        active_session_id = None;
                        processing_already_ongoing.store(false, Ordering::SeqCst);
                    }
                }

                ProcessingDone(session_id) => {
                    if active_session_id == Some(session_id) {
                        processing_already_ongoing.store(false, Ordering::SeqCst);
                    }
                }

                Shutdown => {
                    if let Some(capture_stream) = stream.take() {
                        drop(capture_stream);
                    }
                    return;
                }
            }
        }
        break;
    }
}

fn write_data(state: ProcessingState) {
    if state.session.cancellation.is_cancelled() {
        return;
    }
    // Resample incoming device audio to mono 48 kHz and emit each six-second segment.

    let input_buffer = rodio::buffer::SamplesBuffer::new(
        NonZero::new(state.channels).unwrap(),
        NonZero::new(state.sample_rate).unwrap(),
        state.input_samples,
    );

    let converted_file =
        rodio::source::UniformSourceIterator::new(input_buffer, nz!(1), nz!(48_000));

    let raw_pcm_samples: Vec<f32> = converted_file.collect();

    let audio_segment_buffer =
        &mut state.audio_segment_buffer[..OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS];

    // Update our buffer with data from CPAL

    if raw_pcm_samples.len() >= OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS {
        audio_segment_buffer.copy_from_slice(
            &raw_pcm_samples[raw_pcm_samples.len() - OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS..],
        );
    } else {
        let latter_data = audio_segment_buffer[raw_pcm_samples.len()..].to_vec();

        audio_segment_buffer[..OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS - raw_pcm_samples.len()]
            .copy_from_slice(&latter_data);
        audio_segment_buffer[OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS - raw_pcm_samples.len()..]
            .copy_from_slice(&raw_pcm_samples);
    }

    *state.number_unprocessed_samples += raw_pcm_samples.len();

    if *state.number_unprocessed_samples >= OUTPUT_SAMPLE_RATE * SEGMENT_DURATION_SECS
        && !state.processing_already_ongoing.load(Ordering::SeqCst)
    {
        if !*state.audio_signal_detected {
            let rms = (audio_segment_buffer
                .iter()
                .map(|sample| sample * sample)
                .sum::<f32>()
                / audio_segment_buffer.len() as f32)
                .sqrt();
            *state.audio_signal_detected = rms >= MIN_AUDIO_RMS_FOR_SIGNAL;
            if !*state.audio_signal_detected {
                state
                    .processing_already_ongoing
                    .store(true, Ordering::SeqCst);
                state
                    .event_tx
                    .try_send(AudioCaptureEvent::NoAudioDetected(state.session.id))
                    .unwrap();
            }
        }

        if *state.audio_signal_detected && !audio_segment_buffer.iter().all(|x| *x == 0.0) {
            state
                .audio_tx
                .try_send(AudioSegment {
                    session: state.session.clone(),
                    samples: audio_segment_buffer.to_vec(),
                })
                .unwrap();

            state
                .processing_already_ongoing
                .store(true, Ordering::SeqCst);
        }

        *state.number_unprocessed_samples = 0;
    }

    // Do microphone volume measurement every 24th of second (so that we can
    // update it at 24 FPS) and over the last two 100th of second (so that we
    // can be sure to measure volume for at most 100 Hz)

    *state.number_unmeasured_samples += raw_pcm_samples.len();

    if *state.number_unmeasured_samples >= OUTPUT_SAMPLE_RATE / 24 {
        let mut max_f32_amplitude = 0.0f32;

        for item in audio_segment_buffer
            .iter()
            .take(OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS)
            .skip(OUTPUT_SAMPLE_RATE * BUFFER_SIZE_SECS - OUTPUT_SAMPLE_RATE / 100 * 2)
        {
            if item.abs() > max_f32_amplitude {
                max_f32_amplitude = item.abs();
            }
        }

        state
            .event_tx
            .try_send(AudioCaptureEvent::VolumePercent(max_f32_amplitude * 100.0))
            .unwrap();

        *state.number_unmeasured_samples = 0;
    }
}
