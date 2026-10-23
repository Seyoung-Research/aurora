# Statistical Distributions (`src/stats/distributions.rs`)

This module provides the numerical foundations for turning test statistics into p-values and calculating confidence intervals. To ensure determinism and zero dependencies, all distributions are implemented from first principles.

## Implemented Distributions

### Normal Distribution
- **CDF/SF**: Implemented via the Abramowitz & Stegun approximation of the error function ($\text{erf}$).
- **PPF (Inverse CDF)**: Calculated via bisection on the accurate CDF.

### Student's t-Distribution
Used for small-sample hypothesis tests.
- **CDF**: Implemented via the regularized incomplete beta function.
- **PPF**: Calculated via bisection.

### F-Distribution
Used primarily for ANOVA.
- **CDF**: Implemented via the regularized incomplete beta function.

### Chi-Square Distribution
Used for tests of independence.
- **CDF**: Implemented via the regularized lower incomplete gamma function.

## Numerical Implementation Details

### Regularized Incomplete Beta ($\text{I}_x(a, b)$)
The engine uses a dual-strategy for the incomplete beta function:
1. **Hypergeometric Power Series**: Used when it converges rapidly (typically for small $x$).
2. **Continued Fractions**: A Lentz-style continued fraction is used as a fallback for larger $x$ or specific parameter ranges.

### Regularized Incomplete Gamma ($\text{P}(a, x)$)
Implemented using a combination of power series (for $x < a + 1$) and continued fractions (for $x \ge a + 1$) to maintain precision across the entire domain.

### Gamma Function ($\Gamma(x)$)
The natural log of the Gamma function is computed using the **Lanczos approximation**, providing accuracy within $\sim 10^{-12}$.
