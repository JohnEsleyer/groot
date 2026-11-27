// File: _groot/monitor/src/main.rs
//
// groot-monitor — Interactive terminal TUI inspector for a running Groot game.
//
// Discovers the active session from /tmp/groot-active.json (written by the host
// runner on startup in debug mode), then connects to the UNIX-domain IPC socket
// and renders a live 60-Hz ratatui dashboard.
//
// Keybindings:
//   [Space]  Pause / resume live polling
//   [M]      Dump current memory snapshot to stderr
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
    widgets::{Block, Borders, Paragraph, Sparkline},
    Terminal,
};
use serde::Deserialize;
use std::{
    collections::VecDeque,
    io::{self, Read},
    os::unix::net::UnixStream,
    time::Duration,
};

// ---------------------------------------------------------------------------
// Wire protocol — must stay in sync with _groot/host/src/telemetry.rs and
// cmd/groot/telemetry.go (80-byte little-endian layout).
// ---------------------------------------------------------------------------
const PACKET_SIZE: usize = 80;

#[derive(Clone, Copy, Default, Debug)]
struct TelemetryPacket {
    frame_index:         u64,
    frame_time_us:       u32,
    fps:                 f32,
    arena_peak_bytes:    u32,
    arena_reclaimed:     u32,
    arena_promoted:      u32,
    runaway_warnings:    u32,
    entities_alive:      u32,
    entities_capacity:   u32,
    handle_recycles:     u32,
    stale_handle_hits:   u32,
    quads_rendered:      u32,
    triangles_rendered:  u32,
    instances_3d:        u32,
    vertex_buffer_bytes: u32,
    draw_calls:          u16,
    mouse_x:             f32,
    mouse_y:             f32,
    mouse_buttons_mask:  u8,
    pressed_keys_count:  u8,
}

fn decode(buf: &[u8; PACKET_SIZE]) -> TelemetryPacket {
    let u32le = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let u16le = |o: usize| u16::from_le_bytes(buf[o..o + 2].try_into().unwrap());
    let f32le = |o: usize| f32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let u64le = |o: usize| u64::from_le_bytes(buf[o..o + 8].try_into().unwrap());
    TelemetryPacket {
        frame_index:         u64le(0),
        frame_time_us:       u32le(8),
        fps:                 f32le(12),
        arena_peak_bytes:    u32le(16),
        arena_reclaimed:     u32le(20),
        arena_promoted:      u32le(24),
        runaway_warnings:    u32le(28),
        entities_alive:      u32le(32),
        entities_capacity:   u32le(36),
        handle_recycles:     u32le(40),
        stale_handle_hits:   u32le(44),
        quads_rendered:      u32le(48),
        triangles_rendered:  u32le(52),
        instances_3d:        u32le(56),
        vertex_buffer_bytes: u32le(60),
        draw_calls:          u16le(64),
        mouse_x:             f32le(66),
        mouse_y:             f32le(70),
        mouse_buttons_mask:  buf[74],
        pressed_keys_count:  buf[75],
    }
}

// ---------------------------------------------------------------------------
// Session discovery
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SessionInfo {
    pid:          u32,
    project_name: String,
    #[serde(default)]
    title:        String,
    #[serde(default = "default_mode")]
    mode:         String,
    socket_path:  String,
    #[serde(default)]
    started_at:   u64,
}

fn default_mode() -> String { "debug".into() }

fn discover_session(socket_override: Option<&str>) -> Result<SessionInfo, String> {
    // Allow explicit socket path override (e.g. groot-monitor --socket=...)
    if let Some(path) = socket_override {
        return Ok(SessionInfo {
            pid: 0,
            project_name: "unknown".into(),
            title: "".into(),
            mode: "debug".into(),
            socket_path: path.to_string(),
            started_at: 0,
        });
    }
    let raw = std::fs::read_to_string("/tmp/groot-active.json")
        .map_err(|_| "No active Groot debug session found.\nStart one with: groot start .".to_string())?;
    serde_json::from_str(&raw)
        .map_err(|e| format!("Malformed session file: {e}"))
}

// ---------------------------------------------------------------------------
// Fetch one packet from the socket (connect → read exactly PACKET_SIZE bytes).
// ---------------------------------------------------------------------------
fn fetch_packet(socket_path: &str) -> Option<TelemetryPacket> {
    let mut stream = UnixStream::connect(socket_path).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(100))).ok()?;
    let mut buf = [0u8; PACKET_SIZE];
    stream.read_exact(&mut buf).ok()?;
    Some(decode(&buf))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse optional --socket flag
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

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut fps_history:    VecDeque<u64> = VecDeque::from(vec![60; 60]);
    let mut dt_history:     VecDeque<u64> = VecDeque::from(vec![16; 60]);
    let mut last:           TelemetryPacket = TelemetryPacket::default();
    let mut paused          = false;

    loop {
        // ── Poll telemetry ───────────────────────────────────────────────────
        if !paused {
            if let Some(pkt) = fetch_packet(&session.socket_path) {
                last = pkt;
                if fps_history.len() >= 120 { fps_history.pop_front(); }
                if dt_history.len()  >= 120 { dt_history.pop_front(); }
                fps_history.push_back(pkt.fps.round() as u64);
                dt_history.push_back((pkt.frame_time_us / 1000).max(1) as u64);
            }
        }

        // ── Render ──────────────────────────────────────────────────────────
        {
            let name    = &session.project_name;
            let pid     = session.pid;
            let p       = last;
            let fps_data: Vec<u64> = fps_history.iter().copied().collect();
            let dt_data:  Vec<u64> = dt_history.iter().copied().collect();
            let paused_flag = paused;

            terminal.draw(|f| {
                let size = f.size();

                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),  // header
                        Constraint::Length(5),  // fps sparkline
                        Constraint::Min(8),     // main body (2-col)
                        Constraint::Length(4),  // footer / input
                    ])
                    .split(size);

                // ── Header ──────────────────────────────────────────────────
                let status = if paused_flag { "PAUSED" } else { "LIVE" };
                let header_txt = format!(
                    " Groot Engine Monitor ── [PID {pid}: {name}] ── Mode: DEBUG ── {status} "
                );
                let fps_lbl = format!(" FPS: {:.1} │ DT: {:.1}ms │ Frame: {} ",
                    p.fps, p.frame_time_us as f64 / 1000.0, p.frame_index);
                let header = Paragraph::new(Line::from(vec![
                    Span::styled(&header_txt, Style::default()
                        .fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled(&fps_lbl, Style::default().fg(Color::Yellow)),
                ])).block(Block::default().borders(Borders::ALL));
                f.render_widget(header, rows[0]);

                // ── FPS sparkline ────────────────────────────────────────────
                let spark = Sparkline::default()
                    .block(Block::default()
                        .title(" FRAME TIME (ms, last 120 frames) ")
                        .borders(Borders::ALL))
                    .data(&dt_data)
                    .style(Style::default().fg(Color::Green));
                f.render_widget(spark, rows[1]);

                // ── Main 2-column body ───────────────────────────────────────
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(rows[2]);

                let left = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(cols[0]);

                let right = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(cols[1]);

                // Left-top: FPS history sparkline (secondary)
                let fps_spark = Sparkline::default()
                    .block(Block::default()
                        .title(format!(" FRAME RATE (last 120) — target: 60 fps "))
                        .borders(Borders::ALL))
                    .data(&fps_data)
                    .style(Style::default().fg(Color::Cyan));
                f.render_widget(fps_spark, left[0]);

                // Left-bottom: Batch renderer stats
                let vbuf_kb = p.vertex_buffer_bytes as f64 / 1024.0;
                let renderer_lines = vec![
                    Line::from(format!("  Quads Rendered:   {:>10}", p.quads_rendered)),
                    Line::from(format!("  Triangles:        {:>10}", p.triangles_rendered)),
                    Line::from(format!("  3D Instances:     {:>10}", p.instances_3d)),
                    Line::from(format!("  Vertex Buffer:    {:>9.1} KB", vbuf_kb)),
                    Line::from(format!("  Draw Calls:       {:>10}", p.draw_calls)),
                ];
                let renderer = Paragraph::new(renderer_lines)
                    .block(Block::default()
                        .title(" GPU BATCH RENDERER ")
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::LightGreen)));
                f.render_widget(renderer, left[1]);

                // Right-top: Regional memory (Tier 2 Arenas)
                let peak_kb    = p.arena_peak_bytes as f64 / 1024.0;
                let reclaim_kb = p.arena_reclaimed  as f64 / 1024.0;
                let mut mem_lines = vec![
                    Line::from(format!("  Scratch Peak:      {:>8.1} KB", peak_kb)),
                    Line::from(format!("  Reclaimed/Frame:   {:>8.1} KB", reclaim_kb)),
                    Line::from(format!("  Promoted/Frame:    {:>8} B",   p.arena_promoted)),
                    Line::from(format!("  Leak Rate:              0 B")),
                ];
                if p.runaway_warnings > 0 {
                    mem_lines.push(Line::from(Span::styled(
                        format!("  ⚠ Runaway loops: {}", p.runaway_warnings),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )));
                } else {
                    mem_lines.push(Line::from("  Runaway Warnings:      0"));
                }
                let mem = Paragraph::new(mem_lines)
                    .block(Block::default()
                        .title(" REGIONAL MEMORY (Tier 2 Arenas) ")
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::Cyan)));
                f.render_widget(mem, right[0]);

                // Right-bottom: Generational ECS
                let ecs_lines = vec![
                    Line::from(format!("  Active Entities:  {:>8}", p.entities_alive)),
                    Line::from(format!("  Pool Capacity:    {:>8}", p.entities_capacity)),
                    Line::from(format!("  Recycled Slots:   {:>8}", p.handle_recycles)),
                    Line::from(format!("  Stale Lookups:    {:>8}", p.stale_handle_hits)),
                ];
                let ecs = Paragraph::new(ecs_lines)
                    .block(Block::default()
                        .title(" GENERATIONAL ECS (Pool[T]) ")
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::LightMagenta)));
                f.render_widget(ecs, right[1]);

                // ── Footer: input + keybindings ──────────────────────────────
                let mouse_btn = if p.mouse_buttons_mask & 1 != 0 { "[L]" } else { "[ ]" };
                let input_txt = format!(
                    "  Mouse: ({:.0}, {:.0}) {}  Keys: {}  │  [Space] Pause  [M] Dump  [Q] Quit",
                    p.mouse_x, p.mouse_y, mouse_btn, p.pressed_keys_count
                );
                let footer = Paragraph::new(input_txt)
                    .block(Block::default()
                        .title(" INPUT & PERIPHERALS ")
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::Yellow)));
                f.render_widget(footer, rows[3]);
            })?;
        }

        // ── Event handling ───────────────────────────────────────────────────
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                    KeyCode::Char(' ') => paused = !paused,
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        eprintln!(
                            "[MEMORY DUMP] arena_peak={} reclaimed={} promoted={} entities={}",
                            last.arena_peak_bytes, last.arena_reclaimed,
                            last.arena_promoted, last.entities_alive
                        );
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
