// _groot/host/src/trace.rs
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::time::{SystemTime, UNIX_EPOCH};

pub const TRACE_RING_CAP: usize = 512;

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimings {
    pub frame_index: u64,
    pub total_us: u32,
    pub pump_us: u32,
    pub guest_us: u32,
    pub abi_us: u32,
    pub gpu_upload_us: u32,
    pub gpu_encode_us: u32,
    pub present_wait_us: u32,
    pub dt_ms: f32,
    pub fps: f32,
    pub abi_calls: u32,
    pub draw_calls: u16,
    pub instances_3d: u32,
    pub triangles: u32,
    pub gpu_buffer_reallocs: u16,
    pub mesh_uploads: u16,
    pub surface_reconfigs: u16,
    pub arena_peak_bytes: u32,
    pub arena_reclaimed_bytes: u32,
    pub arena_promoted_bytes: u32,
    pub entities_alive: u32,
    pub active_span_count: u16,
}

#[derive(Clone, Debug)]
pub struct TraceEvent {
    pub frame_index: u64,
    pub time_us_in_frame: u32,
    pub level: &'static str, // "INFO", "WARN", "ERR", "SPAN", "SPIKE"
    pub category: &'static str, // "gpu", "abi", "arena", "ecs", "guest", "vsync"
    pub tag: String,
    pub fields: String, // pipe-delimited k=v pairs for LLM parsing
}

#[derive(Clone, Debug)]
pub struct GuestSpan {
    pub name: String,
    pub start_us: u32,
    pub duration_us: u32,
    pub frame_index: u64,
}

pub struct FlightRecorder {
    pub ring: VecDeque<FrameTimings>,
    pub events: VecDeque<TraceEvent>,
    pub current_spans: Vec<GuestSpan>,
    pub open_span_stack: Vec<(String, std::time::Instant)>,
    pub target_frame_us: u32,
    pub ewma_us: f64,
    pub spike_count: u64,
    pub project_name: String,
    pub vsync: bool,
    pub present_mode: String,
    pub adapter_name: String,
    stream_writer: Option<BufWriter<File>>,
}

impl FlightRecorder {
    pub fn new(
        project_name: String,
        target_fps: f64,
        vsync: bool,
        trace_path: Option<&str>,
    ) -> Self {
        let target_frame_us = (1_000_000.0 / target_fps.max(1.0)) as u32;
        let stream_writer = trace_path.and_then(|p| {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(p)
                .ok()
                .map(BufWriter::new)
        });

        let mut rec = Self {
            ring: VecDeque::with_capacity(TRACE_RING_CAP),
            events: VecDeque::with_capacity(512),
            current_spans: Vec::with_capacity(32),
            open_span_stack: Vec::with_capacity(16),
            target_frame_us,
            ewma_us: target_frame_us as f64,
            spike_count: 0,
            project_name,
            vsync,
            present_mode: if vsync { "Fifo".into() } else { "AutoNoVsync".into() },
            adapter_name: "uninitialized".into(),
            stream_writer,
        };
        rec.emit_header();
        rec
    }

    fn emit_header(&mut self) {
        if let Some(ref mut w) = self.stream_writer {
            let _ = writeln!(
                w,
                "META|project:{}|target_us:{}|vsync:{}|mode:{}",
                self.project_name, self.target_frame_us, self.vsync, self.present_mode
            );
            let _ = w.flush();
        }
    }

    pub fn push_event(
        &mut self,
        frame_index: u64,
        time_us_in_frame: u32,
        level: &'static str,
        category: &'static str,
        tag: impl Into<String>,
        fields: impl Into<String>,
    ) {
        let ev = TraceEvent {
            frame_index,
            time_us_in_frame,
            level,
            category,
            tag: tag.into(),
            fields: fields.into(),
        };
        if let Some(ref mut w) = self.stream_writer {
            let _ = writeln!(
                w,
                "{}|f:{}|+{}us|cat:{}|{}|{}",
                ev.level, ev.frame_index, ev.time_us_in_frame, ev.category, ev.tag, ev.fields
            );
        }
        if self.events.len() >= 512 {
            self.events.pop_front();
        }
        self.events.push_back(ev);
    }

    pub fn record_frame(&mut self, f: FrameTimings) -> bool {
        // Warmup first 15 frames before updating EWMA baseline
        let spike_threshold = ((self.target_frame_us as f64) * 1.35).max(self.ewma_us * 1.6);
        let is_spike = f.frame_index > 15 && (f.total_us as f64) > spike_threshold;

        if f.frame_index > 5 {
            self.ewma_us = self.ewma_us * 0.92 + (f.total_us as f64) * 0.08;
        }

        if is_spike {
            self.spike_count += 1;
            let dominant = dominant_phase(&f);
            self.push_event(
                f.frame_index,
                f.total_us,
                "SPIKE",
                dominant.0,
                "frame_jitter_spike",
                format!(
                    "total_us:{}|target_us:{}|ewma_us:{:.0}|dominant:{}|guest_us:{}|abi_us:{}|upload_us:{}|encode_us:{}|present_us:{}|pump_us:{}|abi_calls:{}|reallocs:{}|mesh_uploads:{}",
                    f.total_us,
                    self.target_frame_us,
                    self.ewma_us,
                    dominant.1,
                    f.guest_us,
                    f.abi_us,
                    f.gpu_upload_us,
                    f.gpu_encode_us,
                    f.present_wait_us,
                    f.pump_us,
                    f.abi_calls,
                    f.gpu_buffer_reallocs,
                    f.mesh_uploads,
                ),
            );
        }

        // Structural pathology detectors (independent of frame duration)
        if f.frame_index > 1 && f.mesh_uploads > 0 {
            self.push_event(
                f.frame_index,
                0,
                "WARN",
                "gpu",
                "per_frame_mesh_upload",
                format!("count:{}|hint:move load_cube_mesh/upload_mesh out of frame loop", f.mesh_uploads),
            );
        }
        if f.abi_calls > 40 {
            self.push_event(
                f.frame_index,
                0,
                "WARN",
                "abi",
                "high_abi_chatter",
                format!("calls:{}|abi_us:{}|hint:batch input or instance uploads", f.abi_calls, f.abi_us),
            );
        }
        if f.surface_reconfigs > 0 {
            self.push_event(
                f.frame_index,
                0,
                "WARN",
                "vsync",
                "swapchain_reconfigure",
                format!("count:{}", f.surface_reconfigs),
            );
        }

        if let Some(ref mut w) = self.stream_writer {
            if is_spike || f.frame_index % 60 == 0 {
                let _ = writeln!(
                    w,
                    "FRAME|f:{}|tot:{}|guest:{}|abi:{}({})|up:{}|enc:{}|pres:{}|pump:{}|inst:{}|dc:{}|arena:{}B",
                    f.frame_index, f.total_us, f.guest_us, f.abi_us, f.abi_calls,
                    f.gpu_upload_us, f.gpu_encode_us, f.present_wait_us, f.pump_us,
                    f.instances_3d, f.draw_calls, f.arena_peak_bytes
                );
                let _ = w.flush();
            }
        }

        if self.ring.len() >= TRACE_RING_CAP {
            self.ring.pop_front();
        }
        self.ring.push_back(f);
        self.current_spans.clear();
        is_spike
    }

    /// Exports a compact, 100%-accurate LLM diagnostic report (~300-600 tokens).
    pub fn export_llm_report(&self, path: &str) -> std::io::Result<()> {
        let mut out = BufWriter::new(File::create(path)?);
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        writeln!(out, "# GROOT FLIGHT RECORDER REPORT (ts={ts})")?;
        writeln!(
            out,
            "ENV|project:{}|adapter:{}|vsync:{}|present_mode:{}|target_us:{}|frames_captured:{}|spikes:{}",
            self.project_name, self.adapter_name, self.vsync, self.present_mode,
            self.target_frame_us, self.ring.len(), self.spike_count
        )?;

        if !self.ring.is_empty() {
            let mut totals: Vec<u32> = self.ring.iter().map(|f| f.total_us).collect();
            let mut guests: Vec<u32> = self.ring.iter().map(|f| f.guest_us).collect();
            let mut abis: Vec<u32> = self.ring.iter().map(|f| f.abi_us).collect();
            let mut uploads: Vec<u32> = self.ring.iter().map(|f| f.gpu_upload_us).collect();
            let mut presents: Vec<u32> = self.ring.iter().map(|f| f.present_wait_us).collect();
            totals.sort_unstable();
            guests.sort_unstable();
            abis.sort_unstable();
            uploads.sort_unstable();
            presents.sort_unstable();

            let pct = |v: &[u32], p: usize| v[(v.len() - 1) * p / 100];
            writeln!(
                out,
                "STATS_US|total:p50={},p95={},p99={},max={}|guest:p50={},max={}|abi:p50={},max={}|gpu_up:p50={},max={}|present_wait:p50={},max={}",
                pct(&totals, 50), pct(&totals, 95), pct(&totals, 99), *totals.last().unwrap(),
                pct(&guests, 50), *guests.last().unwrap(),
                pct(&abis, 50), *abis.last().unwrap(),
                pct(&uploads, 50), *uploads.last().unwrap(),
                pct(&presents, 50), *presents.last().unwrap(),
            )?;
        }

        writeln!(out, "\n# ANOMALIES & EVENTS (newest last)")?;
        for ev in self.events.iter().rev().take(40).collect::<Vec<_>>().into_iter().rev() {
            writeln!(
                out,
                "{}|f:{}|+{}us|cat:{}|{}|{}",
                ev.level, ev.frame_index, ev.time_us_in_frame, ev.category, ev.tag, ev.fields
            )?;
        }

        writeln!(out, "\n# RECENT SPIKE FRAMES (±1 context)")?;
        let slice: Vec<&FrameTimings> = self.ring.iter().collect();
        let threshold = ((self.target_frame_us as f64) * 1.35).max(self.ewma_us * 1.6);
        let mut printed = vec![false; slice.len()];
        for (i, f) in slice.iter().enumerate() {
            if f.frame_index > 15 && (f.total_us as f64) > threshold {
                for j in i.saturating_sub(1)..=(i + 1).min(slice.len() - 1) {
                    printed[j] = true;
                }
            }
        }
        for (i, f) in slice.iter().enumerate() {
            if printed[i] {
                writeln!(
                    out,
                    "F#{}|tot:{}us|dt:{:.2}ms|guest:{}|abi:{}({}c)|up:{}|enc:{}|pres:{}|pump:{}|inst:{}|dc:{}|realloc:{}|mesh_up:{}",
                    f.frame_index, f.total_us, f.dt_ms, f.guest_us, f.abi_us, f.abi_calls,
                    f.gpu_upload_us, f.gpu_encode_us, f.present_wait_us, f.pump_us,
                    f.instances_3d, f.draw_calls, f.gpu_buffer_reallocs, f.mesh_uploads
                )?;
            }
        }
        out.flush()
    }
}

fn dominant_phase(f: &FrameTimings) -> (&'static str, &'static str) {
    let phases = [
        ("guest", "guest_us", f.guest_us),
        ("abi", "abi_us", f.abi_us),
        ("gpu", "gpu_upload_us", f.gpu_upload_us),
        ("gpu", "gpu_encode_us", f.gpu_encode_us),
        ("vsync", "present_wait_us", f.present_wait_us),
        ("window", "pump_us", f.pump_us),
    ];
    let best = phases.into_iter().max_by_key(|p| p.2).unwrap();
    (best.0, best.1)
}
