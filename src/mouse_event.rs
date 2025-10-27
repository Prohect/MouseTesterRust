//! Mouse event module with pcap timestamp-based MouseMoveEvent struct
//!
//! This module provides a MouseMoveEvent struct that uses pcap timestamp fields
//! (ts_sec and ts_usec) instead of a floating-point time field. This allows for
//! more precise timestamp handling when processing USB mouse data from pcap captures.

use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

/// PcapRecordHeader represents the packet capture record header
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
}

impl MouseMoveEvent {
    /// Create a new MouseMoveEvent with explicit timestamp fields
    pub fn new(dx: i16, dy: i16, ts_sec: u32, ts_usec: u32) -> Self {
        Self { dx, dy, ts_sec, ts_usec }
    }

    /// Create a MouseMoveEvent from dx, dy, and a PcapRecordHeader
    pub fn from_pcap_header(dx: i16, dy: i16, rec: &PcapRecordHeader) -> Self {
        Self {
            dx,
            dy,
            ts_sec: rec.ts_sec,
            ts_usec: rec.ts_usec,
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

        let dx = i16::from_le_bytes([payload[2], payload[3]]);
        let dy = i16::from_le_bytes([payload[4], payload[5]]);

        Some(MouseMoveEvent::from_pcap_header(dx, dy, rec))
    }

    /// Parse a mouse movement from a 7-byte USB HID report without Report ID
    ///
    /// Expected format (7 bytes):
    /// - byte 0: Buttons
    /// - bytes 1-2: dx (little-endian i16)
    /// - bytes 3-4: dy (little-endian i16)
    /// - bytes 5-6: Wheel/extra data
    pub fn parse_without_report_id(payload: &[u8], rec: &PcapRecordHeader) -> Option<MouseMoveEvent> {
        if payload.len() < 7 {
            return None;
        }

        let dx = i16::from_le_bytes([payload[1], payload[2]]);
        let dy = i16::from_le_bytes([payload[3], payload[4]]);

        Some(MouseMoveEvent::from_pcap_header(dx, dy, rec))
    }

    /// Automatically detect and parse mouse movement from USB HID report
    ///
    /// This function tries to automatically detect whether the payload includes
    /// a Report ID by checking the payload length:
    /// - 8 bytes: Assumed to have Report ID (calls parse_with_report_id)
    /// - 7 bytes: Assumed to have no Report ID (calls parse_without_report_id)
    /// - Other lengths: Returns None
    pub fn parse_auto(payload: &[u8], rec: &PcapRecordHeader) -> Option<MouseMoveEvent> {
        match payload.len() {
            8 => parse_with_report_id(payload, rec),
            7 => parse_without_report_id(payload, rec),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_event_new() {
        let event = MouseMoveEvent::new(10, -20, 1234, 567890);
        assert_eq!(event.dx, 10);
        assert_eq!(event.dy, -20);
        assert_eq!(event.ts_sec, 1234);
        assert_eq!(event.ts_usec, 567890);
    }

    #[test]
    fn test_time_secs() {
        let event = MouseMoveEvent::new(0, 0, 1234, 567890);
        let expected = 1234.0 + 567890.0 / 1_000_000.0;
        assert!((event.time_secs() - expected).abs() < 1e-9);
    }

    #[test]
    fn test_time_micros() {
        let event = MouseMoveEvent::new(0, 0, 1234, 567890);
        let expected = 1234u64 * 1_000_000 + 567890;
        assert_eq!(event.time_micros(), expected);
    }

    #[test]
    fn test_from_pcap_header() {
        let rec = PcapRecordHeader {
            ts_sec: 5000,
            ts_usec: 123456,
            incl_len: 8,
            orig_len: 8,
        };
        let event = MouseMoveEvent::from_pcap_header(100, -50, &rec);
        assert_eq!(event.dx, 100);
        assert_eq!(event.dy, -50);
        assert_eq!(event.ts_sec, 5000);
        assert_eq!(event.ts_usec, 123456);
    }

    #[test]
    fn test_parse_with_report_id() {
        let rec = PcapRecordHeader {
            ts_sec: 1000,
            ts_usec: 500000,
            incl_len: 8,
            orig_len: 8,
        };
        // 8-byte payload: [report_id, buttons, dx_low, dx_high, dy_low, dy_high, wheel, extra]
        let payload = [0x01, 0x00, 0x0A, 0x00, 0xF6, 0xFF, 0x00, 0x00];

        let event = parser::parse_with_report_id(&payload, &rec).unwrap();
        assert_eq!(event.dx, 10); // 0x000A
        assert_eq!(event.dy, -10); // 0xFFF6
        assert_eq!(event.ts_sec, 1000);
        assert_eq!(event.ts_usec, 500000);
    }

    #[test]
    fn test_parse_without_report_id() {
        let rec = PcapRecordHeader {
            ts_sec: 2000,
            ts_usec: 750000,
            incl_len: 7,
            orig_len: 7,
        };
        // 7-byte payload: [buttons, dx_low, dx_high, dy_low, dy_high, wheel, extra]
        let payload = [0x00, 0x14, 0x00, 0xEC, 0xFF, 0x00, 0x00];

        let event = parser::parse_without_report_id(&payload, &rec).unwrap();
        assert_eq!(event.dx, 20); // 0x0014
        assert_eq!(event.dy, -20); // 0xFFEC
        assert_eq!(event.ts_sec, 2000);
        assert_eq!(event.ts_usec, 750000);
    }

    #[test]
    fn test_parse_auto() {
        let rec = PcapRecordHeader {
            ts_sec: 3000,
            ts_usec: 250000,
            incl_len: 8,
            orig_len: 8,
        };

        // Test 8-byte payload
        let payload_8 = [0x01, 0x00, 0x05, 0x00, 0xFB, 0xFF, 0x00, 0x00];
        let event = parser::parse_auto(&payload_8, &rec).unwrap();
        assert_eq!(event.dx, 5);
        assert_eq!(event.dy, -5);

        // Test 7-byte payload
        let payload_7 = [0x00, 0x0F, 0x00, 0xF1, 0xFF, 0x00, 0x00];
        let event = parser::parse_auto(&payload_7, &rec).unwrap();
        assert_eq!(event.dx, 15);
        assert_eq!(event.dy, -15);

        // Test invalid length
        let payload_invalid = [0x01, 0x02, 0x03];
        assert!(parser::parse_auto(&payload_invalid, &rec).is_none());
    }

    #[test]
    fn test_pcap_record_header_parse() {
        let data = [
            0x01, 0x00, 0x00, 0x00, // ts_sec = 1
            0x02, 0x00, 0x00, 0x00, // ts_usec = 2
            0x08, 0x00, 0x00, 0x00, // incl_len = 8
            0x08, 0x00, 0x00, 0x00, // orig_len = 8
        ];

        let (header, size) = PcapRecordHeader::parse(&data).unwrap();
        assert_eq!(size, 16);
        assert_eq!(header.ts_sec, 1);
        assert_eq!(header.ts_usec, 2);
        assert_eq!(header.incl_len, 8);
        assert_eq!(header.orig_len, 8);
    }

    #[test]
    fn test_pcap_record_header_parse_insufficient_data() {
        let data = [0x01, 0x02, 0x03]; // Less than 16 bytes
        assert!(PcapRecordHeader::parse(&data).is_none());
    }
}
