//! 结果集导出与 CSV 导入。
//!
//! 导出全部在阻塞线程池执行，避免大结果集阻塞 UI；导入按 1000 行分块，
//! 走驱动层参数化写入，杜绝 SQL 拼接注入。

use dbhelper_models::{ColumnMeta, Row, Value};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Json,
    Xlsx,
    Sql,
    Html,
    Xml,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 6] = [
        ExportFormat::Csv,
        ExportFormat::Json,
        ExportFormat::Xlsx,
        ExportFormat::Sql,
        ExportFormat::Html,
        ExportFormat::Xml,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Csv => "CSV",
            ExportFormat::Json => "JSON",
            ExportFormat::Xlsx => "Excel (.xlsx)",
            ExportFormat::Sql => "SQL 脚本",
            ExportFormat::Html => "HTML 表格",
            ExportFormat::Xml => "XML",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Sql => "sql",
            ExportFormat::Html => "html",
            ExportFormat::Xml => "xml",
        }
    }
}

/// 导出结果集，返回写入行数。
pub async fn export(
    path: String,
    format: ExportFormat,
    columns: Vec<ColumnMeta>,
    rows: Vec<Row>,
    table_hint: String,
) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || {
        let p = Path::new(&path);
        let count = rows.len();
        let result = match format {
            ExportFormat::Csv => write_csv(p, &columns, &rows),
            ExportFormat::Json => write_json(p, &columns, &rows),
            ExportFormat::Xlsx => write_xlsx(p, &columns, &rows),
            ExportFormat::Sql => write_sql(p, &table_hint, &columns, &rows),
            ExportFormat::Html => write_html(p, &columns, &rows),
            ExportFormat::Xml => write_xml(p, &columns, &rows),
        };
        result.map(|_| count)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn write_csv(path: &Path, columns: &[ColumnMeta], rows: &[Row]) -> Result<(), String> {
    let mut writer = csv::Writer::from_path(path).map_err(|e| e.to_string())?;
    writer
        .write_record(columns.iter().map(|c| c.name.as_str()))
        .map_err(|e| e.to_string())?;
    for row in rows {
        writer
            .write_record(row.iter().map(|v| match v {
                Value::Null => String::new(),
                other => other.display(),
            }))
            .map_err(|e| e.to_string())?;
    }
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn write_json(path: &Path, columns: &[ColumnMeta], rows: &[Row]) -> Result<(), String> {
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for (i, col) in columns.iter().enumerate() {
                let value = match row.get(i) {
                    Some(Value::Null) | None => serde_json::Value::Null,
                    Some(Value::Int(v)) => serde_json::Value::from(*v),
                    Some(Value::Float(v)) => serde_json::Value::from(*v),
                    Some(Value::Bool(b)) => serde_json::Value::from(*b),
                    Some(Value::Decimal(s)) | Some(Value::Text(s)) | Some(Value::Timestamp(s)) => {
                        serde_json::Value::from(s.clone())
                    }
                    Some(Value::Json(s)) => {
                        serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::from(s.clone()))
                    }
                    Some(Value::Bytes(b)) => serde_json::Value::from(format!("<{} bytes>", b.len())),
                };
                map.insert(col.name.clone(), value);
            }
            serde_json::Value::Object(map)
        })
        .collect();
    let text = serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn write_xlsx(path: &Path, columns: &[ColumnMeta], rows: &[Row]) -> Result<(), String> {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("结果集").map_err(|e| e.to_string())?;
    for (i, col) in columns.iter().enumerate() {
        sheet
            .write_string(0, i as u16, &col.name)
            .map_err(|e| e.to_string())?;
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            let r = (r + 1) as u32;
            let c = c as u16;
            match value {
                Value::Null => {}
                Value::Int(v) => {
                    sheet.write_number(r, c, *v as f64).map_err(|e| e.to_string())?;
                }
                Value::Float(v) => {
                    sheet.write_number(r, c, *v).map_err(|e| e.to_string())?;
                }
                Value::Bool(b) => {
                    sheet.write_boolean(r, c, *b).map_err(|e| e.to_string())?;
                }
                Value::Decimal(s) => {
                    let n: f64 = s.parse().unwrap_or(0.0);
                    sheet.write_number(r, c, n).map_err(|e| e.to_string())?;
                }
                other => {
                    sheet.write_string(r, c, other.display()).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    workbook.save(path).map_err(|e| e.to_string())
}

fn write_sql(path: &Path, table: &str, columns: &[ColumnMeta], rows: &[Row]) -> Result<(), String> {
    let table = if table.is_empty() { "exported_rows" } else { table };
    let mut out = String::new();
    out.push_str(&format!("-- DBHelper 导出 · {} 行\n", rows.len()));
    let cols = columns
        .iter()
        .map(|c| format!("\"{}\"", c.name.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(", ");
    for row in rows {
        let values = row
            .iter()
            .map(|v| v.to_sql_literal())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("INSERT INTO \"{table}\" ({cols}) VALUES ({values});\n"));
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}

fn write_html(path: &Path, columns: &[ColumnMeta], rows: &[Row]) -> Result<(), String> {
    let mut out = String::from(
        "<!DOCTYPE html><meta charset=\"utf-8\"><title>DBHelper 导出</title>\n\
         <style>body{font:13px/1.5 'Inter','Segoe UI',sans-serif;color:#37352f;margin:24px}\
         table{border-collapse:collapse}th,td{border-bottom:1px solid #e9e9e7;padding:6px 12px;text-align:left}\
         th{color:#787774;font-weight:500;font-size:12px}</style>\n<table><thead><tr>",
    );
    for col in columns {
        out.push_str(&format!("<th>{}</th>", escape_html(&col.name)));
    }
    out.push_str("</tr></thead><tbody>");
    for row in rows {
        out.push_str("<tr>");
        for value in row {
            out.push_str(&format!("<td>{}</td>", escape_html(&value.display())));
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
    std::fs::write(path, out).map_err(|e| e.to_string())
}

fn write_xml(path: &Path, columns: &[ColumnMeta], rows: &[Row]) -> Result<(), String> {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<resultset>\n");
    for row in rows {
        out.push_str("  <row>\n");
        for (i, col) in columns.iter().enumerate() {
            let value = row.get(i).map(|v| v.display()).unwrap_or_default();
            out.push_str(&format!(
                "    <field name=\"{}\">{}</field>\n",
                escape_html(&col.name),
                escape_html(&value)
            ));
        }
        out.push_str("  </row>\n");
    }
    out.push_str("</resultset>\n");
    std::fs::write(path, out).map_err(|e| e.to_string())
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 读取 CSV / xlsx 为表头 + 行（用于导入向导）。
pub async fn read_import_file(path: String) -> Result<(Vec<String>, Vec<Vec<Value>>), String> {
    tokio::task::spawn_blocking(move || {
        let lower = path.to_lowercase();
        if lower.ends_with(".xlsx") || lower.ends_with(".xls") {
            read_xlsx(&path)
        } else {
            read_csv(&path)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn read_csv(path: &str) -> Result<(Vec<String>, Vec<Vec<Value>>), String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| e.to_string())?;
    let mut headers: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for (idx, record) in reader.records().enumerate() {
        let record = record.map_err(|e| e.to_string())?;
        if idx == 0 {
            headers = record.iter().map(|s| s.trim().to_string()).collect();
            continue;
        }
        rows.push(
            record
                .iter()
                .map(|s| {
                    if s.is_empty() {
                        Value::Null
                    } else if let Ok(i) = s.parse::<i64>() {
                        Value::Int(i)
                    } else if let Ok(f) = s.parse::<f64>() {
                        Value::Float(f)
                    } else {
                        Value::Text(s.to_string())
                    }
                })
                .collect(),
        );
    }
    Ok((headers, rows))
}

fn read_xlsx(path: &str) -> Result<(Vec<String>, Vec<Vec<Value>>), String> {
    use calamine::{open_workbook, Data, Reader, Xlsx};
    let mut workbook: Xlsx<_> = open_workbook(path).map_err(|e: calamine::XlsxError| e.to_string())?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| "工作簿为空".to_string())?
        .map_err(|e| e.to_string())?;
    let mut headers: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for (idx, row) in range.rows().enumerate() {
        if idx == 0 {
            headers = row.iter().map(|c| c.to_string().trim().to_string()).collect();
            continue;
        }
        rows.push(
            row.iter()
                .map(|c| match c {
                    Data::Empty => Value::Null,
                    Data::Int(i) => Value::Int(*i),
                    Data::Float(f) => Value::Float(*f),
                    Data::Bool(b) => Value::Bool(*b),
                    Data::String(s) => Value::Text(s.clone()),
                    other => Value::Text(other.to_string()),
                })
                .collect(),
        );
    }
    Ok((headers, rows))
}
