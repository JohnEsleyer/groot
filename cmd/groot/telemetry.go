// File: cmd/groot/telemetry.go
// Client-side telemetry reader — connects to a Groot host IPC socket and
// decodes binary TelemetryPacket frames.
package main

import (
	"encoding/binary"
	"fmt"
	"io"
	"math"
	"net"
)

// TelemetryPacket is the Go mirror of the Rust `TelemetryPacket` repr(C) struct
// emitted by the Groot host runner on every call to present().
//
// Field layout (little-endian, packed, 64 bytes total):
//
//	frame_index          u64   8 bytes  offset  0
//	frame_time_us        u32   4 bytes  offset  8
//	fps                  f32   4 bytes  offset 12
//	arena_peak_bytes     u32   4 bytes  offset 16
//	arena_reclaimed      u32   4 bytes  offset 20
//	arena_promoted       u32   4 bytes  offset 24
//	runaway_warnings     u32   4 bytes  offset 28
//	entities_alive       u32   4 bytes  offset 32
//	entities_capacity    u32   4 bytes  offset 36
//	handle_recycles      u32   4 bytes  offset 40
//	stale_handle_hits    u32   4 bytes  offset 44
//	quads_rendered       u32   4 bytes  offset 48
//	triangles_rendered   u32   4 bytes  offset 52
//	instances_3d         u32   4 bytes  offset 56
//	vertex_buffer_bytes  u32   4 bytes  offset 60
//	draw_calls           u16   2 bytes  offset 64
//	mouse_x              f32   4 bytes  offset 66
//	mouse_y              f32   4 bytes  offset 70
//	mouse_buttons_mask   u8    1 byte   offset 74
//	pressed_keys_count   u8    1 byte   offset 75
//	_pad                 u16   2 bytes  offset 76  (align to 8)
//	Total                      80 bytes
const TelemetryPacketSize = 80

type TelemetryPacket struct {
	FrameIndex        uint64
	FrameTimeUs       uint32
	FPS               float32
	ArenaPeakBytes    uint32
	ArenaReclaimed    uint32
	ArenaPromoted     uint32
	RunawayWarnings   uint32
	EntitiesAlive     uint32
	EntitiesCapacity  uint32
	HandleRecycles    uint32
	StaleHandleHits   uint32
	QuadsRendered     uint32
	TrianglesRendered uint32
	Instances3D       uint32
	VertexBufferBytes uint32
	DrawCalls         uint16
	MouseX            float32
	MouseY            float32
	MouseButtonsMask  uint8
	PressedKeysCount  uint8
}

// DecodeTelemetryPacket decodes exactly TelemetryPacketSize bytes into a
// TelemetryPacket. All multi-byte fields are little-endian.
func DecodeTelemetryPacket(buf []byte) (*TelemetryPacket, error) {
	if len(buf) < TelemetryPacketSize {
		return nil, fmt.Errorf("telemetry packet too short: %d bytes (want %d)", len(buf), TelemetryPacketSize)
	}
	p := &TelemetryPacket{}
	p.FrameIndex = binary.LittleEndian.Uint64(buf[0:8])
	p.FrameTimeUs = binary.LittleEndian.Uint32(buf[8:12])
	p.FPS = math.Float32frombits(binary.LittleEndian.Uint32(buf[12:16]))
	p.ArenaPeakBytes = binary.LittleEndian.Uint32(buf[16:20])
	p.ArenaReclaimed = binary.LittleEndian.Uint32(buf[20:24])
	p.ArenaPromoted = binary.LittleEndian.Uint32(buf[24:28])
	p.RunawayWarnings = binary.LittleEndian.Uint32(buf[28:32])
	p.EntitiesAlive = binary.LittleEndian.Uint32(buf[32:36])
	p.EntitiesCapacity = binary.LittleEndian.Uint32(buf[36:40])
	p.HandleRecycles = binary.LittleEndian.Uint32(buf[40:44])
	p.StaleHandleHits = binary.LittleEndian.Uint32(buf[44:48])
	p.QuadsRendered = binary.LittleEndian.Uint32(buf[48:52])
	p.TrianglesRendered = binary.LittleEndian.Uint32(buf[52:56])
	p.Instances3D = binary.LittleEndian.Uint32(buf[56:60])
	p.VertexBufferBytes = binary.LittleEndian.Uint32(buf[60:64])
	p.DrawCalls = binary.LittleEndian.Uint16(buf[64:66])
	p.MouseX = math.Float32frombits(binary.LittleEndian.Uint32(buf[66:70]))
	p.MouseY = math.Float32frombits(binary.LittleEndian.Uint32(buf[70:74]))
	p.MouseButtonsMask = buf[74]
	p.PressedKeysCount = buf[75]
	return p, nil
}

// IPCReader wraps a UNIX domain socket connection and provides a streaming
// decoder for TelemetryPacket frames.
type IPCReader struct {
	conn net.Conn
}

func connectToSocket(socketPath string) (*IPCReader, error) {
	conn, err := net.Dial("unix", socketPath)
	if err != nil {
		return nil, fmt.Errorf("connecting to %s: %w", socketPath, err)
	}
	return &IPCReader{conn: conn}, nil
}

func (r *IPCReader) Close() error {
	return r.conn.Close()
}

// ReadLoop calls handler for every decoded TelemetryPacket until the
// connection is closed or an error occurs.
func (r *IPCReader) ReadLoop(handler func(*TelemetryPacket)) error {
	buf := make([]byte, TelemetryPacketSize)
	for {
		_, err := io.ReadFull(r.conn, buf)
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}
		p, err := DecodeTelemetryPacket(buf)
		if err != nil {
			continue // skip malformed packets
		}
		handler(p)
	}
}
