use crate::mouse_event::MouseMoveEvent;
// Import the LOD module
use crate::lod::{Segment, build_segments, collect_visible_indices, LodCache};
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

    // LOD state
    lod_segments: Vec<Segment>,
    // Error points detected by regression analysis (indices of events with high residuals)
    // Filtered to only show points between min_x_visible and max_x_visible
    lod_error_points: Vec<usize>,
    lod_error_points_backup: Vec<usize>,
    lod_last_events_len: usize,
    lod_last_bounds: Option<PlotBounds>,
    // Cache for visible indices
    lod_cache: Option<LodCache>,
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
            show_stats: false,
            show_histogram: false,
            show_events_table: false,
            is_capturing: true, // Start capturing initially
            captured_events: Vec::new(),
            last_f2_state: false,
            target_device,

            // LOD initialization
            lod_segments: Vec::new(),
            lod_error_points: Vec::new(),
            lod_error_points_backup: Vec::new(),
            lod_last_events_len: 0,
            lod_last_bounds: None,
            lod_cache: None,
        }
    }

    /// Check if plot bounds have changed significantly
    fn bounds_changed_significantly(&self, new_bounds: &PlotBounds) -> bool {
        match self.lod_last_bounds {
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

    /// Calculate error points based on regression residuals
    /// Error is detected when: abs(y0-y1)/max(smallestPositive,abs(y1)) > (sqrt(1-r2)/k)
    fn calculate_error_points(&self, events: &[MouseMoveEvent]) -> Vec<usize> {
        let mut error_points = Vec::new();
        const K: f64 = 3.0;
        const SMALLEST_POSITIVE: f64 = 1e-8;

        for segment in &self.lod_segments {
            if let Segment::Good { start_idx, end_idx, fit } = segment {
                let n = end_idx - start_idx;
                if n < 4 {
                    continue;
                }

                // Normalize indices to [0, 1] for polynomial evaluation
                let indices: Vec<f64> = (0..n).map(|i| i as f64).collect();
                let max_idx = (n - 1) as f64;
                let idx_norm: Vec<f64> = indices.iter().map(|&i| if max_idx > 0.0 { i / max_idx } else { 0.0 }).collect();

                // Check each event in the segment
                for (local_idx, &normalized_idx) in idx_norm.iter().enumerate() {
                    let global_idx = start_idx + local_idx;
                    if global_idx >= events.len() {
                        continue;
                    }

                    let event = &events[global_idx];

                    // Get actual values
                    let dx_actual = event.dx as f64;
                    let dy_actual = event.dy as f64;
                    let time_actual = event.time_secs();

                    // Get predicted values from polynomials
                    let dx_pred = fit.dx_poly.eval(normalized_idx);
                    let dy_pred = fit.dy_poly.eval(normalized_idx);
                    let time_pred = fit.time_poly.eval(normalized_idx);

                    // Calculate error thresholds for each dimension
                    let dx_threshold = (1.0 - fit.dx_r_squared).max(0.0).sqrt() * K;
                    let dy_threshold = (1.0 - fit.dy_r_squared).max(0.0).sqrt() * K;
                    let time_threshold = (1.0 - fit.time_r_squared).max(0.0).sqrt() * K;

                    // Calculate relative errors
                    let dx_error = (dx_actual - dx_pred).abs() / dx_pred.abs().max(SMALLEST_POSITIVE);
                    let dy_error = (dy_actual - dy_pred).abs() / dy_pred.abs().max(SMALLEST_POSITIVE);
                    let time_error = (time_actual - time_pred).abs() / time_pred.abs().max(SMALLEST_POSITIVE);

                    // Mark as error if any dimension exceeds threshold
                    if dx_error > dx_threshold || dy_error > dy_threshold || time_error > time_threshold {
                        error_points.push(global_idx);
                    }
                }
            }
        }

        error_points
    }

    /// Apply the LOD algorithm with regression-based segmentation
    /// Returns indices into the events slice for rendering
    fn apply_lod_indices(&mut self, events: &[MouseMoveEvent], visible_width: f64, visible_height: f64, plot_bounds: Option<&PlotBounds>) -> Vec<usize> {
        if events.is_empty() {
            return Vec::new();
        }

        // Check if we need to rebuild segments (events changed)
        if events.len() != self.lod_last_events_len {
            println!("Building LOD segments for {} events...", events.len());
            // Build segments with good parameters for real mouse data
            // - balance_weight: 0.091 (ln(len) is not and cant be normalized to 0.0 ~ 1.0)
            self.lod_segments = build_segments(events, 10, 1.6, 0.98, 0.091);
            self.lod_last_events_len = events.len();
            println!("Created {} segments", self.lod_segments.len());
            println!("Created {} discrete segments", self.lod_segments.iter().find(|&s| if let Segment::Discrete { idx: _ } = s {
                true
            }else{false}).into_iter().count());

            // Calculate error points after building segments
            let all_error_points = self.calculate_error_points(events);
            println!("Detected {} error points", all_error_points.len());
            self.lod_error_points_backup = all_error_points;
            
            // Clear cache since segments changed
            self.lod_cache = None;
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

        // Calculate visible range with zoom factor for pre-fetching
        let x_range_size = x_max - x_min;
        let zoom_factor = 1.2;
        let tolerance = 3.0;
        
        // Check if we can reuse cached results
        let indices = if let Some(ref cache) = self.lod_cache {
            if cache.can_reuse((x_min, x_max), (y_min, y_max), tolerance, zoom_factor) {
                // Filter cached indices to current view
                let filtered: Vec<usize> = cache.visible_indices.iter()
                    .filter(|&&idx| {
                        if idx < events.len() {
                            let time = events[idx].time_secs();
                            time >= x_min && time <= x_max
                        } else {
                            false
                        }
                    })
                    .copied()
                    .collect();
                println!("Using cached LOD results: {} -> {} indices", cache.visible_indices.len(), filtered.len());
                filtered
            } else {
                // Cache invalid, recompute
                self.compute_and_cache_indices(events, visible_width, visible_height, x_min, x_max, y_min, y_max, tolerance, zoom_factor)
            }
        } else {
            // No cache, compute fresh
            self.compute_and_cache_indices(events, visible_width, visible_height, x_min, x_max, y_min, y_max, tolerance, zoom_factor)
        };
        
        // Filter error points to only those in visible range (with zoom factor extension)
        let min_x_visible = x_min - (x_range_size * ((zoom_factor - 1.0) / 2.0));
        let max_x_visible = x_max + (x_range_size * ((zoom_factor - 1.0) / 2.0));
        
        self.lod_error_points = self.lod_error_points_backup.clone();
        self.lod_error_points.retain(|&idx| {
            if idx < events.len() {
                let time = events[idx].time_secs();
                time >= min_x_visible && time <= max_x_visible
            } else {
                false
            }
        });

        indices
    }
    
    /// Compute visible indices and cache the result
    fn compute_and_cache_indices(
        &mut self,
        events: &[MouseMoveEvent],
        visible_width: f64,
        visible_height: f64,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        tolerance: f64,
        zoom_factor: f64,
    ) -> Vec<usize> {
        // Collect visible indices with LOD using extended range for caching
        let x_range_size = x_max - x_min;
        let cache_x_min = x_min - (x_range_size * ((zoom_factor - 1.0) / 2.0));
        let cache_x_max = x_max + (x_range_size * ((zoom_factor - 1.0) / 2.0));
        
        let indices = collect_visible_indices(
            &self.lod_segments,
            events,
            visible_width,
            visible_height,
            (cache_x_min, cache_x_max),
            (y_min, y_max),
            tolerance,
            zoom_factor,
        );
        
        // Cache the result
        self.lod_cache = Some(LodCache {
            segments: self.lod_segments.clone(),
            visible_indices: indices.clone(),
            zoom_factor,
            last_x_range: (cache_x_min, cache_x_max),
            last_y_range: (y_min, y_max),
            last_tolerance: tolerance,
        });
        
        println!("Computed and cached {} LOD indices", indices.len());
        
        // Filter to current view
        indices.into_iter()
            .filter(|&idx| {
                if idx < events.len() {
                    let time = events[idx].time_secs();
                    time >= x_min && time <= x_max
                } else {
                    false
                }
            })
            .collect()
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
                self.lod_segments.clear();
                self.lod_last_events_len = 0;
                self.lod_last_bounds = None;
                self.lod_cache = None;
            } else {
                // Start a new capture
                println!("F2 pressed: starting new capture...");
                // Clear previous data
                self.events.lock().unwrap().clear();
                self.captured_events.clear();

                // Clear LOD cache
                self.lod_segments.clear();
                self.lod_last_events_len = 0;
                self.lod_last_bounds = None;
                self.lod_cache = None;

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

                            use egui_plot::{Line, Plot, PlotPoints, Points};

                            // Get screen resolution for LOD calculation
                            let available_width = ui.available_width();
                            let available_height = ui.available_height();

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

                                // Apply LOD
                                let lod_indices = self.apply_lod_indices(&display_events, available_width as f64, available_height as f64, Some(&current_bounds));

                                // Helper to safely map indices to plot points
                                let map_to_points = |indices: &[usize], map_fn: fn(&MouseMoveEvent) -> [f64; 2]| indices.iter().filter_map(|&idx| if idx < display_events.len() { Some(map_fn(&display_events[idx])) } else { None }).collect::<PlotPoints>();

                                // Build plot lines by mapping indices to events
                                let dx_points = map_to_points(&lod_indices, |e| [e.time_secs(), e.dx as f64]);
                                let dx_line = Line::new(dx_points).color(egui::Color32::from_rgb(255, 0, 0)).name("dx");

                                let ndy_points = map_to_points(&lod_indices, |e| [e.time_secs(), -(e.dy as f64)]);
                                let ndy_line = Line::new(ndy_points).color(egui::Color32::from_rgb(0, 0, 255)).name("-dy");

                                plot_ui.line(dx_line);
                                plot_ui.line(ndy_line);

                                // Add error points visualization (shown as orange markers)
                                if !self.lod_error_points.is_empty() {
                                    // For dx error points
                                    let dx_error_points = map_to_points(&self.lod_error_points, |e| [e.time_secs(), e.dx as f64]);
                                    let dx_error_markers = Points::new(dx_error_points)
                                        .color(egui::Color32::from_rgb(255, 165, 0))
                                        .radius(3.0)
                                        .name("dx errors");
                                    plot_ui.points(dx_error_markers);

                                    // For -dy error points
                                    let ndy_error_points = map_to_points(&self.lod_error_points, |e| [e.time_secs(), -(e.dy as f64)]);
                                    let ndy_error_markers = Points::new(ndy_error_points)
                                        .color(egui::Color32::from_rgb(255, 165, 0))
                                        .radius(3.0)
                                        .name("-dy errors");
                                    plot_ui.points(ndy_error_markers);
                                }

                                (current_bounds, lod_indices)
                            });

                            // Update cached values
                            let (current_bounds, lod_indices) = plot_response.inner;
                            self.lod_last_bounds = Some(current_bounds);

                            // Show LOD info if downsampling occurred
                            if lod_indices.len() < display_events.len() {
                                // Calculate reduction percentage
                                let reduction = 100.0 * (1.0 - lod_indices.len() as f64 / display_events.len() as f64);
                                ui.label(format!("LOD: Showing {} of {} points ({:.1}% reduction)", lod_indices.len(), display_events.len(), reduction));
                            } else {
                                ui.label(format!("Showing all {} points (no LOD)", display_events.len()));
                            }

                            // Show error points info
                            if !self.lod_error_points.is_empty() {
                                ui.colored_label(
                                    egui::Color32::from_rgb(255, 165, 0),
                                    format!("⚠ {} error points detected (shown as orange markers)", self.lod_error_points.len())
                                );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_events(n: usize) -> Vec<MouseMoveEvent> {
        let mut events = Vec::new();
        for i in 0..n {
            let t_sec = i as u32;
            let t_usec = 0;
            // Linear pattern with some noise
            let dx = (i * 10) as i16;
            let dy = -(i as i16 * 5);
            events.push(MouseMoveEvent::new(dx, dy, t_sec, t_usec));
        }
        events
    }

    #[test]
    fn test_calculate_error_points_empty_segments() {
        let events = create_test_events(10);
        let gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(AtomicBool::new(false)),
            None
        );
        
        let error_points = gui.calculate_error_points(&events);
        assert_eq!(error_points.len(), 0, "Should have no error points with empty segments");
    }

    #[test]
    fn test_calculate_error_points_with_good_segments() {
        let events = create_test_events(50);
        let mut gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(AtomicBool::new(false)),
            None
        );
        
        // Build segments
        gui.lod_segments = build_segments(&events, 10, 1.6, 0.8, 0.091);
        
        let error_points = gui.calculate_error_points(&events);
        
        // With linear data, we should have very few or no error points
        // (since the data fits well to polynomial regression)
        assert!(error_points.len() <= events.len(), "Error points should not exceed total events");
        println!("Detected {} error points out of {} events", error_points.len(), events.len());
    }

    #[test]
    fn test_calculate_error_points_with_outliers() {
        let mut events = create_test_events(30);
        // Add some outliers that should be detected as errors
        if events.len() > 15 {
            events[15].dx = 1000; // Major outlier
            events[16].dy = -1000; // Major outlier
        }
        
        let mut gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(AtomicBool::new(false)),
            None
        );
        
        // Build segments
        gui.lod_segments = build_segments(&events, 10, 1.6, 0.8, 0.091);
        
        let error_points = gui.calculate_error_points(&events);
        
        // Should detect some error points due to outliers
        println!("Detected {} error points with outliers", error_points.len());
        assert!(!error_points.is_empty(), "Should detect error points with outliers");
    }

    #[test]
    fn test_error_points_filtered_by_visible_range() {
        let events = create_test_events(100);
        let mut gui = MouseAnalyzerGui::new(
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(AtomicBool::new(false)),
            None
        );
        
        // Build segments and calculate error points
        gui.lod_segments = build_segments(&events, 10, 1.6, 0.8, 0.091);
        gui.lod_error_points = gui.calculate_error_points(&events);
        
        let initial_error_count = gui.lod_error_points.len();
        
        // Now apply LOD with limited visible range (should filter error points)
        let bounds = PlotBounds {
            x_min: 10.0,
            x_max: 30.0,
            y_min: -500.0,
            y_max: 500.0,
        };
        
        gui.apply_lod_indices(&events, 800.0, 600.0, Some(&bounds));
        
        // Error points should be filtered to visible range
        for &idx in &gui.lod_error_points {
            if idx < events.len() {
                let time = events[idx].time_secs();
                let x_range_size = bounds.x_max - bounds.x_min;
                let zoom_factor = 1.2;
                let min_x_visible = bounds.x_min - (x_range_size * ((zoom_factor - 1.0) / 2.0));
                let max_x_visible = bounds.x_max + (x_range_size * ((zoom_factor - 1.0) / 2.0));
                
                assert!(
                    time >= min_x_visible && time <= max_x_visible,
                    "Error point at index {} with time {} should be within visible range [{}, {}]",
                    idx, time, min_x_visible, max_x_visible
                );
            }
        }
        
        println!(
            "Filtered error points from {} to {} based on visible range",
            initial_error_count,
            gui.lod_error_points.len()
        );
    }
}
