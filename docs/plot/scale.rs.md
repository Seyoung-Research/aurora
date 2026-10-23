# Axis Scaling and Ticks (`src/plot/scale.rs`)

The `scale` module handles the mapping between raw data values and screen coordinates, ensuring that axes look professional and are easy to read.

## Scaling Logic

Aurora supports two types of scales:
- **Linear**: Standard linear mapping.
- **Log10**: Mapping via $\log_{10}(\text{value})$. Values $\le 0$ are treated as invalid and not plotted.

### Coordinate Transformation
The `transform` function converts a raw data value into "drawing space". This is crucial for log scales, as the renderer works in the transformed space to ensure that ticks are placed logically.

## "Nice" Tick Generation

To avoid awkward tick labels (like `1.2345`), Aurora implements a "nice number" algorithm.

### Nice Step Selection
The `nice_step` function selects a step size from the 1-2-5 progression (e.g., $0.1, 0.2, 0.5, 1, 2, 5, \dots$) that best fits the requested number of ticks.

### Range Expansion
The `nice_range` function expands the actual data extent to the nearest "nice" boundaries. This ensures that the chart doesn't end abruptly in the middle of a tick interval.

### Tick Formatting
The `format_tick` function converts numerical values into human-readable strings, avoiding scientific notation for common values while maintaining precision for very small or large numbers.
