//! The one thread that touches hidapi.
//!
//! hidapi's macOS backend binds to the run loop of the thread that first
//! uses it and breaks once that thread is gone, so uploads, device scans and
//! the live key watcher all run here, and other threads send it jobs.
//!
//! The watcher opens the keypad's input interfaces (keyboard, consumer,
//! mouse) and decodes what the keypad sends. Nothing else is read. Where the
//! OS won't share an interface (Windows keeps keyboards and mice to itself;
//! macOS asks for Input Monitoring) it is skipped and the editor falls back
//! to key presses that reach its own window.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use okf_core::driver::{DetectedDevice, Registry};
use okf_core::hidapi::{HidApi, HidDevice};
use okf_core::input::{Decoder, KeypadEvent};

type Job = Box<dyn FnOnce(Result<&mut HidApi, &str>) + Send>;

const SCAN_EVERY: Duration = Duration::from_millis(1500);

pub struct Hid {
    tx: Mutex<mpsc::Sender<Job>>,
    pub devices: Arc<Mutex<Vec<DetectedDevice>>>,
    pub watching: Arc<AtomicBool>,
}

struct Watched {
    dev: HidDevice,
    decoder: Decoder,
}

impl Hid {
    pub fn start(registry: Registry, on_event: impl Fn(KeypadEvent) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let devices = Arc::new(Mutex::new(Vec::new()));
        let watching = Arc::new(AtomicBool::new(false));
        let (k, w) = (devices.clone(), watching.clone());

        thread::Builder::new()
            .name("hid".into())
            .spawn(move || {
                let mut api: Result<HidApi, String> = HidApi::new().map_err(|e| e.to_string());
                let mut watched: HashMap<String, Watched> = HashMap::new();
                let mut refused: HashSet<String> = HashSet::new();
                let mut last_scan = Instant::now() - SCAN_EVERY;
                let mut buf = [0u8; 64];

                loop {
                    match rx.recv_timeout(Duration::from_millis(8)) {
                        Ok(job) => match &mut api {
                            Ok(api) => job(Ok(api)),
                            Err(e) => job(Err(e)),
                        },
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }

                    if last_scan.elapsed() >= SCAN_EVERY {
                        last_scan = Instant::now();
                        if api.is_err() {
                            api = HidApi::new().map_err(|e| e.to_string());
                        }
                        if let Ok(api) = &mut api {
                            let _ = api.refresh_devices();
                            *k.lock().unwrap() = registry.detect(api);
                            rescan_inputs(api, &registry, &mut watched, &mut refused);
                            w.store(!watched.is_empty(), Ordering::Relaxed);
                        }
                    }

                    // Drain whatever the keypad sent since the last pass.
                    let mut dead = Vec::new();
                    for (path, input) in watched.iter_mut() {
                        loop {
                            match input.dev.read_timeout(&mut buf, 0) {
                                Ok(0) => break,
                                Ok(n) => input
                                    .decoder
                                    .feed(&buf[..n])
                                    .into_iter()
                                    .for_each(&on_event),
                                Err(_) => {
                                    dead.push(path.clone());
                                    break;
                                }
                            }
                        }
                    }
                    for path in dead {
                        watched.remove(&path);
                    }
                }
            })
            .expect("spawn hid thread");

        Self {
            tx: Mutex::new(tx),
            devices,
            watching,
        }
    }

    /// Runs `f` on the HID thread and waits for its result.
    pub fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut HidApi) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        let job: Job = Box::new(move |api| {
            let result = match api {
                Ok(api) => f(api).map_err(|e| format!("{e:#}")),
                Err(e) => Err(format!("USB HID is unavailable: {e}")),
            };
            let _ = tx.send(result);
        });
        self.tx
            .lock()
            .unwrap()
            .send(job)
            .map_err(|_| "the HID thread stopped".to_string())?;
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| "the keypad didn't respond in time".to_string())?
    }
}

fn rescan_inputs(
    api: &HidApi,
    registry: &Registry,
    watched: &mut HashMap<String, Watched>,
    refused: &mut HashSet<String>,
) {
    let present = registry.input_interfaces(api);
    watched.retain(|path, _| present.contains(path));
    // Forget refusals for unplugged devices, so a replug (say, after
    // installing the udev rule) is tried again.
    refused.retain(|path| present.contains(path));

    for path in present {
        if watched.contains_key(&path) || refused.contains(&path) {
            continue;
        }
        let opened = CString::new(path.clone())
            .ok()
            .and_then(|c| api.open_path(&c).ok())
            .and_then(|dev| {
                let mut desc = vec![0u8; 4096];
                let n = dev.get_report_descriptor(&mut desc).ok()?;
                Some(Watched {
                    decoder: Decoder::new(&desc[..n]),
                    dev,
                })
            });
        match opened {
            Some(w) => {
                log::info!("watching keypad input {path}");
                watched.insert(path, w);
            }
            None => {
                log::info!(
                    "can't read keypad input {path}; live key test falls back to the window"
                );
                refused.insert(path);
            }
        }
    }
}
