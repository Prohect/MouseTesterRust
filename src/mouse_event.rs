use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

#[derive(Debug, Clone, Copy)]
pub struct PcapRecordHeader {
    pub ts_sec: u32,
    pub ts_usec: u32,
    pub incl_len: u32,
    pub orig_len: u32,
}

impl PcapRecordHeader {
    /// Parse a PcapRecordHeader from raw bytes
    pub fn parse(data: &[u8]) -> Option<(Self, usize)> {
        if data.len() < 16 {
            return None;
        }
        let mut cur = Cursor::new(data);
        Some((
            PcapRecordHeader {
                ts_sec: cur.read_u32::<LittleEndian>().ok()?,
                ts_usec: cur.read_u32::<LittleEndian>().ok()?,
                incl_len: cur.read_u32::<LittleEndian>().ok()?,
                orig_len: cur.read_u32::<LittleEndian>().ok()?,
            },
            16,
        ))
    }
}

/// MouseMoveEvent represents a mouse movement with precise pcap timestamps
///
/// This struct uses separate timestamp fields (ts_sec and ts_usec) for precise
/// time representation, compatible with pcap capture data.
#[derive(Debug, Clone, Copy)]
pub struct MouseMoveEvent {
    pub dx: i16,
    pub dy: i16,
    pub ts_sec: u32,
    pub ts_usec: u32,
    pub has_report_id: bool,
    pub report_id: u8,
    pub buttons_state: [bool; 5],
    pub wheel: i8,
    pub pan: i8,
}

impl MouseMoveEvent {
    /// Create a new MouseMoveEvent with explicit timestamp fields
    pub fn new(dx: i16, dy: i16, ts_sec: u32, ts_usec: u32, has_report_id: bool, report_id: u8, buttons_state: [bool; 5], wheel: i8, pan: i8) -> Self {
        Self {
            dx,
            dy,
            ts_sec,
            ts_usec,
            has_report_id,
            report_id,
            buttons_state,
            wheel,
            pan,
        }
    }
    pub fn alter_time(event: MouseMoveEvent, ts_sec: u32, ts_usec: u32) -> Self {
        let dx = event.dx;
        let dy = event.dy;
        let has_report_id = event.has_report_id;
        let report_id = event.report_id;
        let buttons_state = event.buttons_state;
        let wheel = event.wheel;
        let pan = event.pan;
        Self {
            dx,
            dy,
            ts_sec,
            ts_usec,
            has_report_id,
            report_id,
            buttons_state,
            wheel,
            pan,
        }
    }

    /// Create a MouseMoveEvent from dx, dy, and a PcapRecordHeader
    pub fn from_pcap_header(event: MouseMoveEvent, rec: &PcapRecordHeader) -> Self {
        let dx = event.dx;
        let dy = event.dy;
        let has_report_id = event.has_report_id;
        let report_id = event.report_id;
        let buttons_state = event.buttons_state;
        let wheel = event.wheel;
        let pan = event.pan;
        Self {
            dx,
            dy,
            ts_sec: rec.ts_sec,
            ts_usec: rec.ts_usec,
            has_report_id,
            report_id,
            buttons_state,
            wheel,
            pan,
        }
    }

    /// Get the time in seconds as a floating-point value
    pub fn time_secs(&self) -> f64 {
        self.ts_sec as f64 + self.ts_usec as f64 / 1_000_000.0
    }

    /// Get the time in microseconds as a 64-bit integer
    pub fn time_micros(&self) -> u64 {
        (self.ts_sec as u64) * 1_000_000 + (self.ts_usec as u64)
    }
}

/// Parser module for extracting mouse movement data from USB HID reports
pub mod parser {
    use super::{MouseMoveEvent, PcapRecordHeader};

    /// Parse a mouse movement from an 8-byte USB HID report with Report ID
    ///
    /// Expected format (8 bytes):
    /// - byte 0: Report ID
    /// - byte 1: Buttons
    /// - bytes 2-3: dx (little-endian i16)
    /// - bytes 4-5: dy (little-endian i16)
    /// - bytes 6-7: Wheel/extra data
    pub fn parse_with_report_id(payload: &[u8], rec: &PcapRecordHeader) -> Option<MouseMoveEvent> {
        if payload.len() < 8 {
            return None;
        }
        let report_id = payload[0];
        let buttons_byte = payload[1];
        let buttons = buttons_byte & 0x1F;
        let buttons_state = [(buttons & 0x01) != 0, (buttons & 0x02) != 0, (buttons & 0x04) != 0, (buttons & 0x08) != 0, (buttons & 0x10) != 0];
        let dx = i16::from_le_bytes([payload[2], payload[3]]);
        let dy = i16::from_le_bytes([payload[4], payload[5]]);
        let wheel = payload[6] as i8;
        let pan = payload[7] as i8;
        Some(MouseMoveEvent {
            dx,
            dy,
            ts_sec: rec.ts_sec,
            ts_usec: rec.ts_usec,
            has_report_id: true,
            report_id,
            buttons_state,
            wheel,
            pan,
        })
    }
    pub fn parse_without_report_id(payload: &[u8], rec: &PcapRecordHeader) -> Option<MouseMoveEvent> {
        if payload.len() < 7 {
            return None;
        }
        let buttons_byte = payload[0];
        let buttons = buttons_byte & 0x1F;
        let buttons_state = [(buttons & 0x01) != 0, (buttons & 0x02) != 0, (buttons & 0x04) != 0, (buttons & 0x08) != 0, (buttons & 0x10) != 0];
        let dx = i16::from_le_bytes([payload[1], payload[2]]);
        let dy = i16::from_le_bytes([payload[3], payload[4]]);
        let wheel = payload[5] as i8;
        let pan = payload[6] as i8;
        Some(MouseMoveEvent {
            dx,
            dy,
            ts_sec: rec.ts_sec,
            ts_usec: rec.ts_usec,
            has_report_id: false,
            report_id: 0u8,
            buttons_state,
            wheel,
            pan,
        })
    }
    pub fn parse_auto(payload: &[u8], rec: &PcapRecordHeader) -> Option<MouseMoveEvent> {
        match payload.len() {
            8 => parse_with_report_id(payload, rec),
            7 => parse_without_report_id(payload, rec),
            _ => None,
        }
    }
}
