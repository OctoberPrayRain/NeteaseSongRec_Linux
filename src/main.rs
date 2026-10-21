mod core;

slint::include_modules!();

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use core::netease_api::NeteaseMatchResult;
use core::recognition_pipeline::RecognitionPipeline;
use core::thread_messages::AudioCaptureEvent;
use slint::{ModelRc, SharedString, Timer, TimerMode, VecModel};

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    let icon_path = core::resources::data_dir().join("ui/recognition-128.png");
    if icon_path.is_file()
        && let Ok(icon) = slint::Image::load_from_path(&icon_path)
    {
        window.set_recognition_icon(icon);
    }
    let pipeline = Rc::new(RecognitionPipeline::new());
    let events = pipeline.events();
    let results = pipeline.results();
    let device_names = Rc::new(RefCell::new(Vec::<String>::new()));
    let selected_device = Rc::new(RefCell::new(None::<String>));
    let user_selected_device = Rc::new(Cell::new(false));
    let active_session = Rc::new(Cell::new(None::<u64>));

    window.set_status_text("正在查找音频设备…".into());
    let _ = pipeline.refresh_devices();

    let names_for_selection = device_names.clone();
    let selected_for_callback = selected_device.clone();
    let user_selected_for_callback = user_selected_device.clone();
    let pipeline_for_selection = pipeline.clone();
    let weak_window_for_selection = window.as_weak();
    window.on_device_selected(move |index| {
        let Some(name) = usize::try_from(index)
            .ok()
            .and_then(|index| names_for_selection.borrow().get(index).cloned())
        else {
            return;
        };
        *selected_for_callback.borrow_mut() = Some(name.clone());
        user_selected_for_callback.set(true);
        let _ = pipeline_for_selection.set_device(name);
        if let Some(window) = weak_window_for_selection.upgrade() {
            window.set_selected_device_index(index);
            window.set_has_selection(true);
            window.set_status_text("已切换声音来源，可以开始识别。".into());
        }
    });

    let suppress_next_device_refresh_status = Rc::new(Cell::new(false));
    let active_for_toggle = active_session.clone();
    let suppress_status_for_events = suppress_next_device_refresh_status.clone();
    let pipeline_for_toggle = pipeline.clone();
    let selected_for_toggle = selected_device.clone();
    let weak_window_for_toggle = window.as_weak();
    window.on_toggle_recording(move || {
        let Some(window) = weak_window_for_toggle.upgrade() else {
            return;
        };
        if window.get_is_recording() {
            let _ = pipeline_for_toggle.stop_capture();
            active_for_toggle.set(None);
            suppress_next_device_refresh_status.set(true);
            let _ = pipeline_for_toggle.refresh_devices();
            window.set_is_recording(false);
            window.set_status_text("".into());
        } else if let Some(device) = selected_for_toggle.borrow().clone() {
            window.set_song_title("".into());
            window.set_song_detail("".into());
            window.set_status_text("保持音乐播放，正在为你识别。".into());
            match pipeline_for_toggle.start_capture(device) {
                Ok(session_id) => {
                    active_for_toggle.set(Some(session_id));
                    window.set_is_recording(true)
                },
                Err(error) => window.set_status_text(error.into()),
            }
        }
    });

    let weak_window = window.as_weak();
    let devices_for_events = device_names.clone();
    let selected_for_events = selected_device.clone();
    let user_selected_for_events = user_selected_device.clone();
    let pipeline_for_events = pipeline.clone();
    let active_for_events = active_session.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };

        while let Ok(event) = events.try_recv() {
            match event {
                AudioCaptureEvent::DevicesList(devices) => {
                    let manual_selection = user_selected_for_events
                        .get()
                        .then(|| selected_for_events.borrow().clone())
                        .flatten()
                        .and_then(|name| {
                            devices.iter().position(|device| device.inner_name == name)
                        });
                    if user_selected_for_events.get() && manual_selection.is_none() {
                        user_selected_for_events.set(false);
                    }
                    let selected_index = manual_selection
                        .or_else(|| devices.iter().position(|device| device.is_default_monitor))
                        .or_else(|| devices.iter().position(|device| device.is_monitor));
                    let names: Vec<String> = devices
                        .iter()
                        .map(|device| device.inner_name.clone())
                        .collect();
                    let labels: Vec<SharedString> = devices
                        .iter()
                        .map(|device| device.display_name.clone().into())
                        .collect();
                    *devices_for_events.borrow_mut() = names;
                    window.set_devices(ModelRc::new(VecModel::from(labels)));
                    let suppress_status = suppress_status_for_events.replace(false);
                    if let Some(index) = selected_index {
                        let device = &devices[index];
                        *selected_for_events.borrow_mut() = Some(device.inner_name.clone());
                        let _ = pipeline_for_events.set_device(device.inner_name.clone());
                        window.set_selected_device_index(index as i32);
                        window.set_has_selection(true);
                        if !suppress_status {
                            window.set_status_text(if device.is_default_monitor {
                                "已连接系统默认声音，播放音乐后即可开始。".into()
                            } else if device.is_monitor {
                                "已连接系统声音，播放音乐后即可开始。".into()
                            } else {
                                "请选择正在播放音乐的声音来源。".into()
                            });
                        }
                    } else {
                        *selected_for_events.borrow_mut() = None;
                        window.set_selected_device_index(-1);
                        window.set_has_selection(false);
                        if devices.is_empty() || !suppress_status {
                            window.set_status_text(if devices.is_empty() {
                                "没有找到可用音频设备。".into()
                            } else {
                                "未找到系统声音，请手动选择来源。".into()
                            });
                        }
                    }
                }
                AudioCaptureEvent::AudioError(error) => {
                    let _ = pipeline_for_events.stop_capture();
                    active_for_events.set(None);
                    window.set_is_recording(false);
                    window.set_status_text(error.into());
                }
                AudioCaptureEvent::RecordingStarted(session_id) if active_for_events.get() == Some(session_id) => {
                    window.set_is_recording(true);
                    window.set_status_text("保持音乐播放，正在为你识别。".into());
                }
                AudioCaptureEvent::NoAudioDetected(session_id) if active_for_events.get() == Some(session_id) => {
                    let _ = pipeline_for_events.stop_capture();
                    active_for_events.set(None);
                    window.set_is_recording(false);
                    window.set_song_title("".into());
                    window.set_song_detail("".into());
                    window.set_status_text("没有听到音乐，请确认正在播放或切换声音来源。".into());
                }
                AudioCaptureEvent::RecordingStarted(_) | AudioCaptureEvent::NoAudioDetected(_) => {}
                AudioCaptureEvent::VolumePercent(_) => {}
            }
        }

        while let Ok(result) = results.try_recv() {
            let (session_id, result) = result;
            if active_for_events.get() != Some(session_id) { continue; }
            match result {
                Ok(result) => {
                    let matched_song = !result.matches.is_empty();
                    show_result(&window, result);
                    if matched_song {
                        let _ = pipeline_for_events.stop_capture();
                        active_for_events.set(None);
                        window.set_is_recording(false);
                    }
                }
                Err(error) => {
                    let _ = pipeline_for_events.stop_capture();
                    active_for_events.set(None);
                    window.set_is_recording(false);
                    window.set_song_title("".into());
                    window.set_song_detail("".into());
                    window.set_status_text(error.into());
                }
            }
        }
    });

    window.run()
}

fn show_result(window: &MainWindow, result: NeteaseMatchResult) {
    if let Some(song) = result.matches.first() {
        window.set_song_title(song.name.clone().into());
        let mut detail = song.artists.join(" / ");
        if let Some(album) = &song.album {
            if !detail.is_empty() {
                detail.push_str(" · ");
            }
            detail.push_str(&album);
        }
        window.set_song_detail(detail.into());
        window.set_status_text("识别完成。点击“开始识别”进行下一次识别。".into());
    } else {
        window.set_song_title("没有匹配结果".into());
        window.set_song_detail("请播放更清晰的音乐片段后重试。".into());
        window.set_status_text("本片段未匹配，继续聆听…".into());
    }
}
