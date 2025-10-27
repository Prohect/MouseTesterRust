# LOD Parameter Optimization Results

This document describes the analysis of real mouse capture data and the optimized LOD parameters for MouseTesterRust.

## Test Data Analysis

Analysis was performed on 5 real mouse capture datasets with different configurations:

### Dataset Characteristics

| Configuration | Events | Time Span | Report Rate | Avg Movement | Time Consistency |
|---------------|--------|-----------|-------------|--------------|------------------|
| 20kSensor @ 1kHz | 5,853 | 5.85s | 1000.8 Hz | 301.2 units | 10.9% variation |
| 20kSensor @ 8kHz | 85,752 | 10.74s | 7987.6 Hz | 30.2 units | 8.8% variation |
| LowPower @ 1kHz | 6,710 | 6.71s | 1000.2 Hz | 223.7 units | 9.4% variation |
| CordedGaming @ 4kHz | 23,765 | 5.95s | 3997.6 Hz | 55.5 units | 24.0% variation |
| CordedGaming @ 8kHz | 45,485 | 5.69s | 7994.6 Hz | 28.3 units | 6.4% variation |

### Key Findings

1. **High Report Rate Consistency**: 8kHz sensors show excellent time consistency (6-9% variation) with very small movements per event (28-30 units)

2. **Standard Rate Performance**: 1kHz sensors have good consistency (9-11%) with larger movements per event (223-301 units)

3. **Power-Saving Mode**: LowPower mode maintains good timing consistency (9.4%), contrary to initial concerns

4. **Gaming Sensor Variation**: 4kHz gaming sensor shows higher timing variation (24%), likely due to power-saving features creating irregular reporting

## Optimized GUI LOD Parameters

Based on the analysis, the GUI LOD has been optimized with the following parameters:

### Density-Based Thresholds

```rust
MIN_POINTS_PER_PIXEL: 0.5      // For very high density (8kHz+ fully zoomed out)
MAX_POINTS_PER_PIXEL: 3.0      // For low density (zoomed in detail)
HIGH_DENSITY_THRESHOLD: 5.0    // Threshold for aggressive reduction
```

### Adaptive Strategy

The GUI uses a multi-tier adaptive strategy based on data density (visible_events / visible_width):

| Data Density | Points per Pixel | Use Case |
|--------------|------------------|----------|
| > 5.0 | 0.5 | Very high rate (8kHz+) fully zoomed out |
| 3.0 - 5.0 | 1.0 | High density, normal zoom out |
| 1.0 - 3.0 | 1.0 - 3.0 (scaled) | Medium density, typical viewing |
| < 1.0 | All points | Low density, zoomed in detail |

### Performance Characteristics

For typical scenarios:

- **8kHz @ 2300px width fully zoomed out**: 
  - 85,752 events → ~1,150 points (98.7% reduction)
  - Uses 0.5 points/pixel

- **1kHz @ 2300px width fully zoomed out**:
  - 5,853 events → ~2,300 points (60.7% reduction)
  - Uses 1.0 points/pixel

- **4kHz @ 2300px width zoomed in 50%**:
  - Visible: 12,000 events → ~4,600 points (61.7% reduction)
  - Uses ~2.0 points/pixel

## Offline LOD Module Parameters

For the tree-based LOD module (src/lod.rs), recommended parameters based on real-world testing:

### High Report Rate Devices (8kHz+)

```rust
min_pts: 10          // Prevent over-segmentation
max_pts: 1000        // Balance tree depth
tol_px: 0.5-1.0      // Maintain detail (build tolerance)
view_tol: 2.0-5.0    // View tolerance for 90-99% reduction
px_scale: 1.0        // Standard scale
```

**Performance**: At 5px view tolerance, achieves 99% reduction (85,752 → ~858 points).

**Rationale**: High rate devices have small movements per event. Higher min_pts prevents creating too many tiny segments. Use high view tolerances for aggressive reduction when zoomed out.

### Standard Devices (1-4kHz)

```rust
min_pts: 5-7         // Balanced segmentation  
max_pts: 1000        // Standard tree depth
tol_px: 1.0-1.5      // Good quality/performance (build tolerance)
view_tol: 2.0-5.0    // View tolerance for 40-96% reduction
px_scale: 1.0        // Standard scale
```

**Performance**: At 5px view tolerance, achieves 46-96% reduction depending on movement patterns.

**Rationale**: Balanced approach for typical mouse usage patterns. Lower min_pts accommodates larger movements per event.

### Power-Saving Modes

```rust
min_pts: 5           // Accommodate timing gaps
max_pts: 1000        // Standard depth
tol_px: 1.5-2.0      // Tolerance for irregularities (build tolerance)
view_tol: 2.0-5.0    // View tolerance
px_scale: 1.0        // Standard scale
```

**Performance**: Similar to standard devices (40-46% reduction).

**Rationale**: Lower min_pts and higher tolerance accommodate potential timing irregularities from power-saving features. Analysis shows LowPower mode is actually quite consistent, so aggressive parameter tuning isn't necessary.

## Implementation Notes

### GUI LOD (src/gui.rs)

The GUI uses an inline bucket-based LOD that:
- Calculates data density dynamically based on zoom level
- Applies aggressive reduction for high-density scenarios (8kHz sensors)
- Preserves detail when zoomed in (< 1 event per pixel)
- Includes min/max extrema in each bucket to preserve trend information

### Offline LOD (src/lod.rs)

The tree-based LOD is designed for:
- Static plotting after capture is complete
- Building hierarchical representations once
- Querying at different tolerance levels for export or multi-scale visualization
- CPU-efficient rendering of very large datasets

## Testing Methodology

The analysis tool (`examples/lod_analysis.rs`) evaluates:

1. **Time Consistency**: Measures standard deviation of inter-event timing
   - Lower % = more consistent timing
   - Identifies power-saving artifacts

2. **Movement Patterns**: Analyzes movement magnitude distributions
   - High rate → small movements per event
   - Low rate → larger movements per event

3. **LOD Effectiveness**: Tests multiple parameter combinations
   - Measures reduction ratios at different view tolerances
   - Validates quality/performance trade-offs

## Recommendations for Future Work

1. **Adaptive min_pts**: Consider dynamically adjusting min_pts based on detected report rate

2. **Movement-Based LOD**: For high movement scenarios, consider movement magnitude in LOD decisions

3. **Timing Gap Detection**: For power-saving modes, detect and handle timing gaps specially

4. **Per-Sensor Profiles**: Create LOD profiles that can be selected based on detected sensor characteristics

## Running the Analysis

To reproduce this analysis on your own capture data:

```bash
# Run the analysis tool
cargo run --example lod_analysis

# Or run with custom CSV files
# Place CSV files in examples/test/ directory
# Format: dx,dy,time (with header)
```

## References

- Test data location: `examples/test/*.csv`
- Analysis tool: `examples/lod_analysis.rs`
- GUI LOD implementation: `src/gui.rs` (line 91-220)
- Offline LOD implementation: `src/lod.rs`

---

*Analysis performed: 2025-10-26*  
*Test datasets: 5 real mouse captures (167,565 total events)*  
*Report rates tested: 1kHz, 4kHz, 8kHz*  
*Sensor modes: Standard, LowPower, Gaming*
