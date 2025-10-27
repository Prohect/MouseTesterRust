use crate::mouse_event::MouseMoveEvent;
// Import the new advanced LOD module
use crate::lod_advanced::{build_segments, collect_visible_indices, Segment};
use eframe::egui;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

#[cfg(windows)]
use crate::key_utils;

pub struct MouseAnalyzerGui {
    events: Arc<Mutex<Vec<MouseMoveEvent>>>,
    stop_flag: Arc<AtomicBool>,
    show_plot: bool,
    show_stats: bool,
    show_histogram: bool,
    show_events_table: bool,
    is_capturing: bool,
    captured_events: Vec<MouseMoveEvent>,       // Events snapshot when capture stopped
    last_f2_state: bool,                        // For edge detection
    target_device: Option<crate::TargetDevice>, // Store target device for restarts
    
    // Advanced LOD state
    advanced_lod_segments: Vec<Segment>,
    advanced_lod_last_events_len: usize,
    advanced_lod_last_bounds: Option<PlotBounds>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PlotBounds {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

impl MouseAnalyzerGui {
    pub fn new(events: Arc<Mutex<Vec<MouseMoveEvent>>>, stop_flag: Arc<AtomicBool>, target_device: Option<crate::TargetDevice>) -> Self {
        Self {
            events,
            stop_flag,
            show_plot: true,
            show_stats: true,
            show_histogram: false,
            show_events_table: false,
            is_capturing: true, // Start capturing initially
            captured_events: Vec::new(),
            last_f2_state: false,
            target_device,
            
            // Advanced LOD initialization
            advanced_lod_segments: Vec::new(),
            advanced_lod_last_events_len: 0,
            advanced_lod_last_bounds: None,
        }
    }

    /// Check if plot bounds have changed significantly
    fn bounds_changed_significantly(&self, new_bounds: &PlotBounds) -> bool {
        match self.advanced_lod_last_bounds {
            None => true, // First time, always recalculate
            Some(old_bounds) => {
                let x_range_old = (old_bounds.x_max - old_bounds.x_min).abs();
                let y_range_old = (old_bounds.y_max - old_bounds.y_min).abs();
                let x_range_new = (new_bounds.x_max - new_bounds.x_min).abs();
                let y_range_new = (new_bounds.y_max - new_bounds.y_min).abs();

                // Check if range changed by more than threshold (10%)
                let x_change = ((x_range_new - x_range_old) / x_range_old.max(1e-6)).abs();
                let y_change = ((y_range_new - y_range_old) / y_range_old.max(1e-6)).abs();

                // Also check if center position moved significantly
                let x_center_old = (old_bounds.x_min + old_bounds.x_max) / 2.0;
                let x_center_new = (new_bounds.x_min + new_bounds.x_max) / 2.0;
                let y_center_old = (old_bounds.y_min + old_bounds.y_max) / 2.0;
                let y_center_new = (new_bounds.y_min + new_bounds.y_max) / 2.0;

                let x_center_change = ((x_center_new - x_center_old) / x_range_old.max(1e-6)).abs();
                let y_center_change = ((y_center_new - y_center_old) / y_range_old.max(1e-6)).abs();

                // Trigger if any change exceeds 10% threshold
                let threshold = 0.1;
                x_change > threshold || y_change > threshold || x_center_change > threshold || y_center_change > threshold
            }
        }
    }

    /// NEW ADVANCED LOD: Apply the advanced LOD algorithm with regression-based segmentation
    /// Returns indices into the events slice for rendering
    fn apply_advanced_lod_indices(&mut self, events: &[MouseMoveEvent], visible_width: f64, plot_bounds: Option<&PlotBounds>) -> Vec<usize> {
        if events.is_empty() {
            return Vec::new();
        }

        // Check if we need to rebuild segments (events changed)
        if events.len() != self.advanced_lod_last_events_len {
            println!("Building advanced LOD segments for {} events...", events.len());
            // Build segments with good parameters for real mouse data
            // - initial_size: 15 (start with 15-event segments)
            // - growth_factor: 2.0 (double size when expanding)
            // - min_r_squared: 0.7 (require decent fit quality)
            // - balance_weight: 0.6 (slightly favor length over R-squared)
            self.advanced_lod_segments = build_segments(events, 15, 2.0, 0.7, 0.6);
            self.advanced_lod_last_events_len = events.len();
            println!("Created {} segments", self.advanced_lod_segments.len());
        }

        // Get bounds or use full range
        let (x_min, x_max, y_min, y_max) = if let Some(bounds) = plot_bounds {
            (bounds.x_min, bounds.x_max, bounds.y_min, bounds.y_max)
        } else {
            // Full range
            let x_min = events.first().map(|e| e.time_secs()).unwrap_or(0.0);
            let x_max = events.last().map(|e| e.time_secs()).unwrap_or(1.0);
            let y_min = events.iter().map(|e| -(e.dy as f64)).fold(f64::INFINITY, f64::min);
            let y_max = events.iter().map(|e| -(e.dy as f64)).fold(f64::NEG_INFINITY, f64::max);
            (x_min, x_max, y_min, y_max)
        };

        // Collect visible indices with advanced LOD
        // - tolerance: 3.0 (allow up to 3 events per pixel before hiding)
        // - zoom_factor: 1.5 (for future caching optimization)
        let visible_height = visible_width / 2.0; // Approximate aspect ratio
        let indices = collect_visible_indices(
            &self.advanced_lod_segments,
            events,
            visible_width,
            visible_height,
            (x_min, x_max),
            (y_min, y_max),
            3.0, // tolerance
            1.5, // zoom_factor
        );

        indices
    }

    fn calculate_stats(&self, events: &[MouseMoveEvent]) -> Stats {
        if events.is_empty() {
            return Stats::default();
        }

        let count = events.len();
        let time_start = events.iter().map(|e| e.time_secs()).fold(f64::INFINITY, |a, b| a.min(b));
        let time_end = events.iter().map(|e| e.time_secs()).fold(f64::NEG_INFINITY, |a, b| a.max(b));
        let duration = (time_end - time_start).max(0.0);

        let total_dx: i64 = events.iter().map(|e| e.dx as i64).sum();
        let total_dy: i64 = events.iter().map(|e| e.dy as i64).sum();

        let mut total_distance = 0f64;
        let mut magnitudes: Vec<f64> = Vec::with_capacity(count);
        for e in events {
            let mag = ((e.dx as f64).powi(2) + (e.dy as f64).powi(2)).sqrt();
            magnitudes.push(mag);
            total_distance += mag;
        }

        let avg_distance_per_event = total_distance / (count as f64);
        let avg_speed = if duration > 0.0 { total_distance / duration } else { 0.0 };
        let events_per_sec = if duration > 0.0 { count as f64 / duration } else { 0.0 };

        // Calculate histogram
        let max_mag = magnitudes.iter().copied().fold(0.0f64, |a, b| a.max(b));
        let bucket_count = 12usize;
        let mut histogram = vec![0usize; bucket_count];
        let bucket_size = if max_mag <= 0.0 { 1.0 } else { max_mag / (bucket_count as f64) };

        for &m in &magnitudes {
            let idx = if bucket_size == 0.0 {
                0
            } else {
                let v = (m / bucket_size).floor() as isize;
                let v = v.max(0).min((bucket_count - 1) as isize);
                v as usize
            };
            histogram[idx] += 1;
        }

        Stats {
            count,
            duration,
            total_dx,
            total_dy,
            total_distance,
            avg_distance_per_event,
            avg_speed,
            events_per_sec,
            histogram,
            bucket_size,
        }
    }
}

#[derive(Default)]
struct Stats {
    count: usize,
    duration: f64,
    total_dx: i64,
    total_dy: i64,
    total_distance: f64,
    avg_distance_per_event: f64,
    avg_speed: f64,
    events_per_sec: f64,
    histogram: Vec<usize>,
    bucket_size: f64,
}

impl eframe::App for MouseAnalyzerGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check F2 key state for edge detection (transition from not pressed to pressed)
        #[cfg(windows)]
        let f2_pressed_now = key_utils::is_f2_pressed();
        #[cfg(not(windows))]
        let f2_pressed_now = false;

        // Detect F2 key press (rising edge)
        let f2_just_pressed = f2_pressed_now && !self.last_f2_state;
        self.last_f2_state = f2_pressed_now;

        // Handle F2 key press
        if f2_just_pressed {
            if self.is_capturing {
                // Stop current capture and take snapshot
                println!("F2 pressed: stopping capture and drawing plot...");
                self.stop_flag.store(true, Ordering::SeqCst);
                self.captured_events = self.events.lock().unwrap().clone();
                self.is_capturing = false;
                
                // Clear Advanced LOD cache since we have new data
                self.advanced_lod_segments.clear();
                self.advanced_lod_last_events_len = 0;
                self.advanced_lod_last_bounds = None;
            } else {
                // Start a new capture
                println!("F2 pressed: starting new capture...");
                // Clear previous data
                self.events.lock().unwrap().clear();
                self.captured_events.clear();
                
                // Clear Advanced LOD cache
                self.advanced_lod_segments.clear();
                self.advanced_lod_last_events_len = 0;
                self.advanced_lod_last_bounds = None;
                
                // Reset stop flag and restart capture
                self.stop_flag.store(false, Ordering::SeqCst);
                self.is_capturing = true;

                // Spawn new capture thread
                let events_capture = Arc::clone(&self.events);
                let stop_capture = Arc::clone(&self.stop_flag);
                let target_device = self.target_device;
                thread::spawn(move || {
                    // Disable F2 watcher in GUI mode since GUI handles F2 itself
                    if let Err(e) = crate::run_capture(events_capture, stop_capture, target_device, true) {
                        eprintln!("Capture error: {}", e);
                    }
                });
            }
        }

        // Only request repaint if we're capturing (to show live event count)
        // When not capturing, we only repaint when needed (user interaction)
        if self.is_capturing {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Use appropriate event data source
        let display_events = if self.is_capturing {
            // During capture, don't show plot (deferred plotting)
            Vec::new()
        } else {
            // After capture, show the captured snapshot
            self.captured_events.clone()
        };

        // Calculate stats (use live events for counting during capture)
        let live_events = self.events.lock().unwrap().clone();
        let count_for_display = if self.is_capturing { live_events.len() } else { display_events.len() };
        let stats = self.calculate_stats(&display_events);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🖱 Mouse Event Analyzer");
                ui.separator();

                if self.is_capturing {
                    ui.label(format!("Capturing: {} events", count_for_display));
                } else {
                    ui.label(format!("Events: {}", stats.count));
                    if stats.duration > 0.0 {
                        ui.label(format!("Duration: {:.2}s", stats.duration));
                    }
                }
            });
        });

        egui::SidePanel::left("side_panel").min_width(200.0).show(ctx, |ui| {
            ui.heading("Controls");
            ui.separator();

            ui.checkbox(&mut self.show_plot, "Show Plot");
            ui.checkbox(&mut self.show_stats, "Show Statistics");
            ui.checkbox(&mut self.show_histogram, "Show Histogram");
            ui.checkbox(&mut self.show_events_table, "Show Events Table");

            ui.separator();

            if self.is_capturing {
                ui.colored_label(egui::Color32::GREEN, "● Recording");
                ui.label(format!("{} events captured", count_for_display));
                ui.label("Press F2 to stop and plot");
            } else {
                ui.colored_label(egui::Color32::RED, "● Stopped");
                ui.label(format!("{} events total", stats.count));
                ui.label("Press F2 to start new capture");
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.is_capturing {
                    // Show waiting message during capture
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.heading("Capturing mouse events...");
                        ui.label(format!("{} events captured", count_for_display));
                        ui.add_space(20.0);
                        ui.label("Move your mouse to record events");
                        ui.label("Press F2 to stop capture and display results");
                    });
                } else if display_events.is_empty() {
                    // Show message when no capture has been done yet
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.heading("Ready to capture");
                        ui.label("Press F2 to start capturing (wait for capture thread to initialize)");
                    });
                } else {
                    // Show statistics and plots after capture is stopped
                    if self.show_stats {
                        ui.group(|ui| {
                            ui.heading("Statistics");
                            ui.separator();

                            egui::Grid::new("stats_grid").num_columns(2).spacing([40.0, 4.0]).striped(true).show(ui, |ui| {
                                ui.label("Event Count:");
                                ui.label(format!("{}", stats.count));
                                ui.end_row();

                                ui.label("Duration:");
                                ui.label(format!("{:.6} s", stats.duration));
                                ui.end_row();

                                ui.label("Total dx:");
                                ui.label(format!("{}", stats.total_dx));
                                ui.end_row();

                                ui.label("Total dy:");
                                ui.label(format!("{}", stats.total_dy));
                                ui.end_row();

                                ui.label("Total Distance:");
                                ui.label(format!("{:.3}", stats.total_distance));
                                ui.end_row();

                                ui.label("Avg Distance/Event:");
                                ui.label(format!("{:.3}", stats.avg_distance_per_event));
                                ui.end_row();

                                if stats.duration > 0.0 {
                                    ui.label("Events/sec:");
                                    ui.label(format!("{:.3}", stats.events_per_sec));
                                    ui.end_row();

                                    ui.label("Avg Speed:");
                                    ui.label(format!("{:.3} units/s", stats.avg_speed));
                                    ui.end_row();
                                }
                            });
                        });
                        ui.add_space(10.0);
                    }

                    if self.show_plot {
                        ui.group(|ui| {
                            ui.heading("Movement Plot (dx and -dy vs time)");
                            ui.separator();

                            use egui_plot::{Line, Plot, PlotPoints};

                            // Get screen width for LOD calculation
                            let available_width = ui.available_width();

                            // Show the plot and capture its response to get bounds
                            let plot_response = Plot::new("mouse_plot").view_aspect(2.0).legend(egui_plot::Legend::default()).show(ui, |plot_ui| {
                                // Get current plot bounds
                                let bounds = plot_ui.plot_bounds();
                                let current_bounds = PlotBounds {
                                    x_min: bounds.min()[0],
                                    x_max: bounds.max()[0],
                                    y_min: bounds.min()[1],
                                    y_max: bounds.max()[1],
                                };

                                // Apply Advanced LOD
                                let lod_indices = self.apply_advanced_lod_indices(&display_events, available_width as f64, Some(&current_bounds));

                                // Helper to safely map indices to plot points
                                let map_to_points = |indices: &[usize], map_fn: fn(&MouseMoveEvent) -> [f64; 2]| indices.iter().filter_map(|&idx| if idx < display_events.len() { Some(map_fn(&display_events[idx])) } else { None }).collect::<PlotPoints>();

                                // Build plot lines by mapping indices to events
                                let dx_points = map_to_points(&lod_indices, |e| [e.time_secs(), e.dx as f64]);
                                let dx_line = Line::new(dx_points).color(egui::Color32::from_rgb(255, 0, 0)).name("dx");

                                let ndy_points = map_to_points(&lod_indices, |e| [e.time_secs(), -(e.dy as f64)]);
                                let ndy_line = Line::new(ndy_points).color(egui::Color32::from_rgb(0, 0, 255)).name("-dy");

                                plot_ui.line(dx_line);
                                plot_ui.line(ndy_line);

                                (current_bounds, lod_indices)
                            });

                            // Update cached values
                            let (current_bounds, lod_indices) = plot_response.inner;
                            self.advanced_lod_last_bounds = Some(current_bounds);

                            // Show LOD info if downsampling occurred
                            if lod_indices.len() < display_events.len() {
                                // Calculate reduction percentage
                                let reduction = 100.0 * (1.0 - lod_indices.len() as f64 / display_events.len() as f64);
                                ui.label(format!("Advanced LOD: Showing {} of {} points ({:.1}% reduction)", lod_indices.len(), display_events.len(), reduction));
                            } else {
                                ui.label(format!("Showing all {} points (no LOD)", display_events.len()));
                            }
                        });
                        ui.add_space(10.0);
                    }

                    if self.show_histogram {
                        ui.group(|ui| {
                            ui.heading("Movement Magnitude Histogram");
                            ui.separator();

                            use egui_plot::{Bar, BarChart, Plot};

                            let bars: Vec<Bar> = stats
                                .histogram
                                .iter()
                                .enumerate()
                                .map(|(i, &count)| Bar::new(i as f64, count as f64).width(0.8).name(format!("[{:.1}-{:.1})", stats.bucket_size * i as f64, stats.bucket_size * (i + 1) as f64)))
                                .collect();

                            let chart = BarChart::new(bars).color(egui::Color32::from_rgb(100, 200, 100)).name("Count");

                            Plot::new("histogram_plot").view_aspect(2.0).legend(egui_plot::Legend::default()).show(ui, |plot_ui| {
                                plot_ui.bar_chart(chart);
                            });
                        });
                        ui.add_space(10.0);
                    }

                    if self.show_events_table {
                        ui.group(|ui| {
                            ui.heading("Events Table");
                            ui.separator();

                            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                egui::Grid::new("events_table").num_columns(4).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
                                    ui.label("Index");
                                    ui.label("dx");
                                    ui.label("dy");
                                    ui.label("Time (s)");
                                    ui.end_row();

                                    // Show last 100 events or all if less
                                    let start_idx = if display_events.len() > 100 { display_events.len() - 100 } else { 0 };

                                    for (idx, event) in display_events.iter().enumerate().skip(start_idx) {
                                        ui.label(format!("{}", idx));
                                        ui.label(format!("{}", event.dx));
                                        ui.label(format!("{}", event.dy));
                                        ui.label(format!("{:.6}", event.time_secs()));
                                        ui.end_row();
                                    }
                                });
                            });

                            if display_events.len() > 100 {
                                ui.label(format!("Showing last 100 of {} events", display_events.len()));
                            }
                        });
                    }
                }
            });
        });
    }
}

pub fn run_gui(events: Arc<Mutex<Vec<MouseMoveEvent>>>, stop_flag: Arc<AtomicBool>, target_device: Option<crate::TargetDevice>) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]).with_title("Mouse Event Analyzer"),
        ..Default::default()
    };

    eframe::run_native("Mouse Event Analyzer", options, Box::new(move |_cc| Box::new(MouseAnalyzerGui::new(events, stop_flag, target_device))))
}
