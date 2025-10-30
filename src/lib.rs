//! MouseTesterRust library
//!
//! This library provides modules for processing and analyzing USB mouse movement data
//! captured via pcap. It includes:
//!
//! - `mouse_event`: Data structures and parsers for mouse movement events with pcap timestamps
//! - `lod`: Advanced LOD with time consistency analysis and adaptive segmentation
//!
//! # Example
//!
//! ```rust,ignore
//! use MouseTesterRust::mouse_event::MouseMoveEvent;
//! use MouseTesterRust::lod::{build_segments, collect_visible_indices};
//!
//! // Create or capture events
//! let events: Vec<MouseMoveEvent> = vec![/* ... */];
//!
//! // Build LOD segments for efficient visualization
//! let segments = build_segments(&events, 10, 1.6, 0.98, 0.091);
//!
//! // Collect visible indices for rendering
//! let indices = collect_visible_indices(&segments, &events, 800.0, 600.0,
//!     (0.0, 100.0), (-500.0, 1000.0), 3.0, 1.5);
//! ```

pub mod lod;
pub mod mouse_event;
