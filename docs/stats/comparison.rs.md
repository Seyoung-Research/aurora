# Hypothesis Testing (`src/stats/comparison.rs`)

The `comparison` module implements the logic for comparing two or more groups to determine if observed differences are statistically significant.

## T-Tests (Parametric)
Aurora supports three variants of the Student's t-test:
- **Unpaired T-Test**: Compares the means of two independent groups. Supports **Welch's correction** for groups with unequal variances.
- **Paired T-Test**: Compares means from the same group at different times (or matched pairs) by performing a one-sample t-test on the differences.
- **One-Sample T-Test**: Compares a single group mean against a fixed hypothetical value ($\mu$).

## Analysis of Variance (ANOVA)
- **One-Way ANOVA**: Determines if at least one group mean differs among $k$ groups. It returns the F-statistic and p-value.
- **Two-Way ANOVA**: Analyzes the effect of two different factors on a dependent variable, including an interaction term. This implementation requires replication (multiple subjects per cell).

## Non-Parametric Tests
For data that does not follow a normal distribution, Aurora provides rank-based alternatives:
- **Mann-Whitney U**: The non-parametric alternative to the unpaired t-test.
- **Wilcoxon Signed-Rank**: The non-parametric alternative to the paired t-test.
- **Kruskal-Wallis**: The non-parametric alternative to one-way ANOVA.
- **Friedman Test**: The non-parametric alternative to two-way ANOVA for repeated measures.

## Chi-Square Test
- **Independence Test**: Computes the $\chi^2$ statistic for a `ContingencyTable` to determine if two categorical variables are independent.

## Implementation Note: Ranking
Non-parametric tests utilize a custom `rank_with_ties` function. When multiple observations have the same value, they are all assigned the **average of the ranks** they would have otherwise spanned.
