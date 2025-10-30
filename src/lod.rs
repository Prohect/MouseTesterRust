//! Level-of-Detail (LOD) algorithm with time consistency and regression-based segmentation
//!
//! This module implements an intelligent LOD algorithm that analyzes mouse movement events
//! for time consistency and data quality, segments them based on cubic polynomial regression
//! quality (R-squared), and provides efficient view-dependent filtering with caching.
//!
//! # Dependencies
//!
//! This module uses the `nalgebra` crate for numerical linear algebra operations,
//! specifically SVD (Singular Value Decomposition) for stable least-squares fitting
//! of cubic polynomials to mouse movement data.
//!
//! # Key Features
//!
//! - **Time Consistency Analysis**: Detects events with poor time linearity (report rate issues)
//! - **Discrete Event Detection**: Identifies zero-movement events and outliers
//! - **Adaptive Segmentation**: Creates segments with optimal R-squared and length balance
//! - **Smart Caching**: Caches regression results and reuses them for zoom operations
//! - **View-Dependent Filtering**: Hides redundant events based on rendering resolution
//!
//! # Algorithm Overview
//!
//! 1. **Segment Analysis Phase**:
//!    - Start with initial segment size (e.g., 10 events)
//!    - Try larger segments by multiplying size by growth factor (1.5 or 2)
//!    - For each segment size, compute cubic polynomial fit for dx, dy, and time vs index
//!    - Calculate R-squared to measure fit quality
//!    - Balance between segment length and R-squared (prefer longer segments with good R²)
//!    - Mark discrete events: zero dx/dy, or poor time/position consistency
//!
//! 2. **Caching Phase**:
//!    - Store segment boundaries with their regression parameters
//!    - Cache R-squared values for each segment
//!    - Store indices of discrete events separately
//!
//! 3. **View Collection Phase**:
//!    - Given rendering resolution, x/y ranges, tolerance, and zoom factor
//!    - Calculate which events map to same pixel on screen
//!    - Keep first and last event of each good segment (preserve continuity)
//!    - Apply tolerance: hide events if more than tolerance map to same pixel
//!    - Return list of event indices that should be rendered

use crate::mouse_event::MouseMoveEvent;
use nalgebra::{DMatrix, DVector};
use std::collections::HashSet;

// Constants for numerical stability and tolerance
const SVD_TOLERANCE: f64 = 1e-10; // Tolerance for SVD solving
const MIN_RANGE_VALUE: f64 = 1e-10; // Minimum range to prevent division by zero
const ZOOM_TOLERANCE_FACTOR: f64 = 0.9; // 10% tolerance for zoom factor comparison

/// Cubic polynomial coefficients: f(t) = a0 + a1*t + a2*t^2 + a3*t^3
#[derive(Debug, Clone, Copy)]
pub struct Poly3 {
    pub a0: f64,
    pub a1: f64,
    pub a2: f64,
    pub a3: f64,
}

impl Poly3 {
    /// Evaluate the polynomial at time t
    pub fn eval(&self, t: f64) -> f64 {
        self.a0 + self.a1 * t + self.a2 * t * t + self.a3 * t * t * t
    }

    /// Create a zero polynomial
    pub fn zero() -> Self {
        Self { a0: 0.0, a1: 0.0, a2: 0.0, a3: 0.0 }
    }
}

/// Result of regression analysis for a segment
#[derive(Debug, Clone)]
pub struct SegmentFit {
    pub start_idx: usize,
    pub end_idx: usize,
    pub dx_poly: Poly3,
    pub dy_poly: Poly3,
    pub time_poly: Poly3, // Time vs index fit
    pub dx_r_squared: f64,
    pub dy_r_squared: f64,
    pub time_r_squared: f64, // Time consistency metric
}

/// Represents a segment of events with classification
#[derive(Debug, Clone)]
pub enum Segment {
    /// Good segment with high-quality polynomial fit
    Good { start_idx: usize, end_idx: usize, fit: SegmentFit },
    /// Discrete event that doesn't fit well
    Discrete { idx: usize },
}

/// Cached LOD analysis result
#[derive(Debug, Clone)]
pub struct LodCache {
    pub segments: Vec<Segment>,
    pub visible_indices: Vec<usize>,
    pub zoom_factor: f64,
    pub last_x_range: (f64, f64),
    pub last_y_range: (f64, f64),
    pub last_tolerance: f64,
}

impl LodCache {
    /// Check if cached result can be reused for given view
    pub fn can_reuse(&self, x_range: (f64, f64), y_range: (f64, f64), tolerance: f64, zoom_factor: f64) -> bool {
        // Cache is valid if new view is within cached view (zoomed in)
        let x_within = x_range.0 >= self.last_x_range.0 && x_range.1 <= self.last_x_range.1;
        let y_within = y_range.0 >= self.last_y_range.0 && y_range.1 <= self.last_y_range.1;
        let zoom_ok = zoom_factor >= self.zoom_factor * ZOOM_TOLERANCE_FACTOR;
        let tolerance_ok = (tolerance - self.last_tolerance).abs() < 0.01;

        x_within && y_within && zoom_ok && tolerance_ok
    }
}

/// Normalize values to [0, 1] range for better numerical conditioning
fn normalize_to_unit(values: &[f64]) -> (Vec<f64>, f64, f64) {
    if values.is_empty() {
        return (Vec::new(), 0.0, 1.0);
    }

    let min_val = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_val = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = (max_val - min_val).max(MIN_RANGE_VALUE);

    let normalized: Vec<f64> = values.iter().map(|&v| (v - min_val) / range).collect();

    (normalized, min_val, range)
}

/// Fit a cubic polynomial using least-squares with SVD
///
/// Requires at least 4 data points for cubic fitting.
/// Returns None if fewer than 4 points are provided or if SVD solving fails.
fn fit_cubic(x_norm: &[f64], y: &[f64]) -> Option<Poly3> {
    let n = x_norm.len();
    if n < 4 {
        return None;
    }

    // Build design matrix: [1, x, x^2, x^3]
    let mut a_data = vec![0.0; n * 4];
    for i in 0..n {
        let x = x_norm[i];
        let x2 = x * x;
        let x3 = x2 * x;
        a_data[i * 4] = 1.0;
        a_data[i * 4 + 1] = x;
        a_data[i * 4 + 2] = x2;
        a_data[i * 4 + 3] = x3;
    }

    let a = DMatrix::from_row_slice(n, 4, &a_data);
    let b = DVector::from_row_slice(y);

    let svd = a.svd(true, true);
    let coeffs = svd.solve(&b, SVD_TOLERANCE).ok()?;

    Some(Poly3 {
        a0: coeffs[0],
        a1: coeffs[1],
        a2: coeffs[2],
        a3: coeffs[3],
    })
}

/// Calculate R-squared (coefficient of determination) for a fit
fn calculate_r_squared(y_actual: &[f64], y_pred: &[f64]) -> f64 {
    if y_actual.len() != y_pred.len() || y_actual.is_empty() {
        return 0.0;
    }

    let n = y_actual.len() as f64;
    let y_mean = y_actual.iter().sum::<f64>() / n;

    let ss_tot: f64 = y_actual.iter().map(|&y| (y - y_mean).powi(2)).sum();
    let ss_res: f64 = y_actual.iter().zip(y_pred.iter()).map(|(&y_a, &y_p)| (y_a - y_p).powi(2)).sum();

    if ss_tot < 1e-10 {
        // If variance is near zero, perfect fit
        return 1.0;
    }

    1.0 - (ss_res / ss_tot)
}

/// Analyze a segment and compute regression fit with R-squared
fn analyze_segment(events: &[MouseMoveEvent], start_idx: usize, end_idx: usize) -> Option<SegmentFit> {
    let n = end_idx - start_idx;
    if n < 4 {
        return None;
    }

    // Extract indices normalized to [0, 1]
    let indices: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let (idx_norm, _, _) = normalize_to_unit(&indices);

    // Extract time values
    let times: Vec<f64> = (start_idx..end_idx).map(|i| events[i].time_secs()).collect();

    // Extract dx and dy values
    let dx_vals: Vec<f64> = (start_idx..end_idx).map(|i| events[i].dx as f64).collect();
    let dy_vals: Vec<f64> = (start_idx..end_idx).map(|i| events[i].dy as f64).collect();

    // Fit polynomials
    let dx_poly = fit_cubic(&idx_norm, &dx_vals)?;
    let dy_poly = fit_cubic(&idx_norm, &dy_vals)?;
    let time_poly = fit_cubic(&idx_norm, &times)?;

    // Calculate predictions
    let dx_pred: Vec<f64> = idx_norm.iter().map(|&x| dx_poly.eval(x)).collect();
    let dy_pred: Vec<f64> = idx_norm.iter().map(|&x| dy_poly.eval(x)).collect();
    let time_pred: Vec<f64> = idx_norm.iter().map(|&x| time_poly.eval(x)).collect();

    // Calculate R-squared for each
    let dx_r_squared = calculate_r_squared(&dx_vals, &dx_pred);
    let dy_r_squared = calculate_r_squared(&dy_vals, &dy_pred);
    let time_r_squared = calculate_r_squared(&times, &time_pred);

    Some(SegmentFit {
        start_idx,
        end_idx,
        dx_poly,
        dy_poly,
        time_poly,
        dx_r_squared,
        dy_r_squared,
        time_r_squared,
    })
}

/// Check if an event is discrete (zero movement or poor fit)
fn is_discrete_event(event: &MouseMoveEvent) -> bool {
    event.dx == 0 && event.dy == 0
}

/// Build segments with adaptive sizing and R-squared optimization
///
/// # Parameters
///
/// - `events`: The mouse movement events to segment
/// - `initial_size`: Initial segment size to try (e.g., 10)
/// - `growth_factor`: Factor to multiply segment size when expanding (e.g., 1.5 or 2.0)
/// - `min_r_squared`: Minimum acceptable R-squared (e.g., 0.8 or 0.9)
/// - `balance_weight`: Weight for balancing length vs R-squared (0.0-1.0, higher favors length)
///
/// # Returns
///
/// Vector of segments (Good or Discrete)
pub fn build_segments(events: &[MouseMoveEvent], initial_size: usize, growth_factor: f64, min_r_squared: f64, balance_weight: f64) -> Vec<Segment> {
    if events.is_empty() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut pos = 0;

    while pos < events.len() {
        // Try progressively larger segments
        let mut best_fit: Option<SegmentFit> = None;
        let mut best_score = f64::NEG_INFINITY;
        let mut best_r_squared = f64::NEG_INFINITY;
        let mut current_size = initial_size;
        let mut fit_tolerance = 0;
        let max_fit_tolerance_r_squared_up = 10;
        let max_fit_tolerance_r_squared_down = 3;

        while pos + current_size <= events.len() {
            let end = pos + current_size;

            if let Some(fit) = analyze_segment(events, pos, end) {
                // Calculate composite R-squared (average of dx, dy, time)
                let avg_r_squared = (fit.dx_r_squared + fit.dy_r_squared + fit.time_r_squared) / 3.0;

                // Only consider if all individual R-squared values are reasonable
                if avg_r_squared >= min_r_squared && fit.time_r_squared >= min_r_squared * 0.7 {
                    // Score balances R-squared and segment length
                    // Higher balance_weight favors longer segments
                    let length_score = (current_size as f64).ln();
                    let score = balance_weight * length_score + (1.0 - balance_weight) * avg_r_squared;

                    if score > best_score {
                        best_score = score;
                        best_fit = Some(fit);
                        fit_tolerance = 0;
                    }
                } else {
                    // Fit quality degraded
                    if avg_r_squared > best_r_squared {
                        fit_tolerance += 1;
                        if fit_tolerance > max_fit_tolerance_r_squared_up {
                            break;
                        }
                    } else {
                        fit_tolerance += 1;
                        if fit_tolerance > max_fit_tolerance_r_squared_down {
                            break;
                        }
                    }
                }
                // Try larger segment
                current_size = ((current_size as f64) * growth_factor).ceil() as usize;

                if avg_r_squared > best_r_squared {
                    best_r_squared = avg_r_squared;
                }
            } else {
                break;
            }
        }

        if let Some(fit) = best_fit {
            let segment_len = fit.end_idx - fit.start_idx;
            segments.push(Segment::Good {
                start_idx: fit.start_idx,
                end_idx: fit.end_idx,
                fit,
            });
            pos += segment_len;
        } else {
            // Couldn't fit well, mark as discrete
            segments.push(Segment::Discrete { idx: pos });
            pos += 1;
        }
    }

    segments
}

/// Collect visible event indices for rendering based on view parameters
///
/// # Parameters
///
/// - `segments`: Pre-computed segments from build_segments
/// - `events`: The mouse movement events
/// - `render_width`: Width of rendering area in pixels
/// - `render_height`: Height of rendering area in pixels
/// - `x_range`: (x_min, x_max) time range to render
/// - `y_range`: (y_min, y_max) value range to render
/// - `tolerance`: Maximum events per pixel before hiding (e.g., 3.0)
/// - `zoom_factor`: Zoom factor for caching optimization (>1.0, e.g., 1.5)
///   - Values > 1.0 will pre-fetch a larger area for smoother zoom/pan operations
///   - The visible range is extended by (zoom_factor - 1.0) / 2 on each side
///
/// # Returns
///
/// Vector of event indices to render
pub fn collect_visible_indices(segments: &[Segment], events: &[MouseMoveEvent], render_width: f64, render_height: f64, x_range: (f64, f64), y_range: (f64, f64), tolerance: f64, zoom_factor: f64) -> Vec<usize> {
    if events.is_empty() || segments.is_empty() {
        return Vec::new();
    }

    let mut visible_indices = Vec::new();
    let mut seen_pixels = HashSet::new();

    // Calculate pixel scales
    let x_range_size = x_range.1 - x_range.0;
    let y_range_size = y_range.1 - y_range.0;
    let x_scale = render_width / (x_range_size).max(1e-10);
    let y_scale = render_height / (y_range_size).max(1e-10);

    // Note: zoom_factor is provided but the x_range should already be extended by the caller
    // if they want to cache a larger area. We use x_range directly for visibility checks.
    let min_x_visible = x_range.0;
    let max_x_visible = x_range.1;

    // Helper: convert event to pixel coordinates
    let to_pixel = |event: &MouseMoveEvent| -> (i32, i32) {
        let px = ((event.time_secs() - x_range.0) * x_scale) as i32;
        let py = ((-(event.dy as f64) - y_range.0) * y_scale) as i32;
        (px, py)
    };

    // Helper: check if event is within visible time range
    let is_visible = |event: &MouseMoveEvent| -> bool {
        let time = event.time_secs();
        time >= min_x_visible && time <= max_x_visible
    };

    // Process each segment
    for segment in segments {
        match segment {
            Segment::Discrete { idx } => {
                // Only include discrete events if they're visible
                if *idx < events.len() && is_visible(&events[*idx]) {
                    visible_indices.push(*idx);
                }
            }
            Segment::Good { start_idx, end_idx, .. } => {
                // For good segments, apply intelligent filtering
                if *start_idx >= events.len() || *end_idx > events.len() {
                    continue;
                }

                let segment_events = &events[*start_idx..*end_idx];

                // Check if any event in this segment is visible
                let has_visible = segment_events.iter().any(|e| is_visible(e));
                if !has_visible {
                    // Skip entire segment if no events are visible
                    continue;
                }

                // Always include first and last to preserve continuity (if visible)
                if is_visible(&events[*start_idx]) {
                    visible_indices.push(*start_idx);
                }
                if end_idx - start_idx > 1 && is_visible(&events[*end_idx - 1]) {
                    visible_indices.push(*end_idx - 1);
                }

                // For interior points, apply tolerance-based filtering
                let mut pixel_counts: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();

                for (local_idx, event) in segment_events.iter().enumerate() {
                    // Only process visible events
                    if !is_visible(event) {
                        continue;
                    }
                    let global_idx = start_idx + local_idx;
                    let pixel = to_pixel(event);
                    pixel_counts.entry(pixel).or_insert_with(Vec::new).push(global_idx);
                }

                // Add events based on tolerance
                for (pixel, indices) in pixel_counts.iter() {
                    if seen_pixels.contains(pixel) {
                        continue;
                    }

                    let count = indices.len() as f64;
                    if count <= tolerance {
                        // Include all events at this pixel
                        for &idx in indices {
                            // Don't duplicate first/last
                            if idx != *start_idx && idx != *end_idx - 1 {
                                visible_indices.push(idx);
                            }
                        }
                    } else {
                        // Too many events, sample them
                        let sample_rate = (count / tolerance).ceil() as usize;
                        for (i, &idx) in indices.iter().enumerate() {
                            if i % sample_rate == 0 && idx != *start_idx && idx != *end_idx - 1 {
                                visible_indices.push(idx);
                            }
                        }
                    }

                    seen_pixels.insert(*pixel);
                }
            }
        }
    }

    // Sort indices to maintain time order
    visible_indices.sort_unstable();
    visible_indices.dedup();

    visible_indices
}
