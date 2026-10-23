# Descriptive Statistics (`src/stats/descriptive.rs`)

The `descriptive` module provides a comprehensive set of summary statistics for a single sample of numerical data.

## The `DescriptiveStats` Struct

For any given sample, Aurora computes a wide array of metrics:
- **Central Tendency**: Mean, Median, Mode, and Geometric Mean.
- **Dispersion**: Sample Standard Deviation (using $n-1$), Variance, and Range.
- **Relative Dispersion**: Coefficient of Variation ($\text{CV} = \frac{\text{SD}}{\text{mean}} \times 100\%$).
- **Quantiles**: Min, Max, Q1 (25th percentile), Q3 (75th percentile), and Interquartile Range (IQR).
- **Precision**: Standard Error of the Mean (SEM) and the 95% Confidence Interval (CI).

## Implementation Details

### Quantile Interpolation
The `quantile` function implements a standard interpolation convention to estimate values at specific percentiles, ensuring consistency across different sample sizes.

### Confidence Intervals
The 95% confidence interval is calculated using the Student's t-distribution:
$$\text{CI} = \text{mean} \pm (t_{0.025, df} \times \text{SEM})$$
where $df = n - 1$.

### Column-wise Processing
The `descriptive_by_columns` helper allows the system to process an entire `ColumnTable` at once, returning a list of `DescriptiveStats` for every group in the table.
