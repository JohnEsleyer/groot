// File: _groot/host/src/telemetry.rs
//
// Zero-allocation frame telemetry IPC server for Groot debug mode.
//
// Every frame boundary (`present()`) the host calls `TelemetryServer::publish()`
// with a freshly populated `TelemetryPacket`. Subscriber clients (e.g. `groot
// monitor`) connect over a UNIX domain socket and receive a continuous binary
// stream of `TelemetryPacket` frames.
//
// Architecture:
//   - A background thread accepts connections on `/tmp/groot-<pid>.sock`.
//   - Each accepted connection is served by its own thread that drains
//     the `latest_packet` lock and writes the bytes.  Stale clients that
//     block are automatically dropped when the write fails.
//   - `publish()` is O(1) — it only swaps an `Arc<[u8]>` under a Mutex.

use std::io::Write;
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};
use std::thread;

/// Wire representation of one frame's telemetry data.
/// Must stay in sync with `TelemetryPacket` in `cmd/groot/telemetry.go`.
///
/// Layout (little-endian, 80 bytes):
///  [0..8]   frame_index          u64
///  [8..12]  frame_time_us        u32
///  [12..16] fps                  f32
///  [16..20] arena_peak_bytes     u32
///  [20..24] arena_reclaimed      u32
///  [24..28] arena_promoted       u32
///  [28..32] runaway_warnings     u32
///  [32..36] entities_alive       u32
///  [36..40] entities_capacity    u32
///  [40..44] handle_recycles      u32
///  [44..48] stale_handle_hits    u32
///  [48..52] quads_rendered       u32
///  [52..56] triangles_rendered   u32
///  [56..60] instances_3d         u32
///  [60..64] vertex_buffer_bytes  u32
///  [64..66] draw_calls           u16
///  [66..70] mouse_x              f32
///  [70..74] mouse_y              f32
///  [74]     mouse_buttons_mask   u8
///  [75]     pressed_keys_count   u8
///  [76..80] _pad                 u16 + u16  (alignment)
#[repr(C)]
#[derive(Clone, Copy, Default)]
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
}

impl TelemetryPacket {
    /// Serialise to the 80-byte wire format (little-endian).
    pub fn to_bytes(self) -> [u8; 80] {
        let mut buf = [0u8; 80];
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
        // bytes 76..80 are padding zeros
        buf
    }
}

/// Background IPC server that streams `TelemetryPacket` frames to any
/// number of `groot monitor` clients over a UNIX domain socket.
pub struct TelemetryServer {
    /// The most recently published packet, shared with the accept thread.
    latest: Arc<Mutex<Option<Arc<[u8; 80]>>>>,
    socket_path: String,
    /// Whether any client is currently connected (cheaply observable for
    /// the hot path so we can skip serialisation when nobody is listening).
    has_clients: Arc<std::sync::atomic::AtomicBool>,
}

impl TelemetryServer {
    /// Start the IPC server for `project_name` and return the server handle.
    ///
    /// Returns `None` if the socket cannot be created (e.g. permission
    /// denied), so callers degrade gracefully in constrained environments.
    pub fn start(project_name: &str) -> Option<Self> {
        let pid = std::process::id();
        let socket_path = format!("/tmp/groot-{}.sock", pid);

        // Remove a stale socket from a previous run, if any.
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path).ok()?;

        // Write the session discovery file for `groot monitor`.
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

        let latest: Arc<Mutex<Option<Arc<[u8; 80]>>>> = Arc::new(Mutex::new(None));
        let has_clients = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let latest_clone = Arc::clone(&latest);
        let has_clients_clone = Arc::clone(&has_clients);

        thread::Builder::new()
            .name("groot-telemetry-accept".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let latest_per_client = Arc::clone(&latest_clone);
                    let hc = Arc::clone(&has_clients_clone);
                    hc.store(true, std::sync::atomic::Ordering::Relaxed);

                    thread::Builder::new()
                        .name("groot-telemetry-client".into())
                        .spawn(move || {
                            loop {
                                // Read the latest packet under the lock.
                                let packet = {
                                    let guard = latest_per_client.lock().unwrap();
                                    guard.clone()
                                };
                                if let Some(bytes) = packet {
                                    if stream.write_all(bytes.as_ref()).is_err() {
                                        // Client disconnected.
                                        break;
                                    }
                                    // Throttle: ~60 Hz delivery.
                                    thread::sleep(std::time::Duration::from_millis(16));
                                } else {
                                    thread::sleep(std::time::Duration::from_millis(5));
                                }
                            }
                            hc.store(false, std::sync::atomic::Ordering::Relaxed);
                        })
                        .ok();
                }
            })
            .ok()?;

        Some(Self {
            latest,
            socket_path,
            has_clients,
        })
    }

    /// Returns true if at least one `groot monitor` client is connected.
    /// Used to skip serialisation on the hot path when no one is watching.
    #[inline]
    pub fn has_clients(&self) -> bool {
        self.has_clients.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Publish a new frame telemetry packet to all connected clients.
    /// This is called on every `present()` and must be very fast.
    pub fn publish(&self, packet: TelemetryPacket) {
        // Even if nobody is listening right now, we always store the latest
        // packet so a client that connects mid-game gets data immediately.
        let bytes: Arc<[u8; 80]> = Arc::new(packet.to_bytes());
        if let Ok(mut guard) = self.latest.lock() {
            *guard = Some(bytes);
        }
    }
}

impl Drop for TelemetryServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file("/tmp/groot-active.json");
    }
}
