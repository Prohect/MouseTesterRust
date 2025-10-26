use crate::mouse_event::MouseMoveEvent;
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
    // LOD state for intelligent updates (indices-based)
    cached_lod_indices: Vec<usize>,
    lod_pyramid: Vec<Vec<usize>>, // Lazy LOD pyramid for fast lookups
    last_plot_bounds: Option<PlotBounds>,
    last_target_points: Option<usize>, // Track target_points for coarsen-from-previous
    last_events_len: usize,            // For cache invalidation on capture reset
    lod_threshold: f64,                // Threshold for triggering LOD recalculation (0.1 = 10% change)
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
            cached_lod_indices: Vec::new(),
            lod_pyramid: Vec::new(),
            last_plot_bounds: None,
            last_target_points: None,
            last_events_len: 0,
            lod_threshold: 0.1, // 10% change threshold
        }
    }

    /// Check if plot bounds have changed significantly
    fn bounds_changed_significantly(&self, new_bounds: &PlotBounds) -> bool {
        match self.last_plot_bounds {
            None => true, // First time, always recalculate
            Some(old_bounds) => {
                let x_range_old = (old_bounds.x_max - old_bounds.x_min).abs();
                let y_range_old = (old_bounds.y_max - old_bounds.y_min).abs();
                let x_range_new = (new_bounds.x_max - new_bounds.x_min).abs();
                let y_range_new = (new_bounds.y_max - new_bounds.y_min).abs();

                // Check if range changed by more than threshold
                let x_change = ((x_range_new - x_range_old) / x_range_old.max(1e-6)).abs();
                let y_change = ((y_range_new - y_range_old) / y_range_old.max(1e-6)).abs();

                // Also check if center position moved significantly
                let x_center_old = (old_bounds.x_min + old_bounds.x_max) / 2.0;
                let x_center_new = (new_bounds.x_min + new_bounds.x_max) / 2.0;
                let y_center_old = (old_bounds.y_min + old_bounds.y_max) / 2.0;
                let y_center_new = (new_bounds.y_min + new_bounds.y_max) / 2.0;

                let x_center_change = ((x_center_new - x_center_old) / x_range_old.max(1e-6)).abs();
                let y_center_change = ((y_center_new - y_center_old) / y_range_old.max(1e-6)).abs();

                // Trigger if any change exceeds threshold
                x_change > self.lod_threshold || y_change > self.lod_threshold || x_center_change > self.lod_threshold || y_center_change > self.lod_threshold
            }
        }
    }

    /// Indices-based LOD pipeline for improved performance with static captures
    /// Returns indices into the events slice instead of cloning events
    fn apply_lod_indices(&mut self, events: &[MouseMoveEvent], visible_width: f64, plot_bounds: Option<&PlotBounds>) -> Vec<usize> {
        use std::collections::HashSet;

        if events.is_empty() {
            return Vec::new();
        }

        // Check if cache needs invalidation due to events change
        if events.len() != self.last_events_len {
            self.cached_lod_indices.clear();
            self.lod_pyramid.clear();
            self.last_target_points = None;
            self.last_events_len = events.len();
        }

        // Constants for tuning
        const MARGIN_PX: f64 = 8.0;
        const COLINEARITY_TOL: f64 = 0.6;
        const MIN_POINTS_PER_PIXEL: f64 = 1.0; // Minimum to preserve detail
        const MAX_POINTS_PER_PIXEL: f64 = 3.0; // Maximum when zoomed in

        // Binary search to find visible slice
        let (start_idx, end_idx) = if let Some(bounds) = plot_bounds {
            // Add margin in time units
            let time_range = bounds.x_max - bounds.x_min;
            let margin_time = if time_range > 0.0 { (MARGIN_PX / visible_width) * time_range } else { 0.0 };

            let x_min_with_margin = bounds.x_min - margin_time;
            let x_max_with_margin = bounds.x_max + margin_time;

            // Use partition_point for binary search
            let start = events.partition_point(|e| e.time_secs() < x_min_with_margin);
            let end = events.partition_point(|e| e.time_secs() <= x_max_with_margin);

            (start, end)
        } else {
            (0, events.len())
        };

        let visible_count = end_idx.saturating_sub(start_idx);

        if visible_count == 0 {
            return Vec::new();
        }

        // Calculate target points based on data density and visible width
        // When many events are visible (zoomed out), reduce points per pixel
        // When few events are visible (zoomed in), use more points per pixel
        let data_density = visible_count as f64 / visible_width;

        // Adaptive points per pixel based on data density
        // High density (zoomed out) -> use fewer points per pixel (approach 1.0)
        // Low density (zoomed in) -> use more points per pixel (approach 3.0)
        let points_per_pixel = if data_density > MAX_POINTS_PER_PIXEL {
            // Very high density: use minimum points per pixel
            MIN_POINTS_PER_PIXEL
        } else if data_density < MIN_POINTS_PER_PIXEL {
            // Very low density: use all points (no downsampling)
            data_density
        } else {
            // Medium density: scale between min and max
            // As density increases, reduce points per pixel
            let density_factor = (MAX_POINTS_PER_PIXEL - data_density) / (MAX_POINTS_PER_PIXEL - MIN_POINTS_PER_PIXEL);
            MIN_POINTS_PER_PIXEL + density_factor * (MAX_POINTS_PER_PIXEL - MIN_POINTS_PER_PIXEL)
        };

        let target_points = (visible_width * points_per_pixel).max(visible_width * MIN_POINTS_PER_PIXEL) as usize;
        let target_points = target_points.min(visible_count); // Never exceed visible count

        if visible_count <= target_points {
            // No downsampling needed
            return (start_idx..end_idx).collect();
        }

        // Check for coarsen-from-previous fast path (zooming out)
        if let Some(last_target) = self.last_target_points {
            if target_points < last_target && !self.cached_lod_indices.is_empty() {
                // Coarsen from cached indices
                let step = self.cached_lod_indices.len() / target_points.max(1);
                let step = step.max(1);

                let coarsened: Vec<usize> = self.cached_lod_indices.iter().step_by(step).copied().collect();

                self.last_target_points = Some(target_points);
                return coarsened;
            }
        }

        // Full LOD computation with trend preservation
        let mut selected_indices = Vec::with_capacity(target_points);
        let mut dedup_set = HashSet::new();

        // Calculate bucket size
        let bucket_size = visible_count / target_points.max(1);
        let bucket_size = bucket_size.max(1);

        for bucket_start in (start_idx..end_idx).step_by(bucket_size) {
            let bucket_end = (bucket_start + bucket_size).min(end_idx);

            if bucket_start >= bucket_end {
                continue;
            }

            // Collect indices for this bucket with trend preservation
            // Include first and last of bucket
            let first_idx = bucket_start;
            let last_idx = bucket_end - 1;

            // Find min/max dx and dy in bucket
            let mut min_dx_idx = first_idx;
            let mut max_dx_idx = first_idx;
            let mut min_dy_idx = first_idx;
            let mut max_dy_idx = first_idx;

            for idx in bucket_start..bucket_end {
                if events[idx].dx < events[min_dx_idx].dx {
                    min_dx_idx = idx;
                }
                if events[idx].dx > events[max_dx_idx].dx {
                    max_dx_idx = idx;
                }
                if events[idx].dy < events[min_dy_idx].dy {
                    min_dy_idx = idx;
                }
                if events[idx].dy > events[max_dy_idx].dy {
                    max_dy_idx = idx;
                }
            }

            // Add unique indices using time deduplication
            for &idx in &[first_idx, last_idx, min_dx_idx, max_dx_idx, min_dy_idx, max_dy_idx] {
                let time_bits = events[idx].time_secs().to_bits();
                if dedup_set.insert(time_bits) {
                    selected_indices.push(idx);
                }
            }
        }

        // Sort indices to maintain time order
        selected_indices.sort_unstable();

        // Validate all indices are within bounds
        debug_assert!(selected_indices.iter().all(|&idx| idx < events.len()), "All indices should be within events bounds");

        // Update cache
        self.cached_lod_indices = selected_indices.clone();
        self.last_target_points = Some(target_points);

        selected_indices
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
                // Clear LOD cache since we have new data
                self.cached_lod_indices.clear();
                self.lod_pyramid.clear();
                self.last_plot_bounds = None;
                self.last_target_points = None;
                self.last_events_len = self.captured_events.len();
            } else {
                // Start a new capture
                println!("F2 pressed: starting new capture...");
                // Clear previous data
                self.events.lock().unwrap().clear();
                self.captured_events.clear();
                self.cached_lod_indices.clear();
                self.lod_pyramid.clear();
                self.last_plot_bounds = None;
                self.last_target_points = None;
                self.last_events_len = 0;
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

                                // Check if we need to recalculate LOD
                                let needs_lod_update = self.bounds_changed_significantly(&current_bounds) || self.cached_lod_indices.is_empty();

                                let lod_indices = if needs_lod_update {
                                    // Recalculate LOD with current bounds
                                    self.apply_lod_indices(&display_events, available_width as f64, Some(&current_bounds))
                                } else {
                                    // Use cached LOD indices
                                    self.cached_lod_indices.clone()
                                };

                                // Helper to safely map indices to plot points
                                let map_to_points = |indices: &[usize], map_fn: fn(&MouseMoveEvent) -> [f64; 2]| indices.iter().filter_map(|&idx| if idx < display_events.len() { Some(map_fn(&display_events[idx])) } else { None }).collect::<PlotPoints>();

                                // Build plot lines by mapping indices to events
                                let dx_points = map_to_points(&lod_indices, |e| [e.time_secs(), e.dx as f64]);
                                let dx_line = Line::new(dx_points).color(egui::Color32::from_rgb(255, 0, 0)).name("dx");

                                let ndy_points = map_to_points(&lod_indices, |e| [e.time_secs(), -(e.dy as f64)]);
                                let ndy_line = Line::new(ndy_points).color(egui::Color32::from_rgb(0, 0, 255)).name("-dy");

                                plot_ui.line(dx_line);
                                plot_ui.line(ndy_line);

                                (current_bounds, lod_indices, needs_lod_update)
                            });

                            // Update cached values if LOD was recalculated
                            let (current_bounds, lod_indices, needs_lod_update) = plot_response.inner;
                            if needs_lod_update {
                                self.cached_lod_indices = lod_indices.clone();
                                self.last_plot_bounds = Some(current_bounds);
                            }

                            // Show LOD info if downsampling occurred
                            if lod_indices.len() < display_events.len() {
                                // Calculate reduction percentage
                                let reduction = 100.0 * (1.0 - lod_indices.len() as f64 / display_events.len() as f64);
                                ui.label(format!("LOD: Showing {} of {} points ({:.1}% reduction)", lod_indices.len(), display_events.len(), reduction));
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
