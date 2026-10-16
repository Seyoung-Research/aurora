# Technical Specification: Aurora (Rust)

## 1. Data Model: Tables
Aurora uses a tightly coupled table-analysis-graph pipeline. Four primary table types are supported.

### 1.1 XY Table
- **Purpose**: Independent variable (X) vs. Dependent variable (Y).
- **Structure**: 
    - X-axis: Single column of values.
    - Y-axis: One or more columns for replicates.
- **Primary Analyses**: Nonlinear regression, Linear regression, AUC, Integration.

### 1.2 Column Table
- **Purpose**: Comparing groups defined by a single categorical variable.
- **Structure**: 
    - Multiple columns, each containing replicates for a group.
- **Primary Analyses**: t-tests, One-way ANOVA, Non-parametric tests.

### 1.3 Grouped Table
- **Purpose**: Comparing groups defined by two categorical variables.
- **Structure**: 
    - Row labels (X-axis).
    - Grouped columns (each group has N replicates).
- **Primary Analyses**: Two-way ANOVA, Mixed-model ANOVA.

### 1.4 Contingency Table
- **Purpose**: Frequency counts for categorical data.
- **Structure**: 
    - Matrix of integers (counts).
- **Primary Analyses**: Chi-square test, Fisher's exact test.

---

## 2. Analysis Catalog
All analyses must be deterministic and provide detailed reporting (p-values, confidence intervals, effect sizes).

### 2.1 Descriptive Statistics
- Metrics: Mean, Standard Deviation (SD), Standard Error of the Mean (SEM), Median, Min, Max, Range, N.

### 2.2 Comparative Statistics
- **T-tests**: Unpaired, Paired, One-sample.
- **ANOVA**: One-way, Two-way, Mixed, Repeated Measures.
- **Non-parametric**: Mann-Whitney, Wilcoxon, Kruskal-Wallis, Friedman.

### 2.3 Regression & Curve Fitting
- **Linear**: Simple Linear, Deming Regression.
- **Nonlinear**: 
    - Sigmoidal Dose-Response (variable slope).
    - Exponential Growth/Decay.
    - Michaelis-Menten.
    - Gompertz.
- **Curve Utils**: Spline, LOWESS, Area Under Curve (AUC).

---

## 3. Graphing Specification
Visualizations must be vector-based and highly customizable.

### 3.1 Axis Logic
- **Scales**: Linear, Log10, Log(Y/X), Semi-log.
- **Constraints**: Fixed ranges, Automatic scaling, Axis breaks.

### 3.2 Plot Types
- **Scatter**: Points with optional connecting lines.
- **Bar**: Mean $\pm$ Error bar.
- **Box**: Median, Quartiles, Whiskers.
- **Column**: Individual replicates plotted as points over bars.

### 3.3 Error Bars
- Sources: SD, SEM, or User-defined CI.
- Rendering: Symmetrical or Asymmetrical.

---

## 4. Proposed Architecture

### 4.1 Layered Design
1. **Data Layer**: `Table` trait and implementations. Handles serialization (CSV, XLSX).
2. **Analysis Layer**: `Analysis` trait. Takes `Table` as input $\rightarrow$ `AnalysisResult`.
3. **Rendering Layer**: Translates `AnalysisResult` and `Table` into visual primitives.
4. **UI Layer**: State-driven interface for table entry and graph tuning.

### 4.2 Tech Stack
- **Language**: Rust (Stability, Performance).
- **Math**: `ndarray` (Multidimensional arrays), `nalgebra` (Linear algebra), `argmin` (Optimization for nonlinear fitting).
- **Rendering**: `plotters` (Static/Vector) or `vello`/`wgpu` (High-perf interactive).
- **GUI**: `egui` (Immediate mode, fast iteration, scientific look).
- **Persistence**: JSON or SQLite for project files.
