//! Editable views for every table type.
//!
//! Each editor returns `true` when the data changed, so the caller can mark the
//! project dirty and refresh the linked graphs.

use egui::{DragValue, Grid, RichText, ScrollArea, TextEdit, Ui};

use crate::data::tables::{ColumnTable, ContingencyTable, GroupedTable, XyTable};
use crate::data::TableInstance;

/// Draw the editor for whichever table type `table` is.
pub fn table_editor(ui: &mut Ui, table: &mut TableInstance) -> bool {
    match table {
        TableInstance::Xy(t) => xy_editor(ui, t),
        TableInstance::Column(t) => column_editor(ui, t),
        TableInstance::Grouped(t) => grouped_editor(ui, t),
        TableInstance::Contingency(t) => contingency_editor(ui, t),
    }
}

fn num(ui: &mut Ui, v: &mut f64) -> bool {
    ui.add(DragValue::new(v).speed(0.05).max_decimals(6)).changed()
}

fn group_name_widget(ui: &mut Ui, name: &mut String, i: usize) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", i + 1)).weak());
        if name.is_empty() {
            *name = format!("Group {}", i + 1);
        }
        changed |= ui
            .add(TextEdit::singleline(name).desired_width(90.0))
            .changed();
    });
    changed
}

/// Max number of entries across a slice of columns.
fn max_len(cols: &[Vec<f64>]) -> usize {
    cols.iter().map(|c| c.len()).max().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// XY
// ---------------------------------------------------------------------------

fn xy_editor(ui: &mut Ui, t: &mut XyTable) -> bool {
    let mut changed = false;
    let n_cols = t.data.iter().map(|(_, r)| r.len()).max().unwrap_or(0);

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Name").strong());
        changed |= ui
            .add(TextEdit::singleline(&mut t.name).desired_width(200.0))
            .changed();
        ui.separator();
        if ui.button("+ Row").clicked() {
            let next_x = t.data.last().map(|(x, _)| *x + 1.0).unwrap_or(1.0);
            t.data.push((next_x, vec![0.0; n_cols.max(1)]));
            changed = true;
        }
        if ui.button("- Row").clicked() && !t.data.is_empty() {
            t.data.pop();
            changed = true;
        }
        if ui.button("+ Y column").clicked() {
            for (_, reps) in t.data.iter_mut() {
                reps.push(0.0);
            }
            changed = true;
        }
        if ui.button("- Y column").clicked() {
            let keep = n_cols.saturating_sub(1);
            for (_, reps) in t.data.iter_mut() {
                reps.truncate(keep);
            }
            changed = true;
        }
        ui.label(
            RichText::new(format!("{} row(s), {} Y value(s) per row", t.data.len(), n_cols))
                .weak(),
        );
    });

    ui.separator();
    let mut del_row: Option<usize> = None;
    ScrollArea::both()
        .id_source("xy_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Grid::new("xy_grid")
                .striped(true)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("X").strong());
                    for c in 0..n_cols {
                        ui.label(RichText::new(format!("Y{}", c + 1)).strong());
                    }
                    ui.label("");
                    ui.end_row();

                    for (i, (x, reps)) in t.data.iter_mut().enumerate() {
                        changed |= num(ui, x);
                        for r in reps.iter_mut() {
                            changed |= num(ui, r);
                        }
                        // Keep every row the same width.
                        while reps.len() < n_cols {
                            reps.push(0.0);
                            changed = true;
                        }
                        if ui.small_button("x").on_hover_text("Delete row").clicked() {
                            del_row = Some(i);
                        }
                        ui.end_row();
                    }
                });
        });
    if let Some(i) = del_row {
        t.data.remove(i);
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Column
// ---------------------------------------------------------------------------

fn column_editor(ui: &mut Ui, t: &mut ColumnTable) -> bool {
    let mut changed = false;
    let n_cols = t.columns.len();

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Name").strong());
        changed |= ui
            .add(TextEdit::singleline(&mut t.name).desired_width(200.0))
            .changed();
        ui.separator();
        if ui.button("+ Group").clicked() {
            let n = t.columns.len();
            t.columns.push(Vec::new());
            t.group_names.push(format!("Group {}", n + 1));
            changed = true;
        }
        if ui.button("- Group").clicked() && !t.columns.is_empty() {
            t.columns.pop();
            t.group_names.pop();
            changed = true;
        }
        let rows = max_len(&t.columns);
        if ui.button("+ Row").clicked() {
            for c in t.columns.iter_mut() {
                c.push(0.0);
            }
            changed = true;
        }
        if ui.button("- Row").clicked() {
            for c in t.columns.iter_mut() {
                c.pop();
            }
            changed = true;
        }
        ui.label(RichText::new(format!("{n_cols} group(s), {rows} row(s)")).weak());
    });

    ui.separator();
    let mut del_group: Option<usize> = None;
    let mut del_row: Option<usize> = None;
    let mut add_to: Option<usize> = None;

    ScrollArea::both()
        .id_source("column_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Grid::new("column_grid")
                .striped(true)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("#").strong());
                    for (g, name) in t.group_names.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= group_name_widget(ui, name, g);
                            if ui.small_button("x").on_hover_text("Delete group").clicked() {
                                del_group = Some(g);
                            }
                        });
                    }
                    ui.label("");
                    ui.end_row();

                    let rows = max_len(&t.columns);
                    for r in 0..rows {
                        ui.label(RichText::new(format!("{}", r + 1)).weak());
                        for c in t.columns.iter_mut() {
                            match c.get_mut(r) {
                                Some(v) => changed |= num(ui, v),
                                None => {
                                    ui.label("");
                                }
                            }
                        }
                        if ui
                            .small_button("x")
                            .on_hover_text("Delete this row from every group")
                            .clicked()
                        {
                            del_row = Some(r);
                        }
                        ui.end_row();
                    }

                    ui.label("");
                    for g in 0..t.columns.len() {
                        if ui.small_button("+").on_hover_text("Add a value").clicked() {
                            add_to = Some(g);
                        }
                    }
                    ui.label("");
                    ui.end_row();
                });
        });

    if let Some(g) = del_group {
        t.columns.remove(g);
        t.group_names.remove(g);
        changed = true;
    }
    if let Some(g) = add_to {
        if let Some(c) = t.columns.get_mut(g) {
            let last = c.last().copied().unwrap_or(0.0);
            c.push(last);
            changed = true;
        }
    }
    if let Some(r) = del_row {
        for c in t.columns.iter_mut() {
            if r < c.len() {
                c.remove(r);
            }
        }
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Grouped
// ---------------------------------------------------------------------------

/// Number of replicate columns used by one group.
fn group_reps(data: &[Vec<Vec<f64>>], g: usize) -> usize {
    data.iter()
        .filter_map(|row| row.get(g))
        .map(|c| c.len())
        .max()
        .unwrap_or(0)
        .max(1)
}

fn grouped_editor(ui: &mut Ui, t: &mut GroupedTable) -> bool {
    let mut changed = false;
    let n_groups = t.group_names.len();
    let n_rows = t.data.len();
    let reps: Vec<usize> = (0..n_groups).map(|g| group_reps(&t.data, g)).collect();

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Name").strong());
        changed |= ui
            .add(TextEdit::singleline(&mut t.name).desired_width(180.0))
            .changed();
        ui.separator();
        if ui.button("+ Row").clicked() {
            let g = n_groups.max(1);
            t.data.push(vec![vec![0.0]; g]);
            t.row_labels.push(format!("Row {}", n_rows + 1));
            changed = true;
        }
        if ui.button("- Row").clicked() && !t.data.is_empty() {
            t.data.pop();
            t.row_labels.pop();
            changed = true;
        }
        if ui.button("+ Group").clicked() {
            for row in t.data.iter_mut() {
                row.push(vec![0.0]);
            }
            t.group_names.push(format!("Group {}", n_groups + 1));
            changed = true;
        }
        if ui.button("- Group").clicked() && n_groups > 0 {
            for row in t.data.iter_mut() {
                row.pop();
            }
            t.group_names.pop();
            changed = true;
        }
        if ui.button("+ Replicate").clicked() {
            for row in t.data.iter_mut() {
                for cell in row.iter_mut() {
                    cell.push(0.0);
                }
            }
            changed = true;
        }
        if ui.button("- Replicate").clicked() {
            for row in t.data.iter_mut() {
                for cell in row.iter_mut() {
                    cell.pop();
                }
            }
            changed = true;
        }
        ui.label(
            RichText::new(format!("{n_rows} row(s), {n_groups} group(s)")).weak(),
        );
    });

    ui.separator();
    let mut del_group: Option<usize> = None;
    let mut del_row: Option<usize> = None;

    ScrollArea::both()
        .id_source("grouped_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Group name header.
            ui.horizontal_wrapped(|ui| {
                ui.add_space(30.0);
                for (g, name) in t.group_names.iter_mut().enumerate() {
                    ui.group(|ui| {
                        if name.is_empty() {
                            *name = format!("Group {}", g + 1);
                        }
                        changed |= ui
                            .add(TextEdit::singleline(name).desired_width(90.0))
                            .changed();
                        ui.label(
                            RichText::new(format!(
                                "{} replicate(s)",
                                reps.get(g).copied().unwrap_or(1)
                            ))
                            .weak()
                            .small(),
                        );
                        if ui.small_button("x").on_hover_text("Delete group").clicked() {
                            del_group = Some(g);
                        }
                    });
                }
            });
            ui.separator();

            for (r, row) in t.data.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    if let Some(label) = t.row_labels.get_mut(r) {
                        changed |= ui
                            .add(TextEdit::singleline(label).desired_width(80.0))
                            .changed();
                    }
                    for (g, cell) in row.iter_mut().enumerate() {
                        let name = t
                            .group_names
                            .get(g)
                            .cloned()
                            .unwrap_or_else(|| format!("Group {}", g + 1));
                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&name).weak().small());
                                ui.horizontal(|ui| {
                                    for v in cell.iter_mut() {
                                        changed |= num(ui, v);
                                    }
                                });
                            });
                        });
                    }
                    if ui
                        .small_button("x")
                        .on_hover_text("Delete row")
                        .clicked()
                    {
                        del_row = Some(r);
                    }
                });
            }
        });

    if let Some(g) = del_group {
        for row in t.data.iter_mut() {
            if g < row.len() {
                row.remove(g);
            }
        }
        if g < t.group_names.len() {
            t.group_names.remove(g);
        }
        changed = true;
    }
    if let Some(r) = del_row {
        t.data.remove(r);
        if r < t.row_labels.len() {
            t.row_labels.remove(r);
        }
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Contingency
// ---------------------------------------------------------------------------

fn contingency_editor(ui: &mut Ui, t: &mut ContingencyTable) -> bool {
    let mut changed = false;
    let n_cols = t.col_labels.len();

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Name").strong());
        changed |= ui
            .add(TextEdit::singleline(&mut t.name).desired_width(200.0))
            .changed();
        ui.separator();
        if ui.button("+ Row").clicked() {
            t.data.push(vec![0; n_cols.max(1)]);
            t.row_labels.push(format!("Row {}", t.data.len()));
            changed = true;
        }
        if ui.button("- Row").clicked() && !t.data.is_empty() {
            t.data.pop();
            t.row_labels.pop();
            changed = true;
        }
        if ui.button("+ Column").clicked() {
            for row in t.data.iter_mut() {
                row.push(0);
            }
            t.col_labels.push(format!("Column {}", n_cols + 1));
            changed = true;
        }
        if ui.button("- Column").clicked() && n_cols > 0 {
            for row in t.data.iter_mut() {
                row.pop();
            }
            t.col_labels.pop();
            changed = true;
        }
        ui.label(RichText::new("Counts of subjects in each category").weak());
    });

    ui.separator();
    let mut del_col: Option<usize> = None;
    let mut del_row: Option<usize> = None;

    ScrollArea::both()
        .id_source("contingency_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Grid::new("contingency_grid")
                .striped(true)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("").strong());
                    for (c, label) in t.col_labels.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add(TextEdit::singleline(label).desired_width(110.0))
                                .changed();
                            if ui.small_button("x").on_hover_text("Delete column").clicked() {
                                del_col = Some(c);
                            }
                        });
                    }
                    ui.label("");
                    ui.end_row();

                    for (r, row) in t.data.iter_mut().enumerate() {
                        if let Some(label) = t.row_labels.get_mut(r) {
                            changed |= ui
                                .add(TextEdit::singleline(label).desired_width(110.0))
                                .changed();
                        }
                        for v in row.iter_mut() {
                            changed |= ui.add(DragValue::new(v).speed(0.2)).changed();
                        }
                        if ui.small_button("x").on_hover_text("Delete row").clicked() {
                            del_row = Some(r);
                        }
                        ui.end_row();
                    }
                });
        });

    if let Some(c) = del_col {
        for row in t.data.iter_mut() {
            if c < row.len() {
                row.remove(c);
            }
        }
        if c < t.col_labels.len() {
            t.col_labels.remove(c);
        }
        changed = true;
    }
    if let Some(r) = del_row {
        t.data.remove(r);
        if r < t.row_labels.len() {
            t.row_labels.remove(r);
        }
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_reps_reports_the_widest_cell() {
        let data = vec![
            vec![vec![1.0, 2.0, 3.0], vec![1.0]],
            vec![vec![1.0, 2.0], vec![1.0, 2.0]],
        ];
        assert_eq!(group_reps(&data, 0), 3);
        assert_eq!(group_reps(&data, 1), 2);
    }

    #[test]
    fn max_len_is_zero_for_empty_columns() {
        let cols: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
        assert_eq!(max_len(&cols), 0);
    }
}
