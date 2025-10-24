use anyhow::{Result, anyhow};
use byteorder::{LittleEndian, ReadBytesExt};
use std::{
    fs::OpenOptions,
    io::{BufReader, Cursor, Read, Write},
    process::{Command, Stdio},
    str::FromStr,
};

#[derive(Debug)]
#[allow(dead_code)]
struct PcapRecordHeader {
    ts_sec: u32,
    ts_usec: u32,
    incl_len: u32,
    orig_len: u32,
}

impl PcapRecordHeader {
    fn parse(data: &[u8]) -> Option<(Self, usize)> {
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

#[derive(Debug)]
#[allow(dead_code)]
struct UsbPcapHeader {
    header_len: u16,
    irp_id: u64,
    status: u32,
    function: u16,
    info: u8,
    bus_id: u16,
    device_address: u16,
    endpoint: u8,
    direction_in: bool,
    transfer_type: u8,
    data_length: u32,
}

impl UsbPcapHeader {
    fn parse(data: &[u8]) -> Option<(Self, usize)> {
        if data.len() < 27 {
            return None;
        }
        let mut cur = std::io::Cursor::new(data);

        let header_len = cur.read_u16::<LittleEndian>().ok()?;
        let irp_id = cur.read_u64::<LittleEndian>().ok()?;
        let status = cur.read_u32::<LittleEndian>().ok()?;
        let function = cur.read_u16::<LittleEndian>().ok()?;
        let info = cur.read_u8().ok()?;
        let bus_id = cur.read_u16::<LittleEndian>().ok()?;
        let device_address = cur.read_u16::<LittleEndian>().ok()?;
        let raw_endpoint = cur.read_u8().ok()?;
        let transfer_type = cur.read_u8().ok()?;
        let data_length = cur.read_u32::<LittleEndian>().ok()?;

        let direction_in = (raw_endpoint & 0x80) != 0;
        let endpoint_number = raw_endpoint & 0x7F;
        Some((
            UsbPcapHeader {
                header_len,
                irp_id,
                status,
                function,
                info,
                bus_id,
                device_address,
                endpoint: endpoint_number,
                direction_in,
                transfer_type,
                data_length,
            },
            27,
        ))
    }

    fn is_in_direction(&self) -> bool {
        self.direction_in
    }
}

#[derive(Debug, Clone, Copy)]
struct TargetDevice {
    bus_id: u16,
    device_address: u16,
    endpoint: u8,
}

fn parse_target_device(arg: &str) -> Result<TargetDevice> {
    let parts: Vec<&str> = arg.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!("Invalid device format: {}, expected bus.device.endpoint", arg));
    }
    Ok(TargetDevice {
        bus_id: u16::from_str(parts[0])?,
        device_address: u16::from_str(parts[1])?,
        endpoint: u8::from_str(parts[2])?,
    })
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct MouseMoveEvent {
    dx: i16,
    dy: i16,
    time: f64,
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut target_device: Option<TargetDevice> = None;
    let mut mouse_move_events: Vec<MouseMoveEvent> = Vec::new();
    let mut output_file = OpenOptions::new().append(true).create(true).open("output.csv")?;
    let _ = writeln!(output_file, "xCount,yCount,Time (s)");
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-d" && i + 1 < args.len() {
            target_device = Some(parse_target_device(&args[i + 1])?);
            i += 1;
        }
        i += 1;
    }

    println!("Filtering for target device: {:?}", target_device);
    println!("Starting USBPcapCMD for device {}", r"\\.\USBPcap1");
    let mut child = Command::new(r"C:\Program Files\USBPcap\USBPcapCMD.exe")
        .args(&["-d", r"\\.\USBPcap1", "-o", "-", "-A", "-s", "65535", "-b", "262144"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to start USBPcapCMD: {}", e))?;
    let stdout = child.stdout.take().ok_or(anyhow!("Failed to capture stdout"))?;
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::<u8>::with_capacity(262144);
    let mut temp = vec![0u8; 65535];
    let mut first_target_ts: Option<f64> = None;
    let mut skipped_global = false;
    println!("Reading USB data from pipe...");
    loop {
        let n = reader.read(&mut temp)?;
        if n == 0 {
            println!("USBPcapCMD closed pipe, exiting...");
            break;
        }
        buffer.extend_from_slice(&temp[..n]);
        let mut offset = 0;
        if !skipped_global {
            if buffer.len() < 24 {
                continue;
            }
            println!("Skipped 24-byte global header (PCAP)");
            buffer.drain(0..24);
            skipped_global = true;
        }
        while offset + 16 <= buffer.len() {
            let Some((rec_hdr, rec_size)) = PcapRecordHeader::parse(&buffer[offset..]) else {
                break;
            };
            let total_needed = offset + rec_size + rec_hdr.incl_len as usize;
            if buffer.len() < total_needed {
                break;
            }
            let record_data = &buffer[offset + rec_size..offset + rec_size + rec_hdr.incl_len as usize];
            if let Some((usb_hdr, usb_size)) = UsbPcapHeader::parse(record_data) {
                let payload = &record_data[usb_size..];

                if usb_hdr.is_in_direction() && usb_hdr.data_length == 8 && payload.len() >= 8 {
                    match target_device {
                        Some(target_device) => {
                            if target_device.bus_id == usb_hdr.bus_id && usb_hdr.device_address == target_device.device_address && usb_hdr.endpoint == target_device.endpoint {
                                let ts = rec_hdr.ts_sec as f64 + rec_hdr.ts_usec as f64 / 1_000_000.0;
                                let delta = if let Some(start) = first_target_ts {
                                    ts - start
                                } else {
                                    first_target_ts = Some(ts);
                                    0.0
                                };
                                let dx = i16::from_le_bytes(payload[2..4].try_into().unwrap());
                                let dy = i16::from_le_bytes(payload[4..6].try_into().unwrap());
                                mouse_move_events.push(MouseMoveEvent { dx: dx, dy: dy, time: delta });
                            }
                        }
                        None => {
                            let dx = i16::from_le_bytes(payload[2..4].try_into().unwrap());
                            let dy = i16::from_le_bytes(payload[4..6].try_into().unwrap());
                            println!("?Mouse Move: dx={:<4} dy={:<4} raw={:02X?}", dx, dy, payload);
                        }
                    }
                }
            }
            offset = total_needed;
        }
        if offset > 0 {
            buffer.drain(0..offset);
        }
    }
    child.kill().ok();
    child.wait().ok();
    Ok(())
}
