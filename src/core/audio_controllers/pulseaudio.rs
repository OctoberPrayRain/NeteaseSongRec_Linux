// Based on SongRec (marin-m and contributors), GPL-3.0-or-later.
// Modified for NeteaseSongRec_Linux on 2026-10-21; see THIRD_PARTY_NOTICES.md.
use cpal::platform::{Device, Host};
use cpal::traits::HostTrait;

use pulsectl::controllers::{AppControl, DeviceControl, SourceController};

use crate::core::audio_controllers::audio_backend::AudioBackend;
use crate::core::thread_messages::DeviceListItem;

use log::{debug, error};

pub struct PulseBackend {
    handler: SourceController,
}

impl PulseBackend {
    pub fn try_init() -> Option<Self> {
        match SourceController::create() {
            Ok(mut handler) => {
                if let Err(error) = handler.get_server_info() {
                    error!("Could not get PulseAudio server info: {:?}", error);
                } else if let Err(error) = handler.list_devices() {
                    error!("Could not list PulseAudio devices: {:?}", error);
                } else {
                    return Some(Self { handler });
                }
            }
            Err(error) => {
                error!("Could not initialize PulseAudio backend: {:?}", error);
            }
        }
        None
    }

    fn get_app_idx(&mut self) -> Option<u32> {
        // Get SongRec's source-output index

        let applications = self.handler.list_applications().unwrap();

        let criteria: [String; 5] = [
            format!("process.id = \"{}\"", std::process::id()),
            format!("cpal-capture-{}", std::process::id()),
            "alsa plug-in [songrec]".to_string(),
            "songrec".to_string(),
            format!("{}", std::process::id()),
        ];

        for criterion in criteria {
            for app in &applications {
                if app
                    .proplist
                    .to_string()
                    .unwrap()
                    .to_lowercase()
                    .contains(&criterion)
                {
                    return Some(app.index);
                }
            }
        }
        None
    }
}

impl AudioBackend for PulseBackend {
    fn list_devices(&mut self, _host: &Host) -> Vec<DeviceListItem> {
        let mut device_names: Vec<DeviceListItem> = vec![];
        let mut monitor_device_names: Vec<DeviceListItem> = vec![];
        let mut default_monitor_device: Option<DeviceListItem> = None;

        match self.handler.get_server_info() {
            Ok(info) => match self.handler.list_devices() {
                Ok(devices) => {
                    for dev in devices {
                        if let Some(desc) = &dev.description
                            && let Some(name) = &dev.name
                        {
                            let is_monitor = dev.monitor.is_some();
                            let default_monitor_name = info
                                .default_sink_name
                                .as_deref()
                                .map(|sink_name| format!("{sink_name}.monitor"));
                            let item = DeviceListItem {
                                inner_name: name.to_string(),
                                display_name: desc.to_string(),
                                is_monitor,
                                is_default_monitor: is_monitor
                                    && (dev.monitor_name.as_deref()
                                        == info.default_sink_name.as_deref()
                                        || default_monitor_name.as_deref() == Some(name)),
                            };
                            if item.is_default_monitor {
                                default_monitor_device = Some(item);
                            } else if dev.name == info.default_source_name {
                                device_names.push(item);
                            } else {
                                monitor_device_names.push(item);
                            }
                        }
                    }
                }
                Err(error) => {
                    error!("Could not list PulseAudio devices: {:?}", error);
                }
            },
            Err(error) => {
                error!("Could not get PulseAudio server info: {:?}", error);
            }
        }

        if let Some(default_monitor_device) = default_monitor_device {
            device_names.insert(0, default_monitor_device);
        }
        device_names.extend(monitor_device_names);
        device_names
    }

    fn set_device(&mut self, host: &Host, inner_name: &str) -> Device {
        match self.handler.list_devices() {
            Ok(devices) => {
                if let Some(app_idx) = self.get_app_idx() {
                    for dev in devices {
                        debug!(
                            "Comparing libpulse device names: {:?} / {:?}",
                            dev.name, inner_name
                        );
                        if Some(inner_name) == dev.name.as_deref() {
                            debug!("Selected libpulse device found: {:?}", dev);

                            self.handler.move_app_by_name(app_idx, inner_name).unwrap();
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                error!("Could not list PulseAudio devices: {:?}", error);
            }
        }

        host.default_input_device().unwrap()
    }
}
