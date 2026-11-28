// File: _groot/host/src/telemetry.rs
//
// Zero-allocation frame telemetry IPC server for Groot debug mode.
// Supports persistent streaming with 128-byte TelemetryPacket, event channels,
// and bidirectional commands (e.g. DUMP).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::thread;

pub const PACKET_SIZE: usize = 128;
pub const MSG_PACKET: u8 = 0x01;
pub const MSG_EVENT: u8 = 0x02;
pub const MSG_CMD_REPLY: u8 = 0x03;

/// Wire representation of one frame's telemetry data.
/// Layout (little-endian, 128 bytes):
///  [0..8]    frame_index          u64
///  [8..12]   frame_time_us        u32
///  [12..16]  fps                  f32
///  [16..20]  arena_peak_bytes     u32
///  [20..24]  arena_reclaimed      u32
///  [24..28]  arena_promoted       u32
///  [28..32]  runaway_warnings     u32
///  [32..36]  entities_alive       u32
///  [36..40]  entities_capacity    u32
///  [40..44]  handle_recycles      u32
///  [44..48]  stale_handle_hits    u32
///  [48..52]  quads_rendered       u32
///  [52..56]  triangles_rendered   u32
///  [56..60]  instances_3d         u32
///  [60..64]  vertex_buffer_bytes  u32
///  [64..66]  draw_calls           u16
///  [66..70]  mouse_x              f32
///  [70..74]  mouse_y              f32
///  [74]      mouse_buttons_mask   u8
///  [75]      pressed_keys_count   u8
///  [76..80]  guest_us             u32
///  [80..84]  abi_us               u32
///  [84..88]  gpu_upload_us        u32
///  [88..92]  gpu_encode_us        u32
///  [92..96]  present_wait_us      u32
///  [96..100] pump_us              u32
///  [100..104] abi_calls           u32
///  [104..108] target_frame_us     u32
///  [108..112] spike_count         u32
///  [112..114] gpu_reallocs        u16
///  [114..116] mesh_uploads        u16
///  [116..118] surface_reconfigs   u16
///  [118]     is_spike             u8
///  [119]     vsync_enabled        u8
///  [120..128] _pad                [u8; 8]
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct TelemetryPacket {
    // Timing
    pub frame_index: u64,
    pub frame_time_us: u32,
    pub fps: f32,

    // Tier 2 — Bump Arenas
    pub arena_peak_bytes: u32,
    pub arena_reclaimed_bytes: u32,
    pub arena_promoted_bytes: u32,
    pub runaway_loop_warnings: u32,

    // Tier 3 — Generational Pools (ECS)
    pub entities_alive: u32,
    pub entities_capacity: u32,
    pub handle_recycles: u32,
    pub stale_handle_hits: u32,

    // Renderer
    pub quads_rendered: u32,
    pub triangles_rendered: u32,
    pub instances_3d: u32,
    pub vertex_buffer_bytes: u32,
    pub draw_calls: u16,

    // Input
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_buttons_mask: u8,
    pub pressed_keys_count: u8,

    // Flight Recorder Phase Timings
    pub guest_us: u32,
    pub abi_us: u32,
    pub gpu_upload_us: u32,
    pub gpu_encode_us: u32,
    pub present_wait_us: u32,
    pub pump_us: u32,
    pub abi_calls: u32,
    pub target_frame_us: u32,
    pub spike_count: u32,
    pub gpu_reallocs: u16,
    pub mesh_uploads: u16,
    pub surface_reconfigs: u16,
    pub is_spike: u8,
    pub vsync_enabled: u8,
}

impl TelemetryPacket {
    /// Serialise to the 128-byte wire format (little-endian).
    pub fn to_bytes(self) -> [u8; 128] {
        let mut buf = [0u8; 128];
        buf[0..8].copy_from_slice(&self.frame_index.to_le_bytes());
        buf[8..12].copy_from_slice(&self.frame_time_us.to_le_bytes());
        buf[12..16].copy_from_slice(&self.fps.to_le_bytes());
        buf[16..20].copy_from_slice(&self.arena_peak_bytes.to_le_bytes());
        buf[20..24].copy_from_slice(&self.arena_reclaimed_bytes.to_le_bytes());
        buf[24..28].copy_from_slice(&self.arena_promoted_bytes.to_le_bytes());
        buf[28..32].copy_from_slice(&self.runaway_loop_warnings.to_le_bytes());
        buf[32..36].copy_from_slice(&self.entities_alive.to_le_bytes());
        buf[36..40].copy_from_slice(&self.entities_capacity.to_le_bytes());
        buf[40..44].copy_from_slice(&self.handle_recycles.to_le_bytes());
        buf[44..48].copy_from_slice(&self.stale_handle_hits.to_le_bytes());
        buf[48..52].copy_from_slice(&self.quads_rendered.to_le_bytes());
        buf[52..56].copy_from_slice(&self.triangles_rendered.to_le_bytes());
        buf[56..60].copy_from_slice(&self.instances_3d.to_le_bytes());
        buf[60..64].copy_from_slice(&self.vertex_buffer_bytes.to_le_bytes());
        buf[64..66].copy_from_slice(&self.draw_calls.to_le_bytes());
        buf[66..70].copy_from_slice(&self.mouse_x.to_le_bytes());
        buf[70..74].copy_from_slice(&self.mouse_y.to_le_bytes());
        buf[74] = self.mouse_buttons_mask;
        buf[75] = self.pressed_keys_count;
        buf[76..80].copy_from_slice(&self.guest_us.to_le_bytes());
        buf[80..84].copy_from_slice(&self.abi_us.to_le_bytes());
        buf[84..88].copy_from_slice(&self.gpu_upload_us.to_le_bytes());
        buf[88..92].copy_from_slice(&self.gpu_encode_us.to_le_bytes());
        buf[92..96].copy_from_slice(&self.present_wait_us.to_le_bytes());
        buf[96..100].copy_from_slice(&self.pump_us.to_le_bytes());
        buf[100..104].copy_from_slice(&self.abi_calls.to_le_bytes());
        buf[104..108].copy_from_slice(&self.target_frame_us.to_le_bytes());
        buf[108..112].copy_from_slice(&self.spike_count.to_le_bytes());
        buf[112..114].copy_from_slice(&self.gpu_reallocs.to_le_bytes());
        buf[114..116].copy_from_slice(&self.mesh_uploads.to_le_bytes());
        buf[116..118].copy_from_slice(&self.surface_reconfigs.to_le_bytes());
        buf[118] = self.is_spike;
        buf[119] = self.vsync_enabled;
        buf
    }

    pub fn from_bytes(buf: &[u8; 128]) -> Self {
        Self {
            frame_index: u64::from_le_bytes(buf[0..8].try_into().unwrap()),
            frame_time_us: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            fps: f32::from_le_bytes(buf[12..16].try_into().unwrap()),
            arena_peak_bytes: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            arena_reclaimed_bytes: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            arena_promoted_bytes: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            runaway_loop_warnings: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            entities_alive: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
            entities_capacity: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            handle_recycles: u32::from_le_bytes(buf[40..44].try_into().unwrap()),
            stale_handle_hits: u32::from_le_bytes(buf[44..48].try_into().unwrap()),
            quads_rendered: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
            triangles_rendered: u32::from_le_bytes(buf[52..56].try_into().unwrap()),
            instances_3d: u32::from_le_bytes(buf[56..60].try_into().unwrap()),
            vertex_buffer_bytes: u32::from_le_bytes(buf[60..64].try_into().unwrap()),
            draw_calls: u16::from_le_bytes(buf[64..66].try_into().unwrap()),
            mouse_x: f32::from_le_bytes(buf[66..70].try_into().unwrap()),
            mouse_y: f32::from_le_bytes(buf[70..74].try_into().unwrap()),
            mouse_buttons_mask: buf[74],
            pressed_keys_count: buf[75],
            guest_us: u32::from_le_bytes(buf[76..80].try_into().unwrap()),
            abi_us: u32::from_le_bytes(buf[80..84].try_into().unwrap()),
            gpu_upload_us: u32::from_le_bytes(buf[84..88].try_into().unwrap()),
            gpu_encode_us: u32::from_le_bytes(buf[88..92].try_into().unwrap()),
            present_wait_us: u32::from_le_bytes(buf[92..96].try_into().unwrap()),
            pump_us: u32::from_le_bytes(buf[96..100].try_into().unwrap()),
            abi_calls: u32::from_le_bytes(buf[100..104].try_into().unwrap()),
            target_frame_us: u32::from_le_bytes(buf[104..108].try_into().unwrap()),
            spike_count: u32::from_le_bytes(buf[108..112].try_into().unwrap()),
            gpu_reallocs: u16::from_le_bytes(buf[112..114].try_into().unwrap()),
            mesh_uploads: u16::from_le_bytes(buf[114..116].try_into().unwrap()),
            surface_reconfigs: u16::from_le_bytes(buf[116..118].try_into().unwrap()),
            is_spike: buf[118],
            vsync_enabled: buf[119],
        }
    }
}

pub struct SharedState {
    pub packets: VecDeque<[u8; 128]>,
    pub events: VecDeque<String>,
    pub dump_requested: bool,
    pub dump_reply: Option<String>,
}

pub struct TelemetryServer {
    state: Arc<Mutex<SharedState>>,
    socket_path: String,
    has_clients: Arc<std::sync::atomic::AtomicBool>,
}

impl TelemetryServer {
    pub fn start(project_name: &str) -> Option<Self> {
        let pid = std::process::id();
        let socket_path = format!("/tmp/groot-{}.sock", pid);
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path).ok()?;

        let session_json = format!(
            r#"{{"pid":{},"project_name":"{}","mode":"debug","socket_path":"{}","started_at":{}}}"#,
            pid,
            project_name,
            socket_path,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        let _ = std::fs::write("/tmp/groot-active.json", &session_json);

        let state = Arc::new(Mutex::new(SharedState {
            packets: VecDeque::with_capacity(128),
            events: VecDeque::with_capacity(128),
            dump_requested: false,
            dump_reply: None,
        }));
        let has_clients = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let state_clone = Arc::clone(&state);
        let has_clients_clone = Arc::clone(&has_clients);

        thread::Builder::new()
            .name("groot-telemetry-accept".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let state_per_client = Arc::clone(&state_clone);
                    let hc = Arc::clone(&has_clients_clone);
                    hc.store(true, std::sync::atomic::Ordering::Relaxed);

                    thread::Builder::new()
                        .name("groot-telemetry-client".into())
                        .spawn(move || {
                            let _ = Self::handle_client(stream, state_per_client);
                            hc.store(false, std::sync::atomic::Ordering::Relaxed);
                        })
                        .ok();
                }
            })
            .ok()?;

        Some(Self {
            state,
            socket_path,
            has_clients,
        })
    }

    fn handle_client(stream: UnixStream, state: Arc<Mutex<SharedState>>) -> std::io::Result<()> {
        let stream_read = stream.try_clone()?;
        let mut stream_write = stream;

        // Command reader thread for bidirectional interaction (e.g. DUMP)
        let state_for_reader = Arc::clone(&state);
        let reader_handle = thread::spawn(move || {
            let reader = BufReader::new(stream_read);
            for line in reader.lines() {
                if let Ok(line) = line {
                    let trimmed = line.trim();
                    if trimmed == "DUMP" {
                        let mut g = state_for_reader.lock().unwrap();
                        g.dump_requested = true;
                    }
                } else {
                    break;
                }
            }
        });

        loop {
            if reader_handle.is_finished() {
                break;
            }

            // Drain queued packets & events
            let (packets_to_send, events_to_send, reply_to_send) = {
                let mut g = state.lock().unwrap();
                let pkts: Vec<[u8; 128]> = g.packets.drain(..).collect();
                let evs: Vec<String> = g.events.drain(..).collect();
                let rep = g.dump_reply.take();
                (pkts, evs, rep)
            };

            for pkt in packets_to_send {
                let len_bytes = 128u16.to_le_bytes();
                let header = [MSG_PACKET, len_bytes[0], len_bytes[1]];
                let _ = stream_write.write_all(&header);
                if stream_write.write_all(&pkt).is_err() {
                    return Ok(());
                }
            }

            for ev in events_to_send {
                let bytes = ev.as_bytes();
                let len = bytes.len() as u16;
                let len_bytes = len.to_le_bytes();
                let frame = [MSG_EVENT, len_bytes[0], len_bytes[1]];
                let _ = stream_write.write_all(&frame);
                if stream_write.write_all(bytes).is_err() {
                    return Ok(());
                }
            }

            if let Some(rep) = reply_to_send {
                let bytes = rep.as_bytes();
                let len = bytes.len() as u16;
                let len_bytes = len.to_le_bytes();
                let frame = [MSG_CMD_REPLY, len_bytes[0], len_bytes[1]];
                let _ = stream_write.write_all(&frame);
                if stream_write.write_all(bytes).is_err() {
                    return Ok(());
                }
            }

            let _ = stream_write.flush();
            thread::sleep(std::time::Duration::from_millis(16));
        }

        Ok(())
    }

    #[inline]
    pub fn has_clients(&self) -> bool {
        self.has_clients.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn publish(&self, packet: TelemetryPacket) {
        let bytes = packet.to_bytes();
        if let Ok(mut guard) = self.state.lock() {
            if guard.packets.len() >= 128 {
                guard.packets.pop_front();
            }
            guard.packets.push_back(bytes);
        }
    }

    pub fn publish_event(&self, event_str: String) {
        if let Ok(mut guard) = self.state.lock() {
            if guard.events.len() >= 128 {
                guard.events.pop_front();
            }
            guard.events.push_back(event_str);
        }
    }

    pub fn check_dump_requested(&self) -> bool {
        if let Ok(mut guard) = self.state.lock() {
            let req = guard.dump_requested;
            guard.dump_requested = false;
            req
        } else {
            false
        }
    }

    pub fn send_dump_reply(&self, reply: String) {
        if let Ok(mut guard) = self.state.lock() {
            guard.dump_reply = Some(reply);
        }
    }
}

impl Drop for TelemetryServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file("/tmp/groot-active.json");
    }
}
