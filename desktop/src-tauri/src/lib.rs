use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::net::{IpAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_BUFFER_SAMPLES: usize = 240_000;
const UDP_PACKET_SIZE: usize = 8192;
const DISCOVERY_PORT_OFFSET: u16 = 1;

const PROTOCOL_NAME: &str = "mow2";
const PROTOCOL_MAGIC: [u8; 4] = *b"MOW2";
const PROTOCOL_VERSION: u8 = 2;
const PROTOCOL_HEADER_LEN: usize = 28;
const PROTOCOL_FLAG_ENCRYPTED: u8 = 0x01;
const PROTOCOL_PAYLOAD_PCM16: u16 = 1;

const JITTER_STARTUP_PACKETS: usize = 4;
const JITTER_MAX_PENDING: usize = 32;
const DEFAULT_PACKET_SAMPLES: usize = 480;

#[derive(Default)]
struct RuntimeMetrics {
    packets_received: AtomicUsize,
    packets_dropped: AtomicUsize,
    dropped_samples: AtomicUsize,
    decrypt_failures: AtomicUsize,
    parse_errors: AtomicUsize,
    last_error: Mutex<Option<String>>,
}

struct AudioRuntime {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<()>,
    discovery_worker: JoinHandle<()>,
    metrics: Arc<RuntimeMetrics>,
}

#[derive(Default)]
struct AppState {
    runtime: Mutex<Option<AudioRuntime>>,
}

#[derive(Clone)]
struct ReceiverConfig {
    port: u16,
    device_name: Option<String>,
    secure_mode: bool,
    pair_code: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverStatus {
    running: bool,
    packets_received: usize,
    packets_dropped: usize,
    dropped_samples: usize,
    decrypt_failures: usize,
    parse_errors: usize,
    last_error: Option<String>,
}

#[derive(Deserialize)]
struct DiscoveryRequest {
    kind: String,
    protocol: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryResponse {
    kind: &'static str,
    protocol: &'static str,
    name: &'static str,
    port: u16,
    secure_required: bool,
    ips: Vec<String>,
}

#[derive(Debug)]
enum PacketError {
    Parse(String),
    Decrypt(String),
}

struct StreamPacket {
    sequence: u32,
    session_id: u64,
    channels: u16,
    samples: Vec<f32>,
}

struct JitterBuffer {
    pending: BTreeMap<u32, Vec<f32>>,
    expected_sequence: Option<u32>,
    startup_packets: usize,
    max_pending: usize,
    last_packet_samples: usize,
}

impl JitterBuffer {
    fn new(startup_packets: usize, max_pending: usize) -> Self {
        Self {
            pending: BTreeMap::new(),
            expected_sequence: None,
            startup_packets,
            max_pending,
            last_packet_samples: DEFAULT_PACKET_SAMPLES,
        }
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.expected_sequence = None;
        self.last_packet_samples = DEFAULT_PACKET_SAMPLES;
    }

    fn push_packet(
        &mut self,
        sequence: u32,
        samples: Vec<f32>,
        output: &mut VecDeque<f32>,
        metrics: &RuntimeMetrics,
    ) {
        if self.pending.contains_key(&sequence) {
            return;
        }

        self.last_packet_samples = samples.len().max(1);
        self.pending.insert(sequence, samples);

        if self.expected_sequence.is_none() && self.pending.len() >= self.startup_packets {
            self.expected_sequence = self.pending.keys().next().copied();
        }

        while let Some(expected) = self.expected_sequence {
            if let Some(frame) = self.pending.remove(&expected) {
                output.extend(frame);
                self.expected_sequence = Some(expected.wrapping_add(1));
                continue;
            }

            if self.pending.len() > self.max_pending {
                let silence_samples = self.last_packet_samples.max(DEFAULT_PACKET_SAMPLES);
                for _ in 0..silence_samples {
                    output.push_back(0.0);
                }
                metrics.packets_dropped.fetch_add(1, Ordering::Relaxed);
                self.expected_sequence = Some(expected.wrapping_add(1));
                continue;
            }
            break;
        }

        if self.pending.len() > self.max_pending * 3 {
            while self.pending.len() > self.max_pending {
                if let Some(first_seq) = self.pending.keys().next().copied() {
                    self.pending.remove(&first_seq);
                    metrics.packets_dropped.fetch_add(1, Ordering::Relaxed);
                } else {
                    break;
                }
            }
        }
    }
}

#[tauri::command]
fn list_output_devices() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let devices = host
        .output_devices()
        .map_err(|error| format!("Could not query output devices: {error}"))?;

    let mut names = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

#[tauri::command]
fn list_local_ipv4() -> Vec<String> {
    list_local_ipv4_internal()
}

#[tauri::command]
fn start_receiver(
    port: u16,
    device_name: Option<String>,
    secure_mode: bool,
    pair_code: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    if port == 0 || port == u16::MAX {
        return Err(String::from("Port must be between 1 and 65534."));
    }
    if secure_mode {
        let code = pair_code
            .as_deref()
            .map(str::trim)
            .filter(|value| value.len() >= 6)
            .ok_or_else(|| String::from("Secure mode requires pair code with at least 6 chars."))?;
        if code.is_empty() {
            return Err(String::from("Secure mode requires a pair code."));
        }
    }

    {
        let guard = state
            .runtime
            .lock()
            .map_err(|_| String::from("Receiver state lock poisoned."))?;
        if guard.is_some() {
            return Err(String::from("Receiver is already running."));
        }
    }

    let discovery_port = port
        .checked_add(DISCOVERY_PORT_OFFSET)
        .ok_or_else(|| String::from("Port cannot reserve discovery channel."))?;
    let receiver_probe = UdpSocket::bind(("0.0.0.0", port))
        .map_err(|error| format!("Cannot listen on UDP {port}: {error}"))?;
    let discovery_probe = UdpSocket::bind(("0.0.0.0", discovery_port))
        .map_err(|error| format!("Cannot open discovery UDP {discovery_port}: {error}"))?;
    drop(receiver_probe);
    drop(discovery_probe);

    let config = ReceiverConfig {
        port,
        device_name,
        secure_mode,
        pair_code,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let metrics = Arc::new(RuntimeMetrics::default());
    let worker_stop = Arc::clone(&stop);
    let worker_metrics = Arc::clone(&metrics);
    let worker_config = config.clone();
    let worker = thread::spawn(move || {
        if let Err(error) = run_receiver(worker_stop, worker_metrics.clone(), worker_config) {
            set_last_error(&worker_metrics, error);
        }
    });

    let discovery_stop = Arc::clone(&stop);
    let discovery_metrics = Arc::clone(&metrics);
    let discovery_port_config = config.port;
    let discovery_secure = config.secure_mode;
    let discovery_worker = thread::spawn(move || {
        if let Err(error) =
            run_discovery_responder(discovery_stop, discovery_port_config, discovery_secure)
        {
            set_last_error(&discovery_metrics, error);
        }
    });

    let mut guard = state
        .runtime
        .lock()
        .map_err(|_| String::from("Receiver state lock poisoned."))?;
    *guard = Some(AudioRuntime {
        stop,
        worker,
        discovery_worker,
        metrics,
    });

    Ok(())
}

#[tauri::command]
fn stop_receiver(state: tauri::State<AppState>) -> Result<(), String> {
    let runtime = {
        let mut guard = state
            .runtime
            .lock()
            .map_err(|_| String::from("Receiver state lock poisoned."))?;
        guard.take()
    };

    if let Some(runtime) = runtime {
        runtime.stop.store(true, Ordering::Relaxed);
        runtime
            .worker
            .join()
            .map_err(|_| String::from("Receiver worker thread panicked."))?;
        runtime
            .discovery_worker
            .join()
            .map_err(|_| String::from("Discovery worker thread panicked."))?;
    }

    Ok(())
}

#[tauri::command]
fn receiver_status(state: tauri::State<AppState>) -> Result<ReceiverStatus, String> {
    let guard = state
        .runtime
        .lock()
        .map_err(|_| String::from("Receiver state lock poisoned."))?;

    if let Some(runtime) = guard.as_ref() {
        let last_error = runtime
            .metrics
            .last_error
            .lock()
            .ok()
            .and_then(|value| value.clone());
        Ok(ReceiverStatus {
            running: !runtime.worker.is_finished() && !runtime.discovery_worker.is_finished(),
            packets_received: runtime.metrics.packets_received.load(Ordering::Relaxed),
            packets_dropped: runtime.metrics.packets_dropped.load(Ordering::Relaxed),
            dropped_samples: runtime.metrics.dropped_samples.load(Ordering::Relaxed),
            decrypt_failures: runtime.metrics.decrypt_failures.load(Ordering::Relaxed),
            parse_errors: runtime.metrics.parse_errors.load(Ordering::Relaxed),
            last_error,
        })
    } else {
        Ok(ReceiverStatus {
            running: false,
            packets_received: 0,
            packets_dropped: 0,
            dropped_samples: 0,
            decrypt_failures: 0,
            parse_errors: 0,
            last_error: None,
        })
    }
}

fn run_receiver(
    stop: Arc<AtomicBool>,
    metrics: Arc<RuntimeMetrics>,
    config: ReceiverConfig,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = select_output_device(&host, config.device_name.as_deref())?;
    let queue = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(
        MAX_BUFFER_SAMPLES,
    )));
    let stream = build_output_stream(&device, Arc::clone(&queue), Arc::clone(&metrics))?;
    stream
        .play()
        .map_err(|error| format!("Failed to start playback stream: {error}"))?;

    let pairing_key = if config.secure_mode {
        let code = config
            .pair_code
            .as_deref()
            .ok_or_else(|| String::from("Secure mode started without pair code."))?;
        Some(derive_pairing_key(code.trim()))
    } else {
        None
    };

    let socket = UdpSocket::bind(("0.0.0.0", config.port))
        .map_err(|error| format!("Cannot listen on UDP {}: {error}", config.port))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(120)))
        .map_err(|error| format!("Failed to set socket timeout: {error}"))?;

    let mut packet_buffer = [0_u8; UDP_PACKET_SIZE];
    let mut jitter = JitterBuffer::new(JITTER_STARTUP_PACKETS, JITTER_MAX_PENDING);
    let mut current_session: Option<u64> = None;

    while !stop.load(Ordering::Relaxed) {
        match socket.recv_from(&mut packet_buffer) {
            Ok((size, _source)) => {
                let parse_result = parse_stream_packet(
                    &packet_buffer[..size],
                    config.secure_mode,
                    pairing_key.as_ref(),
                );

                match parse_result {
                    Ok(packet) => {
                        metrics.packets_received.fetch_add(1, Ordering::Relaxed);
                        if current_session != Some(packet.session_id) {
                            current_session = Some(packet.session_id);
                            jitter.reset();
                            if let Ok(mut output) = queue.lock() {
                                output.clear();
                            }
                        }

                        if packet.channels == 0 {
                            metrics.parse_errors.fetch_add(1, Ordering::Relaxed);
                            set_last_error(
                                &metrics,
                                String::from("Invalid stream packet channels."),
                            );
                            continue;
                        }

                        if let Ok(mut output) = queue.lock() {
                            jitter.push_packet(
                                packet.sequence,
                                packet.samples,
                                &mut output,
                                &metrics,
                            );
                            if output.len() > MAX_BUFFER_SAMPLES {
                                let overflow = output.len() - MAX_BUFFER_SAMPLES;
                                for _ in 0..overflow {
                                    output.pop_front();
                                }
                                metrics
                                    .dropped_samples
                                    .fetch_add(overflow, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(PacketError::Decrypt(error)) => {
                        metrics.decrypt_failures.fetch_add(1, Ordering::Relaxed);
                        set_last_error(&metrics, error);
                    }
                    Err(PacketError::Parse(error)) => {
                        metrics.parse_errors.fetch_add(1, Ordering::Relaxed);
                        set_last_error(&metrics, error);
                    }
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => {
                return Err(format!("UDP receive error: {error}"));
            }
        }
    }

    Ok(())
}

fn run_discovery_responder(
    stop: Arc<AtomicBool>,
    port: u16,
    secure_mode: bool,
) -> Result<(), String> {
    let discovery_port = port
        .checked_add(DISCOVERY_PORT_OFFSET)
        .ok_or_else(|| String::from("Could not reserve discovery port."))?;
    let socket = UdpSocket::bind(("0.0.0.0", discovery_port))
        .map_err(|error| format!("Cannot bind discovery UDP {discovery_port}: {error}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(120)))
        .map_err(|error| format!("Cannot configure discovery socket timeout: {error}"))?;

    let mut buffer = [0_u8; 2048];
    while !stop.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buffer) {
            Ok((size, source)) => {
                let request_bytes = &buffer[..size];
                let request_text = match std::str::from_utf8(request_bytes) {
                    Ok(value) => value,
                    Err(_) => continue,
                };

                let request = match serde_json::from_str::<DiscoveryRequest>(request_text) {
                    Ok(value) => value,
                    Err(_) => continue,
                };

                if request.kind != "discover" || request.protocol != PROTOCOL_NAME {
                    continue;
                }

                let response = DiscoveryResponse {
                    kind: "discover_response",
                    protocol: PROTOCOL_NAME,
                    name: "micOwireless Desktop",
                    port,
                    secure_required: secure_mode,
                    ips: list_local_ipv4_internal(),
                };

                if let Ok(payload) = serde_json::to_vec(&response) {
                    let _ = socket.send_to(&payload, source);
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => {
                return Err(format!("Discovery socket error: {error}"));
            }
        }
    }

    Ok(())
}

fn parse_stream_packet(
    packet: &[u8],
    secure_mode: bool,
    pairing_key: Option<&[u8; 32]>,
) -> Result<StreamPacket, PacketError> {
    if packet.len() < PROTOCOL_HEADER_LEN {
        return Err(PacketError::Parse(String::from(
            "Packet is smaller than protocol header.",
        )));
    }
    if packet[0..4] != PROTOCOL_MAGIC {
        return Err(PacketError::Parse(String::from("Protocol magic mismatch.")));
    }

    let version = packet[4];
    if version != PROTOCOL_VERSION {
        return Err(PacketError::Parse(format!(
            "Unsupported protocol version: {version}",
        )));
    }

    let flags = packet[5];
    let payload_len = u16::from_le_bytes([packet[6], packet[7]]) as usize;
    if packet.len() != PROTOCOL_HEADER_LEN + payload_len {
        return Err(PacketError::Parse(String::from(
            "Payload length does not match packet size.",
        )));
    }

    let sequence = u32::from_le_bytes([packet[8], packet[9], packet[10], packet[11]]);
    let session_id = u64::from_le_bytes([
        packet[12], packet[13], packet[14], packet[15], packet[16], packet[17], packet[18],
        packet[19],
    ]);
    let channels = u16::from_le_bytes([packet[24], packet[25]]);
    let payload_format = u16::from_le_bytes([packet[26], packet[27]]);
    if payload_format != PROTOCOL_PAYLOAD_PCM16 {
        return Err(PacketError::Parse(format!(
            "Unsupported payload format {payload_format}.",
        )));
    }

    let encrypted = flags & PROTOCOL_FLAG_ENCRYPTED != 0;
    if secure_mode && !encrypted {
        return Err(PacketError::Parse(String::from(
            "Secure receiver requires encrypted packets.",
        )));
    }

    let header = &packet[..PROTOCOL_HEADER_LEN];
    let payload = &packet[PROTOCOL_HEADER_LEN..];
    let audio_bytes = if encrypted {
        let key = pairing_key.ok_or_else(|| {
            PacketError::Decrypt(String::from(
                "Encrypted packet arrived without pairing key.",
            ))
        })?;
        decrypt_audio_payload(payload, header, session_id, sequence, key)?
    } else {
        payload.to_vec()
    };

    if audio_bytes.len() % 2 != 0 {
        return Err(PacketError::Parse(String::from(
            "PCM payload is not aligned to 16-bit samples.",
        )));
    }

    let mut samples = Vec::with_capacity(audio_bytes.len() / 2);
    for chunk in audio_bytes.chunks_exact(2) {
        let value = i16::from_le_bytes([chunk[0], chunk[1]]);
        samples.push(value as f32 / i16::MAX as f32);
    }

    if channels > 1 {
        let channel_count = usize::from(channels);
        let mut mono = Vec::with_capacity(samples.len() / channel_count);
        for frame in samples.chunks(channel_count) {
            let sum: f32 = frame.iter().copied().sum();
            mono.push(sum / channel_count as f32);
        }
        samples = mono;
    }

    Ok(StreamPacket {
        sequence,
        session_id,
        channels,
        samples,
    })
}

fn decrypt_audio_payload(
    payload: &[u8],
    header: &[u8],
    session_id: u64,
    sequence: u32,
    key: &[u8; 32],
) -> Result<Vec<u8>, PacketError> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|error| {
        PacketError::Decrypt(format!("Could not initialize stream cipher: {error}"))
    })?;
    let nonce_value = build_nonce(session_id, sequence);
    let nonce = Nonce::from_slice(&nonce_value);
    cipher
        .decrypt(
            nonce,
            chacha20poly1305::aead::Payload {
                msg: payload,
                aad: header,
            },
        )
        .map_err(|_| PacketError::Decrypt(String::from("Pair code mismatch or tampered packet.")))
}

fn build_nonce(session_id: u64, sequence: u32) -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    nonce[0..8].copy_from_slice(&session_id.to_le_bytes());
    nonce[8..12].copy_from_slice(&sequence.to_le_bytes());
    nonce
}

fn derive_pairing_key(pair_code: &str) -> [u8; 32] {
    let digest = Sha256::digest(pair_code.as_bytes());
    let mut key = [0_u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn list_local_ipv4_internal() -> Vec<String> {
    let mut addresses = Vec::<String>::new();
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in interfaces {
            if let IpAddr::V4(value) = ip {
                let text = value.to_string();
                if !addresses.contains(&text) {
                    addresses.push(text);
                }
            }
        }
    }
    if !addresses.iter().any(|ip| ip == "127.0.0.1") {
        addresses.push(String::from("127.0.0.1"));
    }
    addresses.sort();
    addresses
}

fn select_output_device(
    host: &cpal::Host,
    preferred: Option<&str>,
) -> Result<cpal::Device, String> {
    if let Some(target_name) = preferred {
        let lowered_target = target_name.to_lowercase();
        let devices = host
            .output_devices()
            .map_err(|error| format!("Could not query output devices: {error}"))?;
        for device in devices {
            if let Ok(name) = device.name() {
                if name.to_lowercase() == lowered_target {
                    return Ok(device);
                }
            }
        }
        return Err(format!("Output device '{target_name}' not found."));
    }

    host.default_output_device()
        .ok_or_else(|| String::from("No default output device available."))
}

fn build_output_stream(
    device: &cpal::Device,
    queue: Arc<Mutex<VecDeque<f32>>>,
    metrics: Arc<RuntimeMetrics>,
) -> Result<cpal::Stream, String> {
    let default_config = device
        .default_output_config()
        .map_err(|error| format!("Could not get output config: {error}"))?;
    let sample_format = default_config.sample_format();
    let config: StreamConfig = default_config.into();
    let channel_count = config.channels as usize;

    let stream = match sample_format {
        SampleFormat::F32 => {
            let queue_for_callback = Arc::clone(&queue);
            let metrics_for_error = Arc::clone(&metrics);
            device
                .build_output_stream(
                    &config,
                    move |output: &mut [f32], _| {
                        write_f32_output(output, channel_count, &queue_for_callback);
                    },
                    move |error| {
                        set_last_error(&metrics_for_error, format!("Playback error: {error}"));
                    },
                    None,
                )
                .map_err(|error| format!("Could not create f32 stream: {error}"))?
        }
        SampleFormat::I16 => {
            let queue_for_callback = Arc::clone(&queue);
            let metrics_for_error = Arc::clone(&metrics);
            device
                .build_output_stream(
                    &config,
                    move |output: &mut [i16], _| {
                        write_i16_output(output, channel_count, &queue_for_callback);
                    },
                    move |error| {
                        set_last_error(&metrics_for_error, format!("Playback error: {error}"));
                    },
                    None,
                )
                .map_err(|error| format!("Could not create i16 stream: {error}"))?
        }
        SampleFormat::U16 => {
            let queue_for_callback = Arc::clone(&queue);
            let metrics_for_error = Arc::clone(&metrics);
            device
                .build_output_stream(
                    &config,
                    move |output: &mut [u16], _| {
                        write_u16_output(output, channel_count, &queue_for_callback);
                    },
                    move |error| {
                        set_last_error(&metrics_for_error, format!("Playback error: {error}"));
                    },
                    None,
                )
                .map_err(|error| format!("Could not create u16 stream: {error}"))?
        }
        other => {
            return Err(format!("Unsupported output sample format: {other:?}"));
        }
    };

    Ok(stream)
}

fn write_f32_output(output: &mut [f32], channels: usize, queue: &Arc<Mutex<VecDeque<f32>>>) {
    if let Ok(mut samples) = queue.lock() {
        for frame in output.chunks_mut(channels) {
            let sample = samples.pop_front().unwrap_or(0.0);
            for channel in frame {
                *channel = sample;
            }
        }
    } else {
        for sample in output {
            *sample = 0.0;
        }
    }
}

fn write_i16_output(output: &mut [i16], channels: usize, queue: &Arc<Mutex<VecDeque<f32>>>) {
    if let Ok(mut samples) = queue.lock() {
        for frame in output.chunks_mut(channels) {
            let sample = samples.pop_front().unwrap_or(0.0).clamp(-1.0, 1.0);
            let value = (sample * i16::MAX as f32) as i16;
            for channel in frame {
                *channel = value;
            }
        }
    } else {
        for sample in output {
            *sample = 0;
        }
    }
}

fn write_u16_output(output: &mut [u16], channels: usize, queue: &Arc<Mutex<VecDeque<f32>>>) {
    if let Ok(mut samples) = queue.lock() {
        for frame in output.chunks_mut(channels) {
            let sample = samples.pop_front().unwrap_or(0.0).clamp(-1.0, 1.0);
            let normalized = ((sample + 1.0) * 0.5 * u16::MAX as f32) as u16;
            for channel in frame {
                *channel = normalized;
            }
        }
    } else {
        for sample in output {
            *sample = u16::MAX / 2;
        }
    }
}

fn set_last_error(metrics: &RuntimeMetrics, value: String) {
    if let Ok(mut last_error) = metrics.last_error.lock() {
        *last_error = Some(value);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_local_ipv4,
            list_output_devices,
            start_receiver,
            stop_receiver,
            receiver_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;

    fn packet_from_i16(
        sequence: u32,
        session_id: u64,
        channels: u16,
        samples: &[i16],
        secure_key: Option<&[u8; 32]>,
    ) -> Vec<u8> {
        let mut payload = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            payload.extend_from_slice(&sample.to_le_bytes());
        }

        let flags = if secure_key.is_some() {
            PROTOCOL_FLAG_ENCRYPTED
        } else {
            0
        };
        let payload_bytes = if let Some(key) = secure_key {
            let payload_len = payload.len() + 16;
            let mut header = [0_u8; PROTOCOL_HEADER_LEN];
            header[0..4].copy_from_slice(&PROTOCOL_MAGIC);
            header[4] = PROTOCOL_VERSION;
            header[5] = flags;
            header[6..8].copy_from_slice(&(payload_len as u16).to_le_bytes());
            header[8..12].copy_from_slice(&sequence.to_le_bytes());
            header[12..20].copy_from_slice(&session_id.to_le_bytes());
            header[20..24].copy_from_slice(&48_000_u32.to_le_bytes());
            header[24..26].copy_from_slice(&channels.to_le_bytes());
            header[26..28].copy_from_slice(&PROTOCOL_PAYLOAD_PCM16.to_le_bytes());

            let cipher = ChaCha20Poly1305::new_from_slice(key).expect("cipher");
            let nonce = build_nonce(session_id, sequence);
            cipher
                .encrypt(
                    Nonce::from_slice(&nonce),
                    chacha20poly1305::aead::Payload {
                        msg: &payload,
                        aad: &header,
                    },
                )
                .expect("encrypt")
        } else {
            payload
        };

        let mut packet = Vec::with_capacity(PROTOCOL_HEADER_LEN + payload_bytes.len());
        packet.extend_from_slice(&PROTOCOL_MAGIC);
        packet.push(PROTOCOL_VERSION);
        packet.push(flags);
        packet.extend_from_slice(&(payload_bytes.len() as u16).to_le_bytes());
        packet.extend_from_slice(&sequence.to_le_bytes());
        packet.extend_from_slice(&session_id.to_le_bytes());
        packet.extend_from_slice(&48_000_u32.to_le_bytes());
        packet.extend_from_slice(&channels.to_le_bytes());
        packet.extend_from_slice(&PROTOCOL_PAYLOAD_PCM16.to_le_bytes());
        packet.extend_from_slice(&payload_bytes);
        packet
    }

    #[test]
    fn parse_plain_packet() {
        let packet = packet_from_i16(7, 42, 1, &[1000, -1000, 500], None);
        let parsed = parse_stream_packet(&packet, false, None).expect("parse plain");
        assert_eq!(parsed.sequence, 7);
        assert_eq!(parsed.session_id, 42);
        assert_eq!(parsed.channels, 1);
        assert_eq!(parsed.samples.len(), 3);
    }

    #[test]
    fn parse_encrypted_packet() {
        let key = derive_pairing_key("ABCDEF12");
        let packet = packet_from_i16(9, 500, 1, &[1234, -800], Some(&key));
        let parsed = parse_stream_packet(&packet, true, Some(&key)).expect("parse encrypted");
        assert_eq!(parsed.sequence, 9);
        assert_eq!(parsed.session_id, 500);
        assert_eq!(parsed.samples.len(), 2);
    }

    #[test]
    fn jitter_buffer_drops_missing_packets() {
        let metrics = RuntimeMetrics::default();
        let mut jitter = JitterBuffer::new(1, 2);
        let mut output = VecDeque::<f32>::new();

        jitter.push_packet(0, vec![0.1, 0.2], &mut output, &metrics);
        jitter.push_packet(2, vec![0.3, 0.4], &mut output, &metrics);
        jitter.push_packet(3, vec![0.5, 0.6], &mut output, &metrics);
        jitter.push_packet(4, vec![0.7, 0.8], &mut output, &metrics);

        assert!(
            metrics.packets_dropped.load(Ordering::Relaxed) >= 1,
            "Expected at least one dropped packet from missing sequence"
        );
        assert!(
            output.iter().any(|sample| *sample == 0.0),
            "Expected silence insertion for dropped packet"
        );
    }

    #[test]
    fn discovery_responder_replies() {
        let port_probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
        let base_port = port_probe.local_addr().expect("probe addr").port();
        drop(port_probe);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let worker = thread::spawn(move || run_discovery_responder(stop_clone, base_port, true));
        thread::sleep(Duration::from_millis(150));

        let socket = UdpSocket::bind(("127.0.0.1", 0)).expect("bind requester");
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("timeout");

        let request = serde_json::json!({
            "kind": "discover",
            "protocol": PROTOCOL_NAME
        });
        let payload = serde_json::to_vec(&request).expect("serialize request");
        socket
            .send_to(&payload, ("127.0.0.1", base_port + DISCOVERY_PORT_OFFSET))
            .expect("send request");

        let mut response_found = None;
        let mut buffer = [0_u8; 2048];
        for _ in 0..5 {
            let (size, _) = socket.recv_from(&mut buffer).expect("receive response");
            let response: serde_json::Value =
                serde_json::from_slice(&buffer[..size]).expect("decode response");
            if response["kind"] == "discover_response" {
                response_found = Some(response);
                break;
            }
        }

        let response = response_found.expect("discovery response packet");
        assert_eq!(response["protocol"], PROTOCOL_NAME);
        assert_eq!(response["port"], base_port);
        assert_eq!(response["secureRequired"], true);

        stop.store(true, Ordering::Relaxed);
        let join_result = worker.join().expect("join worker");
        assert!(join_result.is_ok(), "discovery worker should stop cleanly");
    }
}
