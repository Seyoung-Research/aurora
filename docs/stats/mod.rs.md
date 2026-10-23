# Statistical Engine (`src/stats/mod.rs`)

The `stats` module is the mathematical core of Aurora. It is designed to be entirely dependency-free for its core logic, ensuring that results are deterministic and self-contained.

The module is divided into four specialized areas:
- **`descriptive`**: Basic sample statistics.
- **`comparison`**: Hypothesis tests (T-tests, ANOVA, Non-parametric).
- **`regression`**: Linear and non-linear curve fitting.
- **`distributions`**: Numerical implementations of probability distributions.
