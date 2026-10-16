//! CSV import/export for every table type.
//!
//! The formats are deliberately simple and self-describing:
//!
//! | Table | Layout |
//! |---|---|
//! | XY | `X, Y1, Y2, ...` — one row per X value, one column per replicate |
//! | Column | `Group A, Group B, ...` — one column per group, one row per subject |
//! | Grouped | `, A, A, B, B` — group names repeated once per replicate column |
//! | Contingency | `, Cat 1, Cat 2` — row label then counts |
//!
//! Empty cells are skipped (treated as missing values).

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use csv::{ReaderBuilder, WriterBuilder};

use crate::data::TableFlavor;
use crate::data::tables::{ColumnTable, ContingencyTable, GroupedTable, XyTable};
use crate::data::TableInstance;

fn reader(path: &str) -> Result<csv::Reader<BufReader<File>>> {
    let file = File::open(path).with_context(|| format!("Cannot open {path}"))?;
    Ok(ReaderBuilder::new()
        .flexible(true)
        .has_headers(false)
        .from_reader(BufReader::new(file)))
}

/// All records of a CSV file, skipping completely blank lines.
fn records(path: &str) -> Result<Vec<Vec<String>>> {
    let mut r = reader(path)?;
    let mut out = Vec::new();
    for rec in r.records() {
        let rec = rec?;
        if rec.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        out.push(rec.iter().map(|f| f.trim().to_string()).collect());
    }
    Ok(out)
}

fn parse_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>().ok()
}

fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Imported".to_string())
}

/// `X, Y1, Y2, ...` — one row per X, one column per replicate.
pub fn import_xy_csv(path: &str) -> Result<XyTable> {
    let rows = records(path)?;
    if rows.is_empty() {
        bail!("{path} is empty");
    }
    // Skip a header row when the first cell is not numeric.
    let start = if rows[0].first().and_then(|c| parse_f64(c)).is_some() {
        0
    } else {
        1
    };

    let mut data = Vec::new();
    for row in rows.iter().skip(start) {
        let Some(x) = row.first().and_then(|c| parse_f64(c)) else {
            continue;
        };
        let reps: Vec<f64> = row.iter().skip(1).filter_map(|c| parse_f64(c)).collect();
        if reps.is_empty() {
            continue;
        }
        data.push((x, reps));
    }
    if data.is_empty() {
        bail!("No numeric X/Y rows found in {path}");
    }
    Ok(XyTable {
        name: stem(path),
        data,
    })
}

/// One column per group, one row per subject.
pub fn import_column_csv(path: &str) -> Result<ColumnTable> {
    let rows = records(path)?;
    if rows.is_empty() {
        bail!("{path} is empty");
    }
    let n_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if n_cols == 0 {
        bail!("{path} has no columns");
    }
    // A header row is used for group names when its cells are not numeric.
    let first = &rows[0];
    let has_header = first.iter().any(|c| parse_f64(c).is_none() && !c.is_empty());
    let mut group_names: Vec<String> = (0..n_cols)
        .map(|i| format!("Group {}", i + 1))
        .collect();
    if has_header {
        for (i, name) in first.iter().enumerate().take(n_cols) {
            if !name.is_empty() {
                group_names[i] = name.clone();
            }
        }
    }
    let body = if has_header { &rows[1..] } else { &rows[..] };

    let mut columns: Vec<Vec<f64>> = vec![Vec::new(); n_cols];
    for row in body {
        for (i, cell) in row.iter().enumerate().take(n_cols) {
            if let Some(v) = parse_f64(cell) {
                columns[i].push(v);
            }
        }
    }
    if columns.iter().all(|c| c.is_empty()) {
        bail!("No numeric data found in {path}");
    }
    // Drop empty trailing groups.
    while columns.len() > 1 && columns.last().map(|c| c.is_empty()).unwrap_or(false) {
        columns.pop();
        group_names.pop();
    }
    Ok(ColumnTable {
        name: stem(path),
        columns,
        group_names,
    })
}

/// `, A, A, B, B` — group names repeat once per replicate column.
pub fn import_grouped_csv(path: &str) -> Result<GroupedTable> {
    let rows = records(path)?;
    if rows.len() < 2 {
        bail!("{path} needs a header row plus data rows");
    }
    let header = &rows[0];
    // Group names are runs of equal non-empty header cells after the label.
    let mut group_names: Vec<String> = Vec::new();
    let mut rep_counts: Vec<usize> = Vec::new();
    for cell in header.iter().skip(1) {
        let name = if cell.is_empty() {
            group_names
                .last()
                .cloned()
                .unwrap_or_else(|| format!("Group {}", group_names.len() + 1))
        } else {
            cell.clone()
        };
        if group_names.last() == Some(&name) {
            *rep_counts.last_mut().unwrap() += 1;
        } else {
            group_names.push(name);
            rep_counts.push(1);
        }
    }
    if group_names.is_empty() {
        bail!("{path} has no group columns");
    }

    let mut row_labels = Vec::new();
    let mut data: Vec<Vec<Vec<f64>>> = Vec::new();
    for row in rows.iter().skip(1) {
        let label = row
            .first()
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Row {}", data.len() + 1));
        let mut cells = Vec::with_capacity(group_names.len());
        let mut offset = 1usize;
        for reps in &rep_counts {
            let mut cell = Vec::new();
            for k in 0..*reps {
                if let Some(v) = row.get(offset + k).and_then(|c| parse_f64(c)) {
                    cell.push(v);
                }
            }
            cells.push(cell);
            offset += reps;
        }
        row_labels.push(label);
        data.push(cells);
    }
    if data.is_empty() {
        bail!("No data rows found in {path}");
    }
    Ok(GroupedTable {
        name: stem(path),
        row_labels,
        data,
        group_names,
    })
}

/// `, Cat 1, Cat 2` — row label then integer counts.
pub fn import_contingency_csv(path: &str) -> Result<ContingencyTable> {
    let rows = records(path)?;
    if rows.len() < 2 {
        bail!("{path} needs a header row plus data rows");
    }
    let header = &rows[0];
    let n_cols = header.len().saturating_sub(1).max(
        rows.iter()
            .skip(1)
            .map(|r| r.len().saturating_sub(1))
            .max()
            .unwrap_or(0),
    );
    if n_cols == 0 {
        bail!("{path} has no count columns");
    }
    let col_labels = (0..n_cols)
        .map(|i| {
            header
                .get(i + 1)
                .filter(|s| !s.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("Column {}", i + 1))
        })
        .collect();

    let mut row_labels = Vec::new();
    let mut data = Vec::new();
    for row in rows.iter().skip(1) {
        let label = row
            .first()
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Row {}", data.len() + 1));
        let mut counts = Vec::with_capacity(n_cols);
        for i in 0..n_cols {
            let v = row
                .get(i + 1)
                .and_then(|c| parse_f64(c))
                .ok_or_else(|| anyhow!("Row '{label}' is missing count {}", i + 1))?;
            counts.push(v.max(0.0).round() as u64);
        }
        row_labels.push(label);
        data.push(counts);
    }
    if data.is_empty() {
        bail!("No data rows found in {path}");
    }
    Ok(ContingencyTable {
        name: stem(path),
        row_labels,
        col_labels,
        data,
    })
}

/// Import a CSV as the requested table type.
pub fn import_table(path: &str, flavor: TableFlavor) -> Result<TableInstance> {
    Ok(match flavor {
        TableFlavor::Xy => TableInstance::Xy(import_xy_csv(path)?),
        TableFlavor::Column => TableInstance::Column(import_column_csv(path)?),
        TableFlavor::Grouped => TableInstance::Grouped(import_grouped_csv(path)?),
        TableFlavor::Contingency => {
            TableInstance::Contingency(import_contingency_csv(path)?)
        }
    })
}

/// Write a table to CSV in the format [`import_table`] reads back.
pub fn export_table_csv(table: &TableInstance, path: &str) -> Result<()> {
    let file = File::create(path).with_context(|| format!("Cannot create {path}"))?;
    let mut w = WriterBuilder::new().from_writer(BufWriter::new(file));

    match table {
        TableInstance::Xy(t) => {
            let n = t.data.iter().map(|(_, r)| r.len()).max().unwrap_or(1);
            let mut header = vec!["X".to_string()];
            for i in 0..n {
                header.push(format!("Y{}", i + 1));
            }
            w.write_record(&header)?;
            for (x, reps) in &t.data {
                let mut row = vec![x.to_string()];
                for i in 0..n {
                    row.push(reps.get(i).map(|v| v.to_string()).unwrap_or_default());
                }
                w.write_record(&row)?;
            }
        }
        TableInstance::Column(t) => {
            w.write_record(&t.group_names)?;
            let n = t.columns.iter().map(|c| c.len()).max().unwrap_or(0);
            for r in 0..n {
                let row: Vec<String> = t
                    .columns
                    .iter()
                    .map(|c| c.get(r).map(|v| v.to_string()).unwrap_or_default())
                    .collect();
                w.write_record(&row)?;
            }
        }
        TableInstance::Grouped(t) => {
            let n_groups = t.group_names.len();
            let mut header = vec![String::new()];
            for g in 0..n_groups {
                let reps = t
                    .data
                    .iter()
                    .filter_map(|row| row.get(g))
                    .map(|c| c.len())
                    .max()
                    .unwrap_or(1)
                    .max(1);
                for _ in 0..reps {
                    header.push(t.group_names[g].clone());
                }
            }
            w.write_record(&header)?;
            for (r, row) in t.data.iter().enumerate() {
                let mut out = vec![t
                    .row_labels
                    .get(r)
                    .cloned()
                    .unwrap_or_else(|| format!("Row {}", r + 1))];
                for cell in row.iter() {
                    for v in cell {
                        out.push(v.to_string());
                    }
                }
                w.write_record(&out)?;
            }
        }
        TableInstance::Contingency(t) => {
            let mut header = vec![String::new()];
            header.extend(t.col_labels.iter().cloned());
            w.write_record(&header)?;
            for (r, row) in t.data.iter().enumerate() {
                let mut out = vec![t
                    .row_labels
                    .get(r)
                    .cloned()
                    .unwrap_or_else(|| format!("Row {}", r + 1))];
                out.extend(row.iter().map(|v| v.to_string()));
                w.write_record(&out)?;
            }
        }
    }
    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("aurora_test_{name}.csv"));
        p.to_string_lossy().to_string()
    }

    fn write(path: &str, contents: &str) {
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn xy_roundtrip() {
        let p = tmp("xy");
        write(&p, "X,Y1,Y2\n0.1,1.0,1.1\n1,5.5,5.8\n10,12.1,\n");
        let t = import_xy_csv(&p).unwrap();
        assert_eq!(t.data.len(), 3);
        assert_eq!(t.data[0].1.len(), 2);
        // Missing cell is skipped.
        assert_eq!(t.data[2].1.len(), 1);

        let out = tmp("xy_out");
        export_table_csv(&TableInstance::Xy(t), &out).unwrap();
        let again = import_xy_csv(&out).unwrap();
        assert_eq!(again.data.len(), 3);
        assert!((again.data[1].0 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn xy_without_header() {
        let p = tmp("xy_nohdr");
        write(&p, "1,2\n2,4\n3,6\n");
        let t = import_xy_csv(&p).unwrap();
        assert_eq!(t.data.len(), 3);
        assert!((t.data[2].0 - 3.0).abs() < 1e-12);
    }

    #[test]
    fn column_import_uses_header_names() {
        let p = tmp("col");
        write(&p, "Control,Treated\n1.0,4.0\n1.2,4.4\n1.1,4.2\n");
        let t = import_column_csv(&p).unwrap();
        assert_eq!(t.group_names, vec!["Control", "Treated"]);
        assert_eq!(t.columns[0].len(), 3);
        assert!((t.columns[1][2] - 4.2).abs() < 1e-12);
    }

    #[test]
    fn grouped_import_splits_replicate_runs() {
        let p = tmp("grouped");
        write(&p, ",PBS,PBS,Drug A,Drug A\nDay 1,1,1.1,3,3.1\nDay 2,2,2.1,4,4.1\n");
        let t = import_grouped_csv(&p).unwrap();
        assert_eq!(t.group_names, vec!["PBS", "Drug A"]);
        assert_eq!(t.row_labels, vec!["Day 1", "Day 2"]);
        assert_eq!(t.data[0].len(), 2);
        assert_eq!(t.data[0][0], vec![1.0, 1.1]);
        assert_eq!(t.data[1][1], vec![4.0, 4.1]);
    }

    #[test]
    fn grouped_roundtrip() {
        let p = tmp("grouped2");
        write(&p, ",A,A,B,B\nr1,1,2,3,4\nr2,5,6,7,8\n");
        let t = import_grouped_csv(&p).unwrap();
        let out = tmp("grouped2_out");
        export_table_csv(&TableInstance::Grouped(t), &out).unwrap();
        let again = import_grouped_csv(&out).unwrap();
        assert_eq!(again.group_names, vec!["A", "B"]);
        assert_eq!(again.data[1][1], vec![7.0, 8.0]);
    }

    #[test]
    fn contingency_import_and_export() {
        let p = tmp("cont");
        write(&p, ",Yes,No\nTreated,80,20\nControl,30,70\n");
        let t = import_contingency_csv(&p).unwrap();
        assert_eq!(t.col_labels, vec!["Yes", "No"]);
        assert_eq!(t.data, vec![vec![80, 20], vec![30, 70]]);
        let out = tmp("cont_out");
        export_table_csv(&TableInstance::Contingency(t), &out).unwrap();
        let again = import_contingency_csv(&out).unwrap();
        assert_eq!(again.data, vec![vec![80, 20], vec![30, 70]]);
    }

    #[test]
    fn reporting_is_helpful_on_bad_input() {
        let p = tmp("empty");
        write(&p, "");
        let err = import_xy_csv(&p).unwrap_err().to_string();
        assert!(err.contains("empty") || err.contains("No numeric"), "{err}");
        let err = import_table("/definitely/not/here.csv", TableFlavor::Column)
            .err()
            .expect("should fail");
        assert!(err.to_string().contains("Cannot open"), "{err}");
    }
}
