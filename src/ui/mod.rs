//! The egui desktop application: menus, project tree, editors, results and
//! graphs.
//!
//! The module is split into panels, each owning its part of the UI:
//!
//! - [`analysis_panel`] — the analyses list, the add/run loop and results
//!   rendering.
//! - [`demo`] — the demo project used by the menu, the self-test and the docs.
//! - [`graph_panel`] — the graph viewer, live rendering and PNG export.
//! - [`table_editor`] — editable views for every table type.
//!
//! The [`App`] struct ties the panels together. This module is mostly concerned
//! with egui wiring; the data model, statistics and chart builders live in the
//! sibling modules (`data`, `stats`, `plot`, `analysis`).

pub mod analysis_panel;
pub mod demo;
pub mod graph_panel;
pub mod table_editor;
pub mod theme;

use eframe::egui;
use egui::{Color32, RichText, Ui};

use crate::analysis::{AnalysisSpec, Graph};
use crate::data::csv_import;
use crate::data::persistence::{load_project, save_project};
use crate::data::{Project, TableFlavor, TableInstance};
use crate::plot::{ErrorKind, PlotKind, SeriesStyle};

use graph_panel::{ExportRequest, TexCache};

/// Which modal dialog is open.
#[derive(Clone, Debug)]
enum Dialog {
    /// No dialog open.
    None,
    /// Dialog for creating a new table.
    NewTable,
    /// Dialog for opening a project file.
    Open,
    /// Dialog for choosing a file name to save to.
    SaveAs,
    /// Dialog for importing a CSV file.
    Import,
    /// Dialog for exporting a graph as PNG.
    Export(ExportRequest),
    /// Dialog for exporting a table as CSV.
    ExportTable { path: String },
    /// About dialog.
    About,
    /// Dialog listing the supported CSV import/export formats.
    Formats,
}

/// Which tab the central panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    /// Data tables browser / editor.
    Data,
    /// Analyses and their results.
    Results,
    /// Graph viewer / editor.
    Graph,
}

impl Tab {
    /// Short label of the tab, used for the tab bar buttons.
    fn label(&self) -> &'static str {
        match self {
            Tab::Data => "Data",
            Tab::Results => "Results",
            Tab::Graph => "Graph",
        }
    }
}

/// The egui desktop application: owns the project, the current tab, the
/// selected table/analysis/graph and the state of the modal dialogs.
pub struct App {
    /// The project currently being edited (tables, analyses, graphs).
    project: Project,
    /// Path of the currently open project file, if any.
    path: Option<String>,
    /// Whether the project has unsaved changes.
    dirty: bool,
    /// Which tab the central panel shows.
    tab: Tab,
    /// Light/dark UI theme (toggled from the Settings menu).
    dark: bool,
    /// Index of the selected table in the project.
    selected_table: Option<usize>,
    /// Index of the selected analysis in the project.
    selected_analysis: Option<usize>,
    /// Index of the selected graph in the project.
    selected_graph: Option<usize>,
    /// Which modal dialog, if any, is open.
    dialog: Dialog,
    /// Name to give a newly created table.
    new_table_name: String,
    /// Default flavor used when creating a new table via the dialog.
    new_table_flavor: TableFlavor,
    /// Text field content of the "save as" dialog.
    path_input: String,
    /// Flavor chosen for a CSV import.
    import_flavor: TableFlavor,
    /// One-line status message shown in the status bar.
    status: String,
    /// Bumped whenever data/options change so graphs re-render.
    revision: u64,
    cache: TexCache,
    /// List of files found in the working directory (for the open dialog).
    listing: Vec<String>,
    /// Last window title pushed to the viewport (avoids re-sending it).
    last_title: String,
}

impl App {
    /// Create a new app instance and apply the initial theme.
    ///
    /// Also honours the command-line flags handled by the binary (e.g.
    /// `--open <file>` and `--tab <data|results|graph>`).
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            project: Project::new("Untitled project"),
            path: None,
            dirty: false,
            tab: Tab::Data,
            dark: false,
            selected_table: None,
            selected_analysis: None,
            selected_graph: None,
            dialog: Dialog::None,
            new_table_name: String::new(),
            new_table_flavor: TableFlavor::Column,
            path_input: String::new(),
            import_flavor: TableFlavor::Column,
            status: "Ready. Add a table, import a CSV, or load the demo project.".into(),
            revision: 0,
            cache: TexCache::default(),
            listing: Vec::new(),
            last_title: String::new(),
        };
        apply_theme(&cc.egui_ctx, app.dark);
        app.refresh_listing();

        // `--open <file>` and `--tab <data|results|graph>` on the command line.
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--open" || a == "-o") {
            if let Some(p) = args.get(i + 1) {
                app.open_path(p);
            }
        }
        if let Some(i) = args.iter().position(|a| a == "--tab") {
            match args.get(i + 1).map(|s| s.to_lowercase()).as_deref() {
                Some("graph") => app.tab = Tab::Graph,
                Some("results") => app.tab = Tab::Results,
                Some("data") => app.tab = Tab::Data,
                _ => {}
            }
        }
        app
    }

    // -- file handling ----------------------------------------------------- //

    /// Scan the working directory for `.json` / `.csv` files to offer in the
    /// open dialog.
    fn refresh_listing(&mut self) {
        let mut found = Vec::new();
        if let Ok(rd) = std::fs::read_dir(".") {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let lower = name.to_lowercase();
                if lower.ends_with(".json") || lower.ends_with(".csv") {
                    found.push(name);
                }
            }
        }
        found.sort();
        found.truncate(80);
        self.listing = found;
    }

    /// Open a project file, replacing the current project.
    fn open_path(&mut self, path: &str) {
        match load_project(path) {
            Ok(p) => {
                let n_tables = p.tables.len();
                let n_analyses = p.analyses.len();
                let n_graphs = p.graphs.len();
                self.project = p;
                self.path = Some(path.to_string());
                self.dirty = false;
                self.selected_table = if n_tables > 0 { Some(0) } else { None };
                self.selected_analysis = None;
                self.selected_graph = if n_graphs > 0 { Some(0) } else { None };
                self.cache.clear();
                self.revision += 1;
                self.status = format!(
                    "Opened {path}: {n_tables} table(s), {n_analyses} analysis(es), {n_graphs} graph(s)"
                );
            }
            Err(e) => self.status = format!("Error opening {path}: {e}"),
        }
    }

    /// Save the current project to `path`.
    fn save_to(&mut self, path: &str) {
        match save_project(&self.project, path) {
            Ok(()) => {
                self.path = Some(path.to_string());
                self.dirty = false;
                self.status = format!("Saved {path}");
            }
            Err(e) => self.status = format!("Error saving {path}: {e}"),
        }
    }

    /// Save the current project; if it has no path yet, open the save dialog
    /// using a default file name.
    fn save_current(&mut self) {
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => {
                self.path_input = format!(
                    "{}.json",
                    slug(&self.project.name)
                );
                self.refresh_listing();
                self.dialog = Dialog::SaveAs;
            }
        }
    }

    /// Load the demo project (used by the File menu and `--selftest`).
    fn load_demo(&mut self) {
        self.project = demo::demo_project();
        self.path = None;
        self.dirty = true;
        self.selected_table = Some(0);
        self.selected_analysis = None;
        self.selected_graph = Some(0);
        self.cache.clear();
        self.revision += 1;
        self.tab = Tab::Graph;
        self.status = format!(
            "Loaded demo project: {} tables, {} analyses, {} graphs",
            self.project.tables.len(),
            self.project.analyses.len(),
            self.project.graphs.len()
        );
    }

    /// Create a new table of the given flavor and select it.
    fn new_table(&mut self, flavor: TableFlavor) {
        let name = if self.new_table_name.trim().is_empty() {
            format!("{} table {}", flavor.label(), self.project.tables.len() + 1)
        } else {
            self.new_table_name.trim().to_string()
        };
        let idx = self.project.add_table(new_table_skeleton(flavor, name));
        self.selected_table = Some(idx);
        self.tab = Tab::Data;
        self.dirty = true;
        self.revision += 1;
        self.status = format!("Added a new {} table", flavor.label());
    }

    /// Import a CSV file of the given flavor and select the new table.
    fn import_csv(&mut self, path: &str, flavor: TableFlavor) {
        match csv_import::import_table(path, flavor) {
            Ok(t) => {
                let name = t.as_table().name().to_string();
                let idx = self.project.add_table(t);
                self.selected_table = Some(idx);
                self.tab = Tab::Data;
                self.dirty = true;
                self.revision += 1;
                self.status = format!("Imported '{name}' from {path}");
            }
            Err(e) => self.status = format!("Import failed: {e}"),
        }
    }

    // -- panels ------------------------------------------------------------ //

    /// Draw the egui menu bar (File, Table, Analyses, Graph, Settings, Help).
    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New project").clicked() {
                        self.project = Project::new("Untitled project");
                        self.path = None;
                        self.dirty = false;
                        self.selected_table = None;
                        self.selected_analysis = None;
                        self.selected_graph = None;
                        self.cache.clear();
                        self.revision += 1;
                        self.status = "New project".into();
                        ui.close_menu();
                    }
                    if ui.button("Open…").clicked() {
                        self.dialog = Dialog::Open;
                        self.path_input = self.path.clone().unwrap_or_default();
                        self.refresh_listing();
                        ui.close_menu();
                    }
                    if ui.button("Save").clicked() {
                        self.save_current();
                        ui.close_menu();
                    }
                    if ui.button("Save as…").clicked() {
                        self.path_input = self
                            .path
                            .clone()
                            .unwrap_or_else(|| format!("{}.json", slug(&self.project.name)));
                        self.refresh_listing();
                        self.dialog = Dialog::SaveAs;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Import CSV…").clicked() {
                        self.dialog = Dialog::Import;
                        self.refresh_listing();
                        ui.close_menu();
                    }
                    if ui.button("Export table as CSV…").clicked() {
                        match self.selected_table.and_then(|i| self.project.tables.get(i)) {
                            Some(t) => {
                                self.dialog = Dialog::ExportTable {
                                    path: format!("{}.csv", slug(t.as_table().name())),
                                };
                            }
                            None => self.status = "Select a table first".into(),
                        }
                        ui.close_menu();
                    }
                    if ui.button("Load demo project").clicked() {
                        self.load_demo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Table", |ui| {
                    if ui.button("New table…").clicked() {
                        self.new_table_name = format!(
                            "{} table {}",
                            self.new_table_flavor.label(),
                            self.project.tables.len() + 1
                        );
                        self.dialog = Dialog::NewTable;
                        ui.close_menu();
                    }
                    ui.separator();
                    for f in TableFlavor::all() {
                        if ui.button(format!("New {} table", f.label())).clicked() {
                            self.new_table(f);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Import CSV…").clicked() {
                        self.dialog = Dialog::Import;
                        self.refresh_listing();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Analyses", |ui| {
                    let flavor = self
                        .selected_table
                        .and_then(|i| self.project.tables.get(i))
                        .map(|t| t.flavor());
                    match flavor {
                        Some(f) => {
                            ui.label(RichText::new(format!("for the selected {} table", f.label())).weak());
                            let n_groups = self
                                .selected_table
                                .and_then(|i| self.project.tables.get(i))
                                .map(crate::analysis::table_group_count)
                                .unwrap_or(2);
                            for spec in AnalysisSpec::menu_for(f, n_groups) {
                                if ui.button(spec.label()).clicked() {
                                    if let Some(t) = self.selected_table {
                                        let idx = self
                                            .project
                                            .add_analysis(crate::analysis::Analysis::new(spec, t));
                                        let _ = self.project.run_analysis(idx);
                                        self.selected_analysis = Some(idx);
                                        self.tab = Tab::Results;
                                        self.dirty = true;
                                        self.revision += 1;
                                    }
                                    ui.close_menu();
                                }
                            }
                        }
                        None => {
                            ui.label(RichText::new("Select a table first").weak());
                        }
                    }
                    ui.separator();
                    if ui.button("Run all analyses").clicked() {
                        self.project.run_all_analyses();
                        self.revision += 1;
                        self.status = "Ran every analysis".into();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Graph", |ui| {
                    if ui.button("New graph of selected table").clicked() {
                        if let Some(i) = self.selected_table {
                            if let Some(t) = self.project.tables.get(i) {
                                let flavor = t.flavor();
                                let (kind, style, error) = match flavor {
                                    TableFlavor::Xy => (
                                        PlotKind::Xy,
                                        SeriesStyle::LinesAndPoints,
                                        ErrorKind::Sem,
                                    ),
                                    TableFlavor::Contingency => (
                                        PlotKind::GroupedBar,
                                        SeriesStyle::Bars,
                                        ErrorKind::None,
                                    ),
                                    _ => (PlotKind::Bar, SeriesStyle::Bars, ErrorKind::Sem),
                                };
                                let g = Graph {
                                    name: format!("Graph {}", self.project.graphs.len() + 1),
                                    table: Some(i),
                                    analysis: None,
                                    plot_kind: kind,
                                    style,
                                    error,
                                    x_log: false,
                                    y_log: false,
                                    title: t.as_table().name().to_string(),
                                    x_label: String::new(),
                                    y_label: String::new(),
                                    show_fit: true,
                                    ..Default::default()
                                };
                                self.project.add_graph(g);
                                self.selected_graph = Some(self.project.graphs.len() - 1);
                                self.tab = Tab::Graph;
                                self.dirty = true;
                                self.revision += 1;
                            }
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Export current graph as PNG…").clicked() {
                        if let Some(i) = self.selected_graph {
                            let name = self
                                .project
                                .graphs
                                .get(i)
                                .map(|g| g.name.clone())
                                .unwrap_or_else(|| "graph".into());
                            let (w, h) = self
                                .project
                                .graphs
                                .get(i)
                                .map(|g| (g.width, g.height))
                                .unwrap_or((1000, 700));
                            self.dialog = Dialog::Export(ExportRequest {
                                graph: i,
                                path: format!("{}.png", slug(&name)),
                                width: w,
                                height: h,
                            });
                        }
                        ui.close_menu();
                    }
                });

                ui.menu_button("Settings", |ui| {
                    let label = if self.dark {
                        "Switch to light theme"
                    } else {
                        "Switch to dark theme"
                    };
                    if ui.button(label).clicked() {
                        self.dark = !self.dark;
                        apply_theme(ctx, self.dark);
                        self.status = format!(
                            "{} theme",
                            if self.dark { "Dark" } else { "Light" }
                        );
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("CSV formats").clicked() {
                        self.dialog = Dialog::Formats;
                        ui.close_menu();
                    }
                    if ui.button("About").clicked() {
                        self.dialog = Dialog::About;
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_value(&mut self.tab, Tab::Graph, Tab::Graph.label())
                        .clicked()
                    {
                        if self.selected_graph.is_none() && !self.project.graphs.is_empty() {
                            self.selected_graph = Some(0);
                        }
                    }
                    if ui
                        .selectable_value(&mut self.tab, Tab::Results, Tab::Results.label())
                        .clicked()
                    {}
                    if ui
                        .selectable_value(&mut self.tab, Tab::Data, Tab::Data.label())
                        .clicked()
                    {}
                });
            });
        });
    }

    /// Draw the left-hand project tree panel (tables, analyses, graphs).
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("project_tree")
            .default_width(300.0)
            .width_range(220.0..=460.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Project").strong());
                    if self.dirty {
                        ui.label(RichText::new("•").color(Color32::from_rgb(200, 120, 0)));
                    }
                });
                ui.label(
                    RichText::new(
                        self.path
                            .clone()
                            .unwrap_or_else(|| "(not saved yet)".into()),
                    )
                    .weak()
                    .small(),
                );
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_source("tree_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // ---- tables -------------------------------------- //
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Tables").strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.menu_button("+", |ui| {
                                        for f in TableFlavor::all() {
                                            if ui.button(format!("{} table", f.label())).clicked()
                                            {
                                                self.new_table(f);
                                                ui.close_menu();
                                            }
                                        }
                                    });
                                },
                            );
                        });
                        if self.project.tables.is_empty() {
                            ui.label(RichText::new("no tables yet").weak().small());
                        }
                        for (i, t) in self.project.tables.iter().enumerate() {
                            let label = format!(
                                "{}   [{}]",
                                t.as_table().name(),
                                t.flavor().label()
                            );
                            if ui
                                .selectable_label(self.selected_table == Some(i), label)
                                .clicked()
                            {
                                self.selected_table = Some(i);
                                self.tab = Tab::Data;
                            }
                        }

                        // ---- analyses ------------------------------------ //
                        ui.add_space(8.0);
                        ui.label(RichText::new("Analyses").strong());
                        let mine: Vec<usize> = self
                            .selected_table
                            .map(|t| self.project.analyses_for_table(t))
                            .unwrap_or_default();
                        let shown = mine.len();
                        for i in mine {
                            let a = &self.project.analyses[i];
                            let headline = a
                                .result
                                .as_ref()
                                .map(|r| r.headline())
                                .unwrap_or_else(|| "error".into());
                            let label = format!("{}  ({})", a.name(), headline);
                            if ui
                                .selectable_label(self.selected_analysis == Some(i), label)
                                .clicked()
                            {
                                self.selected_analysis = Some(i);
                                self.tab = Tab::Results;
                            }
                        }
                        if shown == 0 {
                            ui.label(
                                RichText::new("none for the selected table")
                                    .weak()
                                    .small(),
                            );
                        }

                        // ---- graphs -------------------------------------- //
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Graphs").strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("+").clicked() {
                                        if let Some(i) = self.selected_table {
                                            if let Some(t) = self.project.tables.get(i) {
                                                let flavor = t.flavor();
                                                let (kind, style, error) = match flavor {
                                                    TableFlavor::Xy => (
                                                        PlotKind::Xy,
                                                        SeriesStyle::LinesAndPoints,
                                                        ErrorKind::Sem,
                                                    ),
                                                    TableFlavor::Contingency => (
                                                        PlotKind::GroupedBar,
                                                        SeriesStyle::Bars,
                                                        ErrorKind::None,
                                                    ),
                                                    _ => (
                                                        PlotKind::Bar,
                                                        SeriesStyle::Bars,
                                                        ErrorKind::Sem,
                                                    ),
                                                };
                                                self.project.add_graph(Graph {
                                                    name: format!(
                                                        "Graph {}",
                                                        self.project.graphs.len() + 1
                                                    ),
                                                    table: Some(i),
                                                    analysis: None,
                                                    plot_kind: kind,
                                                    style,
                                                    error,
                                                    x_log: false,
                                                    y_log: false,
                                                    title: t.as_table().name().to_string(),
                                                    x_label: String::new(),
                                                    y_label: String::new(),
                                                    show_fit: true,
                                                    ..Default::default()
                                                });
                                                self.selected_graph =
                                                    Some(self.project.graphs.len() - 1);
                                                self.tab = Tab::Graph;
                                                self.dirty = true;
                                                self.revision += 1;
                                            }
                                        }
                                    }
                                },
                            );
                        });
                        if self.project.graphs.is_empty() {
                            ui.label(RichText::new("no graphs yet").weak().small());
                        }
                        for (i, g) in self.project.graphs.iter().enumerate() {
                            let label = if g.name.is_empty() {
                                format!("Graph {}", i + 1)
                            } else {
                                g.name.clone()
                            };
                            if ui
                                .selectable_label(self.selected_graph == Some(i), label)
                                .clicked()
                            {
                                self.selected_graph = Some(i);
                                self.tab = Tab::Graph;
                            }
                        }
                    });

                ui.separator();
                if self.project.tables.is_empty() {
                    ui.label(
                        RichText::new(
                            "Tip: press 'Load demo project' in the File menu to explore a full example.",
                        )
                        .weak()
                        .small(),
                    );
                    if ui.button("Load demo project").clicked() {
                        self.load_demo();
                    }
                }
            });
    }

    /// Draw the central panel: the selected table editor / analyses / graph.
    fn central(&mut self, ui: &mut Ui) {
        match self.tab {
            Tab::Data => match self.selected_table {
                Some(i) if i < self.project.tables.len() => {
                    let changed = match self.project.tables.get_mut(i) {
                        Some(t) => table_editor::table_editor(ui, t),
                        None => false,
                    };
                    if changed {
                        self.dirty = true;
                        self.revision += 1;
                    }
                }
                _ => {
                    ui.add_space(20.0);
                    ui.label("No table selected.");
                    ui.label(
                        RichText::new(
                            "Create one from the Table menu, import a CSV from the File menu, or load the demo project.",
                        )
                        .weak(),
                    );
                    if ui.button("Load demo project").clicked() {
                        self.load_demo();
                    }
                }
            },
            Tab::Results => {
                let table_index = self.selected_table.unwrap_or(0);
                let mut revision = self.revision;
                let changed = {
                    let Project {
                        tables,
                        analyses,
                        graphs,
                        ..
                    } = &mut self.project;
                    analysis_panel::analysis_panel(
                        ui,
                        tables,
                        analyses,
                        graphs,
                        table_index,
                        &mut self.selected_analysis,
                        &mut revision,
                    )
                };
                self.revision = revision;
                if changed {
                    self.dirty = true;
                }
            }
            Tab::Graph => {
                let ctx = ui.ctx().clone();
                let mut revision = self.revision;
                let mut export: Option<ExportRequest> = None;
                let changed = {
                    let Project {
                        tables,
                        analyses,
                        graphs,
                        ..
                    } = &mut self.project;
                    graph_panel::graph_panel(
                        ui,
                        &ctx,
                        tables,
                        self.selected_table,
                        analyses,
                        graphs,
                        &mut self.selected_graph,
                        &mut self.cache,
                        revision,
                        &mut export,
                    )
                };
                if changed {
                    self.dirty = true;
                    revision += 1;
                }
                self.revision = revision;
                if let Some(req) = export {
                    self.dialog = Dialog::Export(req);
                }
            }
        }
    }

    /// Draw the status bar at the bottom of the window.
    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let is_error = self.status.starts_with("Error") || self.status.starts_with("Import failed");
                let color = if is_error {
                    error_color(ui)
                } else {
                    ui.visuals().text_color()
                };
                ui.label(RichText::new(&self.status).color(color).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} tables | {} analyses | {} graphs",
                            self.project.tables.len(),
                            self.project.analyses.len(),
                            self.project.graphs.len()
                        ))
                        .weak()
                        .small(),
                    );
                });
            });
        });
    }

    /// Draw the modal dialogs (new table, open, save as, import, export, about,
    /// formats) on top of the current UI.
    fn dialogs(&mut self, ctx: &egui::Context) {
        match self.dialog.clone() {
            Dialog::None => {}
            Dialog::NewTable => {
                let mut open = true;
                let mut create = false;
                egui::Window::new("New table")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_table_name)
                                    .desired_width(200.0),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Type");
                            for f in TableFlavor::all() {
                                ui.selectable_value(&mut self.new_table_flavor, f, f.label());
                            }
                        });
                        if ui.button("Create").clicked() {
                            create = true;
                        }
                    });
                if create {
                    let f = self.new_table_flavor;
                    self.new_table(f);
                    self.dialog = Dialog::None;
                } else if !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::Open => {
                let mut open = true;
                let mut chosen: Option<String> = None;
                egui::Window::new("Open project")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(true)
                    .default_width(460.0)
                    .show(ctx, |ui| {
                        ui.label("Project file (.json saved by this app)");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.path_input)
                                    .desired_width(320.0),
                            );
                            if ui.button("Open").clicked() {
                                chosen = Some(self.path_input.clone());
                            }
                            if ui.button("Refresh").clicked() {
                                self.refresh_listing();
                            }
                        });
                        ui.separator();
                        ui.label(RichText::new("Files in the working directory").weak().small());
                        egui::ScrollArea::vertical()
                            .id_source("open_list")
                            .max_height(240.0)
                            .show(ui, |ui| {
                                for f in &self.listing {
                                    if ui.selectable_label(false, f).clicked() {
                                        chosen = Some(f.clone());
                                    }
                                }
                            });
                    });
                if let Some(p) = chosen {
                    self.open_path(&p);
                    self.dialog = Dialog::None;
                } else if !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::SaveAs => {
                let mut open = true;
                let mut chosen: Option<String> = None;
                egui::Window::new("Save project as")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("File name");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.path_input)
                                    .desired_width(320.0),
                            );
                        });
                        ui.label(
                            RichText::new("Saved as JSON, including tables, analyses and graphs.")
                                .weak()
                                .small(),
                        );
                        if ui.button("Save").clicked() {
                            chosen = Some(self.path_input.clone());
                        }
                    });
                if let Some(p) = chosen {
                    if !p.is_empty() {
                        self.save_to(&p);
                    }
                    self.dialog = Dialog::None;
                } else if !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::Import => {
                let mut open = true;
                let mut chosen: Option<String> = None;
                egui::Window::new("Import CSV")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(true)
                    .default_width(460.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Table type");
                            for f in TableFlavor::all() {
                                ui.selectable_value(&mut self.import_flavor, f, f.label());
                            }
                        });
                        ui.label(
                            RichText::new(match self.import_flavor {
                                TableFlavor::Xy => "Columns: X, Y1, Y2, ... (one row per X)",
                                TableFlavor::Column => "One column per group, one row per subject",
                                TableFlavor::Grouped => {
                                    "First column = row label, then group names repeated once per replicate"
                                }
                                TableFlavor::Contingency => {
                                    "First column = row label, then counts per category"
                                }
                            })
                            .weak()
                            .small(),
                        );
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.path_input)
                                    .desired_width(320.0),
                            );
                            if ui.button("Import").clicked() {
                                chosen = Some(self.path_input.clone());
                            }
                            if ui.button("Refresh").clicked() {
                                self.refresh_listing();
                            }
                        });
                        egui::ScrollArea::vertical()
                            .id_source("import_list")
                            .max_height(200.0)
                            .show(ui, |ui| {
                                for f in &self.listing {
                                    if !f.to_lowercase().ends_with(".csv") {
                                        continue;
                                    }
                                    if ui.selectable_label(false, f).clicked() {
                                        chosen = Some(f.clone());
                                    }
                                }
                            });
                    });
                if let Some(p) = chosen {
                    if !p.is_empty() {
                        let f = self.import_flavor;
                        self.import_csv(&p, f);
                    }
                    self.dialog = Dialog::None;
                } else if !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::Export(mut req) => {
                let mut open = true;
                let mut do_export = false;
                egui::Window::new("Export graph as PNG")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("File");
                            ui.add(
                                egui::TextEdit::singleline(&mut req.path)
                                    .desired_width(280.0),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Size");
                            ui.add(
                                egui::DragValue::new(&mut req.width)
                                    .speed(4.0)
                                    .clamp_range(200..=4000)
                                    .prefix("w "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut req.height)
                                    .speed(4.0)
                                    .clamp_range(150..=4000)
                                    .prefix("h "),
                            );
                        });
                        if ui.button("Export").clicked() {
                            do_export = true;
                        }
                    });
                if do_export {
                    let path = req.path.clone();
                    let (w, h) = (req.width, req.height);
                    match self.project.chart_for_graph(req.graph) {
                        Some(chart) => match crate::plot::render::save_png(&chart, &path, w, h) {
                            Ok(()) => {
                                let abs = std::fs::canonicalize(&path)
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|_| path.clone());
                                self.status = format!("Exported {abs} ({w}x{h})");
                                self.dialog = Dialog::None;
                            }
                            Err(e) => {
                                self.status = format!("Error exporting PNG: {e}");
                                self.dialog = Dialog::Export(req);
                            }
                        },
                        None => {
                            self.status = "Error: that graph has no renderable data".into();
                            self.dialog = Dialog::Export(req);
                        }
                    }
                } else if !open {
                    self.dialog = Dialog::None;
                } else {
                    self.dialog = Dialog::Export(req);
                }
            }
            Dialog::ExportTable { path: initial } => {
                let mut open = true;
                let mut do_export = false;
                let mut path = initial;
                let table_name = self
                    .selected_table
                    .and_then(|i| self.project.tables.get(i))
                    .map(|t| t.as_table().name().to_string())
                    .unwrap_or_else(|| "(none)".into());
                egui::Window::new("Export table as CSV")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(format!("Table: {table_name}"));
                        ui.horizontal(|ui| {
                            ui.label("File");
                            ui.add(
                                egui::TextEdit::singleline(&mut path).desired_width(280.0),
                            );
                        });
                        if ui.button("Export").clicked() {
                            do_export = true;
                        }
                    });
                if do_export {
                    match self
                        .selected_table
                        .and_then(|i| self.project.tables.get(i))
                    {
                        Some(t) => match csv_import::export_table_csv(t, &path) {
                            Ok(()) => {
                                let abs = std::fs::canonicalize(&path)
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|_| path.clone());
                                self.status = format!("Exported {abs}");
                                self.dialog = Dialog::None;
                            }
                            Err(e) => {
                                self.status = format!("Error exporting CSV: {e}");
                                self.dialog = Dialog::ExportTable { path };
                            }
                        },
                        None => {
                            self.status = "Select a table first".into();
                            self.dialog = Dialog::None;
                        }
                    }
                } else if !open {
                    self.dialog = Dialog::None;
                } else {
                    self.dialog = Dialog::ExportTable { path };
                }
            }
            Dialog::Formats => {
                let mut open = true;
                egui::Window::new("CSV formats")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(true)
                    .default_width(520.0)
                    .show(ctx, |ui| {
                        ui.label(RichText::new("Comma separated values, first row used as headers where it is not numeric.").strong());
                        ui.add_space(6.0);
                        egui::Grid::new("fmt_grid")
                            .striped(true)
                            .spacing([18.0, 4.0])
                            .show(ui, |ui| {
                                let rows = [
                                    ("XY", "X,Y1,Y2 — one row per X value, one column per replicate", "0.1,1.2,1.3\n1,5.5,5.8"),
                                    ("Column", "one column per group, one row per subject", "Control,Treated\n1.0,4.0\n1.2,4.4"),
                                    ("Grouped", "label column, then group names repeated per replicate", ",PBS,PBS,Drug A,Drug A\nDay 1,1,1.1,3,3.1"),
                                    ("Contingency", "label column, then counts per category", ",Yes,No\nTreated,80,20\nControl,30,70"),
                                ];
                                for (kind, desc, example) in rows {
                                    ui.label(RichText::new(kind).strong());
                                    ui.vertical(|ui| {
                                        ui.label(desc);
                                        ui.label(RichText::new(example).monospace().small());
                                    });
                                    ui.end_row();
                                }
                            });
                    });
                if !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::About => {
                let mut open = true;
                egui::Window::new("About")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(RichText::new("Aurora — statistics, curve fitting and graphs").strong());
                        ui.add_space(4.0);
                        ui.label("Tables, t tests, ANOVA, non-parametric tests, contingency tests,");
                        ui.label("linear/nonlinear regression, LOWESS, splines, and publication-style graphs.");
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "Statistics are computed in-process; graphs are rendered with plotters.",
                            )
                            .weak()
                            .small(),
                        );
                    });
                if !open {
                    self.dialog = Dialog::None;
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keyboard shortcuts.
        let (save, open) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::S),
                i.modifiers.command && i.key_pressed(egui::Key::O),
            )
        });
        if save {
            self.save_current();
        }
        if open {
            self.dialog = Dialog::Open;
            self.refresh_listing();
        }

        let title = format!(
            "{}{} — Aurora",
            self.project.name,
            if self.dirty { " *" } else { "" }
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        self.menu_bar(ctx);
        self.status_bar(ctx);
        self.sidebar(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            self.central(ui);
        });
        self.dialogs(ctx);
    }
}

/// Convert a display name into a file-name-safe slug
/// (`"Dose-response curve"` → `"dose_response_curve"`).
pub fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "aurora".to_string()
    } else {
        trimmed
    }
}

/// Apply the app-wide sleek purple theme (light or dark).
fn apply_theme(ctx: &egui::Context, dark: bool) {
    theme::apply_theme(ctx, dark);
}

/// Error red, adjusted so it stays legible on both themes.
pub(crate) fn error_color(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(235, 100, 90)
    } else {
        Color32::from_rgb(180, 40, 40)
    }
}

/// Success green, adjusted so it stays legible on both themes.
pub(crate) fn success_color(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(110, 200, 110)
    } else {
        Color32::from_rgb(70, 130, 70)
    }
}

/// A small starter table so a new table is not empty.
/// Create a new (empty) table skeleton of the given flavor with the given name.
fn new_table_skeleton(flavor: TableFlavor, name: String) -> TableInstance {
    match flavor {
        TableFlavor::Xy => TableInstance::Xy(crate::data::tables::XyTable {
            name,
            data: (1..=5)
                .map(|i| (i as f64, vec![0.0, 0.0, 0.0]))
                .collect(),
        }),
        TableFlavor::Column => TableInstance::Column(crate::data::tables::ColumnTable {
            name,
            columns: vec![vec![0.0; 5], vec![0.0; 5]],
            group_names: vec!["Group 1".into(), "Group 2".into()],
        }),
        TableFlavor::Grouped => TableInstance::Grouped(crate::data::tables::GroupedTable {
            name,
            row_labels: vec!["Row 1".into(), "Row 2".into()],
            group_names: vec!["Group 1".into(), "Group 2".into()],
            data: vec![vec![vec![0.0; 3]; 2]; 2],
        }),
        TableFlavor::Contingency => {
            TableInstance::Contingency(crate::data::tables::ContingencyTable {
                name,
                row_labels: vec!["Row 1".into(), "Row 2".into()],
                col_labels: vec!["Column 1".into(), "Column 2".into()],
                data: vec![vec![0, 0], vec![0, 0]],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_filename_safe() {
        assert_eq!(slug("Dose-response curve"), "dose_response_curve");
        assert_eq!(slug("***"), "aurora");
        assert_eq!(slug(""), "aurora");
    }

    #[test]
    fn skeletons_match_their_flavor() {
        for f in TableFlavor::all() {
            let t = new_table_skeleton(f, "t".into());
            assert_eq!(t.flavor(), f);
            assert_eq!(t.as_table().name(), "t");
        }
    }

    #[test]
    fn demo_project_runs_every_analysis_and_builds_every_graph() {
        let mut p = demo::demo_project();
        assert_eq!(p.tables.len(), 4);
        for i in 0..p.analyses.len() {
            p.run_analysis(i).unwrap();
        }
        // Every analysis produced a result and no error.
        for (i, a) in p.analyses.iter().enumerate() {
            assert!(a.error.is_none(), "analysis {i} failed: {:?}", a.error);
            assert!(a.result.is_some(), "analysis {i} has no result");
        }
        // Every graph renders.
        for i in 0..p.graphs.len() {
            let chart = p
                .chart_for_graph(i)
                .unwrap_or_else(|| panic!("graph {i} has no chart"));
            let img = crate::plot::render::render(&chart, 640, 440)
                .unwrap_or_else(|e| panic!("graph {i} failed to render: {e}"));
            assert_eq!(img.rgb.len(), 640 * 440 * 3);
        }
    }

    // -- headless UI smoke tests ------------------------------------------- //

    fn test_context() -> egui::Context {
        let ctx = egui::Context::default();
        // A fixed screen so layout does not depend on the test environment.
        ctx.set_pixels_per_point(1.0);
        ctx
    }

    fn run_frame(ctx: &egui::Context, size: [f32; 2], mut body: impl FnMut(&egui::Context)) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0], size[1]),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| body(ctx));
    }

    #[test]
    fn theme_toggle_applies_light_and_dark_visuals() {
        let ctx = test_context();
        apply_theme(&ctx, true);
        assert!(ctx.style().visuals.dark_mode, "dark theme not applied");
        apply_theme(&ctx, false);
        assert!(!ctx.style().visuals.dark_mode, "light theme not applied");
    }

    #[test]
    fn table_editors_render_without_changing_data() {
        let mut p = demo::demo_project();
        let ctx = test_context();
        let before = serde_json::to_string(&p).unwrap();
        run_frame(&ctx, [1200.0, 800.0], |ctx| {
            for t in 0..p.tables.len() {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Some(table) = p.tables.get_mut(t) {
                        let changed = table_editor::table_editor(ui, table);
                        assert!(!changed, "table {t} changed without any input");
                    }
                });
            }
        });
        assert_eq!(before, serde_json::to_string(&p).unwrap());
    }

    #[test]
    fn analysis_panel_renders_every_spec() {
        use crate::data::tables::{ColumnTable, XyTable};
        use crate::stats::comparison::TTestKind;

        let mut p = Project::new("specs");
        p.add_table(TableInstance::Column(ColumnTable {
            name: "Groups".into(),
            columns: vec![vec![1.0, 2.0, 3.0, 4.0], vec![2.0, 3.0, 4.0, 5.0]],
            group_names: vec!["A".into(), "B".into()],
        }));
        p.add_table(TableInstance::Xy(XyTable {
            name: "XY".into(),
            data: vec![
                (0.0, vec![1.0, 1.1]),
                (1.0, vec![2.0, 2.1]),
                (2.0, vec![3.0, 3.2]),
            ],
        }));
        let specs = vec![
            AnalysisSpec::Descriptive,
            AnalysisSpec::TTest {
                kind: TTestKind::Unpaired,
                group_a: 0,
                group_b: 1,
            },
            AnalysisSpec::TTest {
                kind: TTestKind::OneSample,
                group_a: 0,
                group_b: 0,
            },
            AnalysisSpec::MannWhitney {
                group_a: 0,
                group_b: 1,
            },
            AnalysisSpec::Wilcoxon {
                group_a: 0,
                group_b: 1,
            },
            AnalysisSpec::OneWayAnova,
            AnalysisSpec::LinearRegression,
            AnalysisSpec::Deming { lambda: 1.0 },
            AnalysisSpec::CurveFit {
                model: "Michaelis-Menten".into(),
            },
            AnalysisSpec::AreaUnderCurve,
            AnalysisSpec::Lowess { frac: 0.5 },
        ];
        for spec in specs {
            let table = if spec.applicable_to(TableFlavor::Xy) && !spec.applicable_to(TableFlavor::Column) { 1 } else { 0 };
            let idx = p.add_analysis(crate::analysis::Analysis::new(spec, table));
            let _ = p.run_analysis(idx);
        }

        let ctx = test_context();
        let mut revision = 0;
        for selected in 0..p.analyses.len() {
            let mut sel = Some(selected);
            run_frame(&ctx, [1100.0, 700.0], |ctx| {
                let Project {
                    tables,
                    analyses,
                    graphs,
                    ..
                } = &mut p;
                egui::CentralPanel::default().show(ctx, |ui| {
                    let changed = analysis_panel::analysis_panel(
                        ui,
                        tables,
                        analyses,
                        graphs,
                        analyses[selected].table,
                        &mut sel,
                        &mut revision,
                    );
                    assert!(!changed, "the analysis panel changed data without input");
                });
            });
            assert_eq!(sel, Some(selected));
        }
        // Every runnable analysis found a result.
        for (i, a) in p.analyses.iter().enumerate() {
            assert!(a.result.is_some(), "analysis {i} has no result: {:?}", a.error);
        }
    }

    #[test]
    fn graph_panel_renders_every_graph_into_textures() {
        let mut p = demo::demo_project();
        let ctx = test_context();
        let mut cache = graph_panel::TexCache::default();
        let mut export: Option<graph_panel::ExportRequest> = None;
        let revision = 0u64;

        // Two frames: the first builds the textures, the second reuses them.
        for _frame in 0..2 {
            for g in 0..p.graphs.len() {
                let mut sel = Some(g);
                run_frame(&ctx, [1280.0, 820.0], |ctx| {
                    let Project {
                        tables,
                        analyses,
                        graphs,
                        ..
                    } = &mut p;
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let changed = graph_panel::graph_panel(
                            ui,
                            ctx,
                            tables,
                            Some(0),
                            analyses,
                            graphs,
                            &mut sel,
                            &mut cache,
                            revision,
                            &mut export,
                        );
                        assert!(!changed, "the graph panel changed data without input");
                    });
                });
                assert_eq!(sel, Some(g));
            }
        }
        assert!(cache.len() > 0, "no chart texture was created");
        assert!(export.is_none(), "no export was requested");
    }

    #[test]
    fn graph_panel_handles_missing_references() {
        // A graph whose table/analysis were removed must not panic.
        let mut p = Project::new("broken");
        p.add_graph(crate::analysis::Graph {
            name: "Orphan".into(),
            table: Some(7),
            analysis: Some(9),
            ..Default::default()
        });
        let ctx = test_context();
        let mut cache = graph_panel::TexCache::default();
        let mut export = None;
        let mut sel = Some(0);
        run_frame(&ctx, [900.0, 620.0], |ctx| {
            let Project {
                tables,
                analyses,
                graphs,
                ..
            } = &mut p;
            egui::CentralPanel::default().show(ctx, |ui| {
                graph_panel::graph_panel(
                    ui,
                    ctx,
                    tables,
                    None,
                    analyses,
                    graphs,
                    &mut sel,
                    &mut cache,
                    0,
                    &mut export,
                );
            });
        });
        assert_eq!(cache.len(), 0, "nothing should have been rendered");
        assert_eq!(sel, Some(0));
    }

    #[test]
    fn new_graph_from_each_table_type_renders() {
        for flavor in TableFlavor::all() {
            let mut p = Project::new("one");
            p.add_table(new_table_skeleton(flavor, "t".into()));
            let g = p.add_graph(crate::analysis::Graph {
                name: format!("{flavor:?} graph"),
                table: Some(0),
                ..Default::default()
            });
            let chart = p.chart_for_graph(g).expect("chart");
            assert!(
                crate::plot::render::render(&chart, 480, 360).is_ok(),
                "{flavor:?} graph failed to render"
            );
        }
    }
}
