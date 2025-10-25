use anyhow::{Result, anyhow};
use byteorder::{LittleEndian, ReadBytesExt};
use plotters::prelude::*;
use std::{
    env,
    fs::OpenOptions,
    io::{BufReader, Cursor, Read, Write},
    process::{Command, Stdio},
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
    },
    thread,
    time::Duration,
};

mod gui;

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
        let mut cur = Cursor::new(data);

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
pub struct MouseMoveEvent {
    pub dx: i16,
    pub dy: i16,
    pub time: f64,
}

#[cfg(windows)]
pub mod key_utils {
    // Win32 GetAsyncKeyState via FFI
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetAsyncKeyState(vKey: i32) -> i16;
    }

    pub const VK_F2: i32 = 0x71;

    pub fn is_f2_pressed() -> bool {
        // Cast to u16 so mask literal fits correctly
        unsafe { ((GetAsyncKeyState(VK_F2) as u16) & 0x8000u16) != 0u16 }
    }
}

#[cfg(not(windows))]
pub mod key_utils {
    // fallback: no OS-level F2 detection available here
    pub fn is_f2_pressed() -> bool {
        false
    }
}

/// Create a PNG plot of dx(t) and -dy(t) using plotters.
///
/// `path` - output PNG path
/// `times` - X values (seconds)
/// `dx` - dx values
/// `ndy` - -dy values
fn plot_to_png(path: &str, times: &[f64], dx: &[f64], ndy: &[f64]) -> Result<()> {
    let width = 3840u32;
    let height = 2160u32;
    let root = BitMapBackend::new(path, (width, height)).into_drawing_area();
    root.fill(&WHITE)?;

    // X range
    let t_min = times.first().copied().unwrap_or(0.0);
    let t_max = times.last().copied().unwrap_or(1.0);
    let t_span = (t_max - t_min).abs().max(1e-6);
    let x_range = (t_min - 0.02 * t_span)..(t_max + 0.02 * t_span);

    // Y range across both series
    let v_min = dx.iter().chain(ndy.iter()).copied().fold(f64::INFINITY, f64::min);
    let v_max = dx.iter().chain(ndy.iter()).copied().fold(f64::NEG_INFINITY, f64::max);
    let v_span = (v_max - v_min).abs().max(1e-6);
    let y_range = (v_min - 0.1 * v_span)..(v_max + 0.1 * v_span);

    let mut chart = ChartBuilder::on(&root)
        .caption("dx and -dy vs time", ("sans-serif", 24).into_font())
        .margin(10)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(x_range, y_range)?;

    chart.configure_mesh().x_desc("time (s)").y_desc("value").draw()?;

    chart.draw_series(LineSeries::new(times.iter().copied().zip(dx.iter().copied()), &RED))?.label("dx");

    chart.draw_series(LineSeries::new(times.iter().copied().zip(ndy.iter().copied()), &BLUE))?.label("-dy");

    // draw points optionally
    chart.draw_series(times.iter().copied().zip(dx.iter().copied()).map(|(t, v)| Circle::new((t, v), 2, RED.filled())))?;
    chart.draw_series(times.iter().copied().zip(ndy.iter().copied()).map(|(t, v)| Circle::new((t, v), 2, BLUE.filled())))?;

    chart.configure_series_labels().background_style(&WHITE.mix(0.8)).border_style(&BLACK).draw()?;

    root.present()?;
    Ok(())
}

fn analyze_and_write_csv_and_plot(events: &[MouseMoveEvent]) -> Result<()> {
    if events.is_empty() {
        println!("No MouseMoveEvents recorded.");
        return Ok(());
    }

    let count = events.len();
    let time_start = events.iter().map(|e| e.time).fold(f64::INFINITY, |a, b| a.min(b));
    let time_end = events.iter().map(|e| e.time).fold(f64::NEG_INFINITY, |a, b| a.max(b));
    let duration = (time_end - time_start).max(0.0);

    let total_dx: i64 = events.iter().map(|e| e.dx as i64).sum();
    let total_dy: i64 = events.iter().map(|e| e.dy as i64).sum();

    let mut total_distance = 0f64;
    let mut magnitudes: Vec<f64> = Vec::with_capacity(events.len());
    for e in events {
        let mag = ((e.dx as f64).powi(2) + (e.dy as f64).powi(2)).sqrt();
        magnitudes.push(mag);
        total_distance += mag;
    }

    let avg_distance_per_event = total_distance / (count as f64);
    let avg_speed = if duration > 0.0 { total_distance / duration } else { 0.0 };

    println!("--- MouseMoveEvents Analysis ---");
    println!("Events: {}", count);
    println!("Duration (s): {:.6}", duration.max(0.0));
    println!("Sum dx: {}, Sum dy: {}", total_dx, total_dy);
    println!("Total distance (sum of step lengths): {:.3}", total_distance);
    println!("Avg per event: dx={:.3}, dy={:.3}", total_dx as f64 / count as f64, total_dy as f64 / count as f64);
    if duration > 0.0 {
        println!("Events/sec: {:.3}", count as f64 / duration);
        println!("Average speed (units/sec): {:.3}", total_distance / duration);
    }

    // Histogram
    let max_mag = magnitudes.iter().copied().fold(0.0f64, |a, b| a.max(b));
    let bucket_count = 12usize;
    let mut buckets = vec![0usize; bucket_count];
    let bucket_size = if max_mag <= 0.0 { 1.0 } else { max_mag / (bucket_count as f64) };

    for &m in &magnitudes {
        let idx = if bucket_size == 0.0 {
            0
        } else {
            let v = (m / bucket_size).floor() as isize;
            let v = v.max(0).min((bucket_count - 1) as isize);
            v as usize
        };
        buckets[idx] += 1;
    }

    println!("\nMovement magnitude histogram (each bucket range shown):");
    let max_bucket = *buckets.iter().max().unwrap_or(&0);
    for (i, &c) in buckets.iter().enumerate() {
        let range_start = bucket_size * (i as f64);
        let range_end = bucket_size * ((i + 1) as f64);
        let bar_len = if max_bucket > 0 { ((c as f64 / max_bucket as f64) * 40.0).round() as usize } else { 0 };
        let bar = std::iter::repeat('#').take(bar_len).collect::<String>();
        println!("  [{:6.3} - {:6.3}) : {:5} {}", range_start, range_end, c, bar);
    }

    // Write CSV file summary + events
    let mut f = OpenOptions::new().write(true).truncate(true).create(true).open("output.csv")?;
    writeln!(f, "dx,dy,time")?;
    for e in events {
        writeln!(f, "{},{},{:.6}", e.dx, e.dy, e.time)?;
    }
    writeln!(f, "\n# Summary")?;
    writeln!(f, "# Count,{},TimeSpan(s),{:.6}", count, duration)?;
    writeln!(f, "# TotalDistance,{:.6}", total_distance)?;
    writeln!(f, "# AvgDistancePerEvent,{:.6}", avg_distance_per_event)?;
    writeln!(f, "# AvgSpeed(units/s),{:.6}", avg_speed)?;

    println!("\nWrote detailed events + summary to output.csv");

    // Prepare PNG plot and open it in the system default viewer
    let times_plot: Vec<f64> = events.iter().map(|e| e.time).collect();
    let dx_plot: Vec<f64> = events.iter().map(|e| e.dx as f64).collect();
    let ndy_plot: Vec<f64> = events.iter().map(|e| -(e.dy as f64)).collect();
    let png_path = "mouse_plot.png";
    plot_to_png(png_path, &times_plot, &dx_plot, &ndy_plot)?;

    // try to open the PNG with platform default
    #[cfg(target_os = "windows")]
    {
        // Use start via cmd (start requires a title arg; provide empty title)
        let _ = Command::new("cmd").args(&["/C", "start", "", png_path]).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("xdg-open").arg(png_path).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }

    Ok(())
}

fn run_capture(
    events_arc: Arc<Mutex<Vec<MouseMoveEvent>>>,
    stop_flag: Arc<AtomicBool>,
    target_device: Option<TargetDevice>,
) -> Result<()> {
    println!("Filtering for target device: {:?}", target_device);
    println!("Starting USBPcapCMD for device {}", r"\\.\USBPcap1");

    // Start usbpcap child
    let mut child = Command::new(r"C:\Program Files\USBPcap\USBPcapCMD.exe")
        .args(&["-d", r"\\.\USBPcap1", "-o", "-", "-A", "-s", "65535", "-b", "262144"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to start USBPcapCMD: {}", e))?;

    let child_pid = child.id().to_string();

    // Keyboard watcher thread (Windows: GetAsyncKeyState). On F2 it will try to stop the capture.
    {
        let stop_flag = Arc::clone(&stop_flag);
        let _pid = child_pid.clone();
        thread::spawn(move || {
            loop {
                if stop_flag.load(AtomicOrdering::SeqCst) {
                    break;
                }
                #[cfg(windows)]
                {
                    if key_utils::is_f2_pressed() {
                        println!("F2 pressed: requesting stop...");
                        stop_flag.store(true, AtomicOrdering::SeqCst);
                        // try to stop the child so pipe unblocks
                        let _ = Command::new("taskkill").args(&["/PID", &_pid, "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(80));
            }
        });
    }

    // reader loop, collect events
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to capture stdout"))?;
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::<u8>::with_capacity(262144);
    let mut temp = vec![0u8; 65535];
    let mut first_target_ts: Option<f64> = None;
    let mut skipped_global = false;

    println!("Reading USB data from pipe... (press F2 to stop capture and analyze)");

    loop {
        if stop_flag.load(AtomicOrdering::SeqCst) {
            println!("Stop requested, breaking read loop...");
            break;
        }

        let n = match reader.read(&mut temp) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error reading from pipe: {}", e);
                break;
            }
        };
        if n == 0 {
            println!("USBPcapCMD closed pipe, exiting read loop...");
            break;
        }
        buffer.extend_from_slice(&temp[..n]);
        let mut offset: usize = 0;

        if !skipped_global {
            if buffer.len() < 24 {
                // wait until we have global header
            } else {
                // drop the global pcap header
                buffer.drain(0..24);
                skipped_global = true;
            }
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
                    if let Some(td) = target_device {
                        if td.bus_id == usb_hdr.bus_id && td.device_address == usb_hdr.device_address && td.endpoint == usb_hdr.endpoint {
                            let ts = rec_hdr.ts_sec as f64 + rec_hdr.ts_usec as f64 / 1_000_000.0;
                            let delta = if let Some(start) = first_target_ts {
                                ts - start
                            } else {
                                first_target_ts = Some(ts);
                                0.0
                            };
                            let dx = i16::from_le_bytes(payload[2..4].try_into().unwrap());
                            let dy = i16::from_le_bytes(payload[4..6].try_into().unwrap());
                            let mut events = events_arc.lock().unwrap();
                            events.push(MouseMoveEvent { dx, dy, time: delta });
                        }
                    } else {
                        // no target specified, just print sample debug
                        let dx = i16::from_le_bytes(payload[2..4].try_into().unwrap());
                        let dy = i16::from_le_bytes(payload[4..6].try_into().unwrap());
                        println!("?Mouse Move: dx={:<4} dy={:<4} raw={:02X?}", dx, dy, payload);
                    }
                }
            }
            offset = total_needed;
        }

        if offset > 0 {
            buffer.drain(0..offset);
        }
    }

    // ensure child stopped
    child.kill().ok();
    child.wait().ok();

    Ok(())
}

fn main() -> Result<()> {
    // Shared event storage and stop flag
    let events_arc: Arc<Mutex<Vec<MouseMoveEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let stop_flag = Arc::new(AtomicBool::new(false));

    // parse args for optional target device and gui flag
    let args: Vec<String> = env::args().collect();
    let mut target_device: Option<TargetDevice> = None;
    let mut use_gui = false;
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "-d" && i + 1 < args.len() {
            target_device = Some(parse_target_device(&args[i + 1])?);
            i += 1;
        } else if args[i] == "--gui" || args[i] == "-g" {
            use_gui = true;
        }
        i += 1;
    }

    if use_gui {
        // GUI mode: run capture in background thread, GUI on main thread
        let events_capture = Arc::clone(&events_arc);
        let stop_capture = Arc::clone(&stop_flag);
        thread::spawn(move || {
            if let Err(e) = run_capture(events_capture, stop_capture, target_device) {
                eprintln!("Capture error: {}", e);
            }
        });
        
        // Run GUI on main thread (required by eframe)
        let stop_gui = Arc::clone(&stop_flag);
        if let Err(e) = gui::run_gui(events_arc, stop_gui) {
            eprintln!("GUI error: {}", e);
            return Err(anyhow!("GUI failed: {}", e));
        }
    } else {
        // CLI mode: run capture on main thread
        run_capture(Arc::clone(&events_arc), Arc::clone(&stop_flag), target_device)?;
        
        // extract events for analysis and plotting
        let events = events_arc.lock().unwrap().clone();

        // write CSV & print analysis, create PNG plot and open it
        analyze_and_write_csv_and_plot(&events)?;
    }

    Ok(())
}
