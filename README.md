# Aurora

A scientific statistics and graphing desktop app written in Rust. It has four table types, a statistics engine (t tests, ANOVA,
non-parametric tests, contingency tests, curve fitting, regression), and a
publication-style graph renderer with PNG export all in a native egui desktop
app.

```
git clone https://github.com/Seyoung-Research/aurora.git
cd aurora
cargo run                 # start the desktop app
cargo run --release       # same, but faster graph rendering
cargo test                # 99 unit + headless-UI tests
cargo run -- --selftest   # build the demo project, run every analysis and
                          # render every graph without opening a window
```

On macOS, `Cargo.toml` disables debug assertions for the `objc2` crate only.
That crate (pulled in by winit through eframe) otherwise aborts on a benign
Objective-C return-type check while winit enumerates screens, which prevented
the window from opening in debug builds. Nothing else is affected — tests still
run with debug assertions enabled.

## First steps in the app

1. **File ▸ Load demo project** — a four-table project with eight analyses and
   six graphs, useful for exploring every screen.
2. Or **File ▸ Import CSV…** / **Table ▸ New … table** to start from your data.
3. The left sidebar is the project tree: tables, the analyses of the selected
   table, and graphs. The tabs above the sidebar switch the main area between
   **Data**, **Results** and **Graph**.
4. **File ▸ Save / Open** write the whole project (tables, analyses, graphs) as
   JSON. **⌘S** and **⌘O** work as shortcuts.

### Data tab

One editor per table type: XY (X rows × replicate columns), Column (one column
per group), Grouped (rows × groups × replicates) and Contingency (counts).
Every editor can add/remove rows, groups/columns and replicates, rename things,
and shows live values in drag fields.

### Results tab

* **Add analysis** offers only the analyses that apply to the selected table
  type; **Run all** re-runs them.
* Selecting an analysis shows its options (t-test kind, compared groups, fit
  model, Deming lambda, LOWESS fraction) and its full result table.
* **Graph** creates a graph pre-configured for that analysis.
* Results are recomputed from the table whenever you change the data.

### Graph tab

Pick a graph, set title and axis labels, style (points, lines, bars, boxes,
jittered columns), error bars (SD, SEM, 95% CI, range), log X/Y, legend, fit
curve, per-replicate series and jitter. The image is rendered live at the size
of the panel; **Axis details** reports the exact ranges and tick labels that
were chosen. **Export PNG** writes the graph at the pixel size you set.

## Analyses

| Table | Analyses |
| --- | --- |
| Column | descriptive statistics, unpaired/paired/one-sample t tests, Welch t test, one-way ANOVA with Bonferroni post-hoc, Mann-Whitney, Wilcoxon signed rank, Kruskal-Wallis, Friedman |
| XY | descriptive statistics, simple linear regression, Deming regression, nonlinear fits (4PL dose-response, exponential growth, exponential decay 2- and 3-parameter, Michaelis-Menten, Gompertz, logarithmic, power), area under the curve, LOWESS, cubic spline |
| Grouped | descriptive statistics, two-way ANOVA (both factors + interaction) |
| Contingency | chi-square test of independence (with expected counts and Cramér's V) |

Graphs overlay the fitted line/curve of the analysis they reference.

## CSV formats

Comma separated, with a header row for CSV types that have named columns.
Extra columns are allowed; a leading index column is ignored on import.

```
XY            X,Y1,Y2          Column        Control,Treated
              0.1,1.2,1.3                     1.0,4.0
              1,5.5,5.8                       1.2,4.4

Grouped       ,PBS,PBS,Drug A,Drug A          Contingency   ,Yes,No
              Day 1,1,1.1,3,3.1                             Treated,80,20
              Day 2,2,2.1,4,4.2                             Control,30,70
```

Grouped files repeat the group name once per replicate; the number of
repetitions decides how many replicate columns that group has. Import mistakes
(such as ragged rows) are reported in the status bar instead of panicking.
**File ▸ Export table as CSV…** writes the selected table back out in the same
layout.

## Layout

```
src/stats/     descriptive stats, t tests, ANOVA, non-parametric and
               contingency tests, regression and curve fitting, plus
               self-contained distribution functions (no external stats crate)
src/plot/      chart model, scales/ticks, the pixel renderer built on plotters
               low-level primitives, and a small PNG encoder (store deflate)
src/data/      table types, project model, JSON persistence, CSV import/export
src/analysis.rs  analysis specifications, results, and graph definitions
src/ui/        the egui app: project tree, table editors, analysis panel,
               graph panel, demo project
```

See `SPEC.md` and `ARCHITECTURE.md` for the intended feature set and the module
layout, and `samples/` for PNGs produced by the renderer
(`cargo run -- --selftest --write-samples samples/selftest` regenerates them,
including `demo_project.json`, which can be opened in the app).

## Known limitations

* Contingency tables use the chi-square test of independence; Fisher's exact
  test is not implemented.
* Post-hoc comparisons after ANOVA are Bonferroni-adjusted pairwise Welch
  t tests.
* Non-parametric p values (Mann-Whitney, Wilcoxon, Kruskal-Wallis, Friedman)
  come from the normal approximation with continuity and tie corrections
  rather than exact tables.
* The chart text is drawn with the system sans-serif font and is best-effort:
  if no font can be resolved the chart still renders, without labels.
* Graphs are not editable object-by-object (no manual axis limits, per-point
  labels or error-bar selection per series) and there is no undo.
* File dialogs are simple path fields with a list of the files in the working
  directory, not native pickers.
