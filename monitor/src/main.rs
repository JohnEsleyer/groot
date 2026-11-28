// File: _groot/monitor/src/main.rs
//
// groot-monitor — Interactive 4-tab terminal TUI for Groot game profiling.
//
// Connects persistently to the host's UNIX-domain IPC socket (128-byte framed
// packets) instead of reconnecting every frame, eliminating the per-frame
// thread-thrash in the game process.
//
// Keybindings:
//   [1-4]    Switch tabs (Overview / Jitter & Spikes / Memory & ECS / Logs)
//   [Space]  Pause / resume live polling
//   [S]      Freeze on next spike
//   [E]      Export LLM Flight Recorder report
//   [Q/Esc]  Quit

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Sparkline},
    Terminal,
};
use serde::Deserialize;
use std::{
    collections::VecDeque,
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

// ---------------------------------------------------------------------------
// Wire protocol — 128-byte framed packets (MSG_PACKET = 0x01)
// Must stay in sync with _groot/host/src/telemetry.rs
// ---------------------------------------------------------------------------
const PACKET_SIZE: usize = 128;
const MSG_PACKET: u8 = 0x01;
const MSG_EVENT: u8 = 0x02;
const MSG_CMD_REPLY: u8 = 0x03;

#[derive(Clone, Copy, Default, Debug)]
struct TelemetryPacket {
    frame_index: u64,
    frame_time_us: u32,
    fps: f32,
    arena_peak_bytes: u32,
    arena_reclaimed: u32,
    arena_promoted: u32,
    runaway_warnings: u32,
    entities_alive: u32,
    entities_capacity: u32,
    handle_recycles: u32,
    stale_handle_hits: u32,
    quads_rendered: u32,
    triangles_rendered: u32,
    instances_3d: u32,
    vertex_buffer_bytes: u32,
    draw_calls: u16,
    mouse_x: f32,
    mouse_y: f32,
    mouse_buttons_mask: u8,
    pressed_keys_count: u8,
    // Phase timings (bytes 76+)
    guest_us: u32,
    abi_us: u32,
    gpu_upload_us: u32,
    gpu_encode_us: u32,
    present_wait_us: u32,
    pump_us: u32,
    abi_calls: u32,
    target_frame_us: u32,
    spike_count: u32,
    gpu_reallocs: u16,
    mesh_uploads: u16,
    surface_reconfigs: u16,
    is_spike: u8,
    vsync_enabled: u8,
}

fn decode_packet(buf: &[u8; 128]) -> TelemetryPacket {
    let u32le = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let u16le = |o: usize| u16::from_le_bytes(buf[o..o + 2].try_into().unwrap());
    let f32le = |o: usize| f32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let u64le = |o: usize| u64::from_le_bytes(buf[o..o + 8].try_into().unwrap());
    TelemetryPacket {
        frame_index: u64le(0),
        frame_time_us: u32le(8),
        fps: f32le(12),
        arena_peak_bytes: u32le(16),
        arena_reclaimed: u32le(20),
        arena_promoted: u32le(24),
        runaway_warnings: u32le(28),
        entities_alive: u32le(32),
        entities_capacity: u32le(36),
        handle_recycles: u32le(40),
        stale_handle_hits: u32le(44),
        quads_rendered: u32le(48),
        triangles_rendered: u32le(52),
        instances_3d: u32le(56),
        vertex_buffer_bytes: u32le(60),
        draw_calls: u16le(64),
        mouse_x: f32le(66),
        mouse_y: f32le(70),
        mouse_buttons_mask: buf[74],
        pressed_keys_count: buf[75],
        guest_us: u32le(76),
        abi_us: u32le(80),
        gpu_upload_us: u32le(84),
        gpu_encode_us: u32le(88),
        present_wait_us: u32le(92),
        pump_us: u32le(96),
        abi_calls: u32le(100),
        target_frame_us: u32le(104),
        spike_count: u32le(108),
        gpu_reallocs: u16le(112),
        mesh_uploads: u16le(114),
        surface_reconfigs: u16le(116),
        is_spike: buf[118],
        vsync_enabled: buf[119],
    }
}

// ---------------------------------------------------------------------------
// Session discovery
// ---------------------------------------------------------------------------
#[derive(Deserialize)]
struct SessionInfo {
    pid: u32,
    project_name: String,
    #[serde(default = "default_mode")]
    mode: String,
    socket_path: String,
    #[allow(dead_code)]
    started_at: u64,
}

fn default_mode() -> String {
    "debug".into()
}

fn discover_session(socket_override: Option<&str>) -> Result<SessionInfo, String> {
    if let Some(path) = socket_override {
        return Ok(SessionInfo {
            pid: 0,
            project_name: "unknown".into(),
            mode: "debug".into(),
            socket_path: path.to_string(),
            started_at: 0,
        });
    }
    let raw = std::fs::read_to_string("/tmp/groot-active.json")
        .map_err(|_| "No active Groot debug session found.\nStart one with: groot start .".to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("Malformed session file: {e}"))
}

// ---------------------------------------------------------------------------
// Persistent background receiver — zero per-frame reconnects
// ---------------------------------------------------------------------------
enum SocketMsg {
    Packet(TelemetryPacket),
    Event(String),
    CmdReply(String),
    Disconnected,
}

fn spawn_receiver(socket_path: String) -> Receiver<SocketMsg> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("groot-monitor-recv".into())
        .spawn(move || {
            loop {
                match UnixStream::connect(&socket_path) {
                    Ok(stream) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut reader = BufReader::new(stream);
                        loop {
                            let mut header = [0u8; 3];
                            match reader.read_exact(&mut header) {
                                Ok(_) => {}
                                Err(_) => {
                                    let _ = tx.send(SocketMsg::Disconnected);
                                    break;
                                }
                            }
                            let kind = header[0];
                            let len = u16::from_le_bytes([header[1], header[2]]) as usize;
                            match kind {
                                MSG_PACKET if len == 0 => {
                                    // 128-byte fixed packet
                                    let mut buf = [0u8; 128];
                                    if reader.read_exact(&mut buf).is_ok() {
                                        let pkt = decode_packet(&buf);
                                        let _ = tx.send(SocketMsg::Packet(pkt));
                                    } else {
                                        let _ = tx.send(SocketMsg::Disconnected);
                                        break;
                                    }
                                }
                                MSG_EVENT | MSG_CMD_REPLY if len > 0 => {
                                    let mut payload = vec![0u8; len];
                                    if reader.read_exact(&mut payload).is_ok() {
                                        let s = String::from_utf8_lossy(&payload).to_string();
                                        let msg = if kind == MSG_CMD_REPLY {
                                            SocketMsg::CmdReply(s)
                                        } else {
                                            SocketMsg::Event(s)
                                        };
                                        let _ = tx.send(msg);
                                    } else {
                                        let _ = tx.send(SocketMsg::Disconnected);
                                        break;
                                    }
                                }
                                _ => {
                                    // Skip unknown
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Host not running yet — wait and retry
                        thread::sleep(Duration::from_millis(500));
                    }
                }
                // Reconnect after disconnect
                thread::sleep(Duration::from_millis(100));
            }
        })
        .expect("spawn groot-monitor-recv");
    rx
}

// ---------------------------------------------------------------------------
// Jitter spike record
// ---------------------------------------------------------------------------
#[derive(Clone, Debug)]
struct SpikeRecord {
    frame: u64,
    total_us: u32,
    guest_us: u32,
    abi_us: u32,
    abi_calls: u32,
    upload_us: u32,
    encode_us: u32,
    present_us: u32,
    pump_us: u32,
    dominant: &'static str,
}

fn dominant_phase(p: &TelemetryPacket) -> &'static str {
    let phases = [
        ("guest", p.guest_us),
        ("abi", p.abi_us),
        ("gpu_upload", p.gpu_upload_us),
        ("gpu_encode", p.gpu_encode_us),
        ("present_wait", p.present_wait_us),
        ("pump", p.pump_us),
    ];
    phases.iter().max_by_key(|&&(_, v)| v).map(|&(n, _)| n).unwrap_or("?")
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------
struct App {
    tab: usize,
    paused: bool,
    freeze_on_spike: bool,
    frozen: bool,
    last: TelemetryPacket,
    dt_history: VecDeque<u64>,
    spike_history: Vec<SpikeRecord>,
    log_lines: VecDeque<String>,
    p50_us: u32,
    p95_us: u32,
    sample_buf: VecDeque<u32>,
    dump_reply: Option<String>,
    connected: bool,
    // persistent stream ref for sending commands
    cmd_stream: Option<Arc<Mutex<UnixStream>>>,
}

impl App {
    fn new() -> Self {
        Self {
            tab: 0,
            paused: false,
            freeze_on_spike: false,
            frozen: false,
            last: TelemetryPacket::default(),
            dt_history: VecDeque::from(vec![16; 120]),
            spike_history: Vec::new(),
            log_lines: VecDeque::with_capacity(200),
            p50_us: 16_666,
            p95_us: 16_666,
            sample_buf: VecDeque::with_capacity(512),
            dump_reply: None,
            connected: false,
            cmd_stream: None,
        }
    }

    fn ingest_packet(&mut self, p: TelemetryPacket) {
        if self.frozen {
            return;
        }
        self.last = p;
        if self.dt_history.len() >= 120 {
            self.dt_history.pop_front();
        }
        self.dt_history.push_back((p.frame_time_us / 1000).max(1) as u64);

        // Rolling percentiles (last 512 frames)
        if self.sample_buf.len() >= 512 {
            self.sample_buf.pop_front();
        }
        self.sample_buf.push_back(p.frame_time_us);
        let mut sorted: Vec<u32> = self.sample_buf.iter().copied().collect();
        sorted.sort_unstable();
        let n = sorted.len();
        self.p50_us = sorted[n * 50 / 100];
        self.p95_us = sorted[n * 95 / 100];

        if p.is_spike != 0 {
            let sr = SpikeRecord {
                frame: p.frame_index,
                total_us: p.frame_time_us,
                guest_us: p.guest_us,
                abi_us: p.abi_us,
                abi_calls: p.abi_calls,
                upload_us: p.gpu_upload_us,
                encode_us: p.gpu_encode_us,
                present_us: p.present_wait_us,
                pump_us: p.pump_us,
                dominant: dominant_phase(&p),
            };
            if self.spike_history.len() > 200 {
                self.spike_history.remove(0);
            }
            self.spike_history.push(sr);
            if self.freeze_on_spike {
                self.frozen = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let socket_override = args.iter().find_map(|a| a.strip_prefix("--socket="));

    let session = discover_session(socket_override).map_err(|e| {
        eprintln!("groot-monitor: {e}");
        e
    })?;

    if session.mode == "release" {
        eprintln!(
            "groot-monitor: '{}' is running in release mode — telemetry is disabled.\n\
             Restart with: groot start . --debug",
            session.project_name
        );
        std::process::exit(1);
    }

    let rx = spawn_receiver(session.socket_path.clone());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let session_name = session.project_name.clone();
    let session_pid = session.pid;

    loop {
        // ── Drain receiver ───────────────────────────────────────────────────
        while let Ok(msg) = rx.try_recv() {
            match msg {
                SocketMsg::Packet(pkt) => {
                    app.connected = true;
                    if !app.paused {
                        app.ingest_packet(pkt);
                    }
                }
                SocketMsg::Event(s) => {
                    if app.log_lines.len() >= 200 {
                        app.log_lines.pop_front();
                    }
                    app.log_lines.push_back(s);
                }
                SocketMsg::CmdReply(s) => {
                    app.dump_reply = Some(s);
                }
                SocketMsg::Disconnected => {
                    app.connected = false;
                }
            }
        }

        // ── Render ───────────────────────────────────────────────────────────
        {
            let p = app.last;
            let dt_data: Vec<u64> = app.dt_history.iter().copied().collect();
            let tab = app.tab;
            let paused = app.paused;
            let frozen = app.frozen;
            let freeze_on = app.freeze_on_spike;
            let connected = app.connected;
            let p50 = app.p50_us;
            let p95 = app.p95_us;
            let spikes = app.spike_history.clone();
            let logs: Vec<String> = app.log_lines.iter().cloned().collect();
            let dump_reply = app.dump_reply.clone();
            let name = session_name.clone();

            let target_ms = if p.target_frame_us > 0 {
                p.target_frame_us as f64 / 1000.0
            } else {
                16.666
            };

            terminal.draw(|f| {
                let size = f.size();
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // header
                        Constraint::Length(3), // tab bar
                        Constraint::Min(8),    // body
                        Constraint::Length(3), // footer
                    ])
                    .split(size);

                // ── Header ───────────────────────────────────────────────────
                let status = if !connected {
                    "DISCONNECTED"
                } else if frozen {
                    "FROZEN-ON-SPIKE"
                } else if paused {
                    "PAUSED"
                } else {
                    "LIVE"
                };
                let vsync_str = if p.vsync_enabled != 0 { "VSync:ON" } else { "VSync:OFF" };
                let hdr = format!(
                    " Groot Monitor [PID {}: {}] │ {} │ {} │ Frame#{} ",
                    session_pid, name, vsync_str, status, p.frame_index
                );
                let hdr_color = if !connected { Color::Red } else { Color::Cyan };
                let header = Paragraph::new(Line::from(vec![
                    Span::styled(hdr, Style::default().fg(hdr_color).add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!(" FPS:{:.1} │ p50:{:.1}ms │ p95:{:.1}ms ",
                            p.fps, p50 as f64 / 1000.0, p95 as f64 / 1000.0),
                        Style::default().fg(Color::Yellow),
                    ),
                ])).block(Block::default().borders(Borders::ALL));
                f.render_widget(header, rows[0]);

                // ── Tab bar ──────────────────────────────────────────────────
                let spike_badge = if spikes.is_empty() {
                    "[2] Jitter & Spikes".to_string()
                } else {
                    format!("[2] Jitter & Spikes ({})", spikes.len())
                };
                let tabs_text = format!(
                    " [1] Overview  {}  [3] Memory & ECS  [4] Logs/Spans ",
                    spike_badge
                );
                let tab_bar = Paragraph::new(Line::from(vec![
                    Span::styled(tabs_text, Style::default().fg(Color::White)),
                ])).block(Block::default().borders(Borders::ALL).style(
                    Style::default().fg(if connected { Color::Green } else { Color::DarkGray })
                ));
                f.render_widget(tab_bar, rows[1]);

                match tab {
                    // ── Tab 1: Overview ──────────────────────────────────────
                    0 => {
                        let body = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Length(10), // phase breakdown
                                Constraint::Min(5),     // sparkline + diagnostics
                            ])
                            .split(rows[2]);

                        // Phase budget bars
                        let total = p.frame_time_us.max(1) as f64;
                        let budget = (p.target_frame_us as f64 / 1000.0).max(1.0);
                        let total_ms = total / 1000.0;
                        let pct = |v: u32| ((v as f64 / total * 100.0) as u16).min(100);
                        let ms = |v: u32| v as f64 / 1000.0;

                        let total_ratio = ((total_ms / budget) * 100.0) as u16;
                        let total_color = if total_ratio > 135 { Color::Red } else if total_ratio > 100 { Color::Yellow } else { Color::Green };

                        let phase_lines = vec![
                            (format!("Total     {:>8.2}ms / {:.2}ms budget", total_ms, budget), total_ratio, total_color),
                            (format!("├ Guest   {:>8.2}ms ({:2}%)", ms(p.guest_us), pct(p.guest_us)), pct(p.guest_us), Color::Cyan),
                            (format!("├ ABI     {:>8.2}ms ({:2}%) [{} calls{}]", ms(p.abi_us), pct(p.abi_us), p.abi_calls,
                                if p.abi_calls > 40 { " ⚠HIGH" } else { "" }), pct(p.abi_us), if p.abi_calls > 40 { Color::Red } else { Color::Blue }),
                            (format!("├ Upload  {:>8.2}ms ({:2}%) [realloc:{}  mesh_up:{}]", ms(p.gpu_upload_us), pct(p.gpu_upload_us), p.gpu_reallocs, p.mesh_uploads),
                                pct(p.gpu_upload_us), if p.mesh_uploads > 0 { Color::Yellow } else { Color::LightGreen }),
                            (format!("├ Encode  {:>8.2}ms ({:2}%)", ms(p.gpu_encode_us), pct(p.gpu_encode_us)), pct(p.gpu_encode_us), Color::Green),
                            (format!("├ VSync   {:>8.2}ms ({:2}%) [{} reconf]", ms(p.present_wait_us), pct(p.present_wait_us), p.surface_reconfigs),
                                pct(p.present_wait_us), if p.surface_reconfigs > 0 { Color::Red } else { Color::Magenta }),
                            (format!("└ Pump    {:>8.2}ms ({:2}%)", ms(p.pump_us), pct(p.pump_us)), pct(p.pump_us), Color::DarkGray),
                        ];

                        let phase_rows = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints(vec![Constraint::Length(1); phase_lines.len() + 1])
                            .split(body[0]);

                        let border_block = Block::default()
                            .title(format!(" FRAME BUDGET: {:.1}ms │ Spikes: {} ", budget, p.spike_count))
                            .borders(Borders::ALL)
                            .style(Style::default().fg(total_color));
                        f.render_widget(border_block, body[0]);

                        for (i, (label, ratio, color)) in phase_lines.iter().enumerate() {
                            if i + 1 < phase_rows.len() {
                                let g = Gauge::default()
                                    .gauge_style(Style::default().fg(*color))
                                    .label(label.as_str())
                                    .ratio((*ratio as f64 / 100.0).min(1.0));
                                // Can't render gauge inside already-bordered block; use Paragraph instead
                                let bar_width = phase_rows[i + 1].width.saturating_sub(2) as usize;
                                let fill = (*ratio as usize * bar_width / 100).min(bar_width);
                                let bar = format!("{} {}", "█".repeat(fill), label);
                                let pg = Paragraph::new(Line::from(Span::styled(
                                    bar, Style::default().fg(*color)
                                )));
                                f.render_widget(pg, phase_rows[i + 1]);
                            }
                        }

                        // Sparkline + GPU stats
                        let bottom = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                            .split(body[1]);

                        let spark = Sparkline::default()
                            .block(Block::default().title(" FRAME TIME (last 120, ms) ").borders(Borders::ALL))
                            .data(&dt_data)
                            .style(Style::default().fg(Color::Green));
                        f.render_widget(spark, bottom[0]);

                        let gpu_lines = vec![
                            Line::from(format!("  Draw calls:  {:>8}", p.draw_calls)),
                            Line::from(format!("  Instances3D: {:>8}", p.instances_3d)),
                            Line::from(format!("  Triangles:   {:>8}", p.triangles_rendered)),
                            Line::from(format!("  VBuf:   {:>6.1} KB", p.vertex_buffer_bytes as f64 / 1024.0)),
                        ];
                        let gpu_w = Paragraph::new(gpu_lines)
                            .block(Block::default().title(" GPU STATS ").borders(Borders::ALL));
                        f.render_widget(gpu_w, bottom[1]);
                    }

                    // ── Tab 2: Jitter & Spike Inspector ─────────────────────
                    1 => {
                        let items: Vec<ListItem> = spikes.iter().rev().take(40).map(|sr| {
                            let color = if sr.total_us > 33_000 { Color::Red } else { Color::Yellow };
                            ListItem::new(Line::from(Span::styled(
                                format!(" F#{:<8} {:>6.1}ms dom:{:<13} guest:{:>5} abi:{:>5}({}c) up:{:>5} pres:{:>5}",
                                    sr.frame, sr.total_us as f64 / 1000.0, sr.dominant,
                                    sr.guest_us, sr.abi_us, sr.abi_calls, sr.upload_us, sr.present_us),
                                Style::default().fg(color),
                            )))
                        }).collect();

                        let list = List::new(items)
                            .block(Block::default()
                                .title(format!(" JITTER SPIKES ({} total) │ [S] Freeze-on-Spike: {} ",
                                    spikes.len(), if freeze_on { "ON" } else { "OFF" }))
                                .borders(Borders::ALL))
                            .style(Style::default().fg(Color::White));
                        f.render_widget(list, rows[2]);
                    }

                    // ── Tab 3: Memory & ECS ──────────────────────────────────
                    2 => {
                        let cols = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                            .split(rows[2]);

                        let mem_lines = vec![
                            Line::from(format!("  Arena Peak:    {:>8.1} KB", p.arena_peak_bytes as f64 / 1024.0)),
                            Line::from(format!("  Reclaimed:     {:>8.1} KB", p.arena_reclaimed as f64 / 1024.0)),
                            Line::from(format!("  Promoted:      {:>8} B", p.arena_promoted)),
                            Line::from(format!("  Runaway Loops: {:>8}", p.runaway_warnings)),
                        ];
                        let mem_w = Paragraph::new(mem_lines)
                            .block(Block::default().title(" ARENA MEMORY (Tier 2) ").borders(Borders::ALL)
                                .style(Style::default().fg(Color::Cyan)));
                        f.render_widget(mem_w, cols[0]);

                        let ecs_lines = vec![
                            Line::from(format!("  Entities Alive:  {:>8}", p.entities_alive)),
                            Line::from(format!("  Pool Capacity:   {:>8}", p.entities_capacity)),
                            Line::from(format!("  Recycled Slots:  {:>8}", p.handle_recycles)),
                            Line::from(format!("  Stale Lookups:   {:>8}", p.stale_handle_hits)),
                        ];
                        let ecs_w = Paragraph::new(ecs_lines)
                            .block(Block::default().title(" GENERATIONAL ECS ").borders(Borders::ALL)
                                .style(Style::default().fg(Color::Magenta)));
                        f.render_widget(ecs_w, cols[1]);
                    }

                    // ── Tab 4: Logs & Guest Spans ────────────────────────────
                    _ => {
                        let recent: Vec<ListItem> = logs.iter().rev().take(40)
                            .map(|s| {
                                let color = if s.starts_with("SPIKE") || s.starts_with("ERR") {
                                    Color::Red
                                } else if s.starts_with("WARN") {
                                    Color::Yellow
                                } else if s.starts_with("SPAN") {
                                    Color::Cyan
                                } else {
                                    Color::White
                                };
                                ListItem::new(Line::from(Span::styled(s.clone(), Style::default().fg(color))))
                            })
                            .collect();
                        let list = List::new(recent)
                            .block(Block::default().title(" LOGS & SPANS (newest first) ").borders(Borders::ALL));
                        f.render_widget(list, rows[2]);
                    }
                }

                // ── Footer ───────────────────────────────────────────────────
                let footer_txt = if let Some(ref reply) = dump_reply {
                    format!(" Exported: {}  │ [1-4] Tabs [Space] Pause [S] Freeze [E] Export [Q] Quit", reply.trim())
                } else {
                    " [1-4] Tabs  [Space] Pause  [S] Freeze-on-Spike  [E] Export LLM Report  [Q] Quit".to_string()
                };
                let footer = Paragraph::new(Line::from(Span::styled(
                    footer_txt, Style::default().fg(Color::DarkGray)
                ))).block(Block::default().borders(Borders::ALL));
                f.render_widget(footer, rows[3]);
            })?;
        }

        // ── Event handling ────────────────────────────────────────────────────
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                    KeyCode::Char('1') => app.tab = 0,
                    KeyCode::Char('2') => app.tab = 1,
                    KeyCode::Char('3') => app.tab = 2,
                    KeyCode::Char('4') => app.tab = 3,
                    KeyCode::Char(' ') => {
                        app.paused = !app.paused;
                        if !app.paused {
                            app.frozen = false;
                        }
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        app.freeze_on_spike = !app.freeze_on_spike;
                        if !app.freeze_on_spike {
                            app.frozen = false;
                        }
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') => {
                        // Send DUMP command over a fresh connection (simple approach)
                        app.dump_reply = None;
                        if let Ok(mut stream) = UnixStream::connect(&session.socket_path) {
                            let _ = stream.write_all(b"DUMP\n");
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
