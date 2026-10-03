use std::fmt::Write as _;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Local};
use serde::Serialize;
use serde_json::Value;
use stabbur_client::{RunLog, Software};

use crate::AppError;

pub fn json(value: &impl Serialize) -> Result<(), AppError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|_| AppError::Output)?
    );
    Ok(())
}

pub fn record(value: &impl Serialize, json_output: bool) -> Result<(), AppError> {
    if json_output {
        return json(value);
    }
    let value = serde_json::to_value(value).map_err(|_| AppError::Output)?;
    let object = value.as_object().ok_or(AppError::Output)?;
    if object.contains_key("queued_jobs") && object.contains_key("oldest_queued_at") {
        println!(
            "Queue: {} waiting, {} running",
            display(&value["queued_jobs"]),
            display(&value["running_jobs"])
        );
        println!(
            "Oldest queued work: {}",
            human_value("oldest_queued_at", &value["oldest_queued_at"])
        );
        println!("Workers draining: {}", display(&value["draining_workers"]));
        println!(
            "History: {} failed jobs, {} expired attempts",
            display(&value["failed_jobs"]),
            display(&value["expired_attempts"])
        );
    } else {
        print!("{}", render_record(&value, 0));
    }
    Ok(())
}

pub fn list<T: Serialize>(
    values: &[T],
    json_output: bool,
    columns: &[&str],
) -> Result<(), AppError> {
    if json_output {
        return json(&values);
    }
    let rows = values
        .iter()
        .map(|value| {
            let value = serde_json::to_value(value).map_err(|_| AppError::Output)?;
            if !value.is_object() {
                return Err(AppError::Output);
            }
            Ok(columns
                .iter()
                .map(|column| human_value(column, &value[*column]))
                .collect::<Vec<_>>())
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(120)
        .clamp(40, 240);
    print!("{}", render_table(columns, &rows, width));
    Ok(())
}

pub fn software_list(
    page: &stabbur_client::CursorPage<Software>,
    json_output: bool,
) -> Result<(), AppError> {
    if json_output {
        return json(page);
    }
    list(&page.items, false, &["slug", "name", "revision"])?;
    next_cursor(page.next_cursor.as_deref());
    Ok(())
}

pub fn logs(values: &[RunLog], json_output: bool) -> Result<(), AppError> {
    if json_output {
        for value in values {
            println!(
                "{}",
                serde_json::to_string(value).map_err(|_| AppError::Output)?
            );
        }
        return Ok(());
    }
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    for value in values {
        let bytes = STANDARD
            .decode(&value.message_base64)
            .map_err(|_| AppError::InvalidEventStream)?;
        let destination: &mut dyn std::io::Write = if value.stream == "stderr" {
            &mut stderr
        } else {
            &mut stdout
        };
        destination.write_all(&bytes)?;
        if !bytes.ends_with(b"\n") {
            destination.write_all(b"\n")?;
        }
    }
    Ok(())
}

pub fn message(message: &str, json_output: bool) -> Result<(), AppError> {
    if json_output {
        json(&serde_json::json!({"message": message}))
    } else {
        println!("{}", safe(message));
        Ok(())
    }
}

fn display(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::String(value) => safe(value),
        Value::Bool(value) => if *value { "Yes" } else { "No" }.to_owned(),
        Value::Number(value) => value.to_string(),
        Value::Array(values) if values.is_empty() => "None".to_owned(),
        Value::Array(values) => values.iter().map(display).collect::<Vec<_>>().join(", "),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| format!("{}: {}", title(key), display(value)))
            .collect::<Vec<_>>()
            .join("; "),
    }
}

fn human_value(field: &str, value: &Value) -> String {
    if field.ends_with("_at")
        && let Some(value) = value.as_str()
        && let Ok(date) = DateTime::parse_from_rfc3339(value)
    {
        return date
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S %Z")
            .to_string();
    }
    if field == "schedule" && value["kind"] == "manual" {
        return "Manual".into();
    }
    if field == "schedule"
        && let Some(seconds) = value["every_seconds"].as_u64()
    {
        return format!("Every {seconds} seconds");
    }
    display(value)
}

fn render_record(value: &Value, indent: usize) -> String {
    let mut output = String::new();
    let pad = " ".repeat(indent);
    if let Value::Object(values) = value {
        for (field, value) in values {
            if value.is_object()
                || value
                    .as_array()
                    .is_some_and(|items| items.iter().any(Value::is_object))
            {
                write!(
                    output,
                    "{pad}{}:\n{}",
                    title(field),
                    render_record(value, indent + 2)
                )
                .expect("writing to a String cannot fail");
            } else {
                writeln!(
                    output,
                    "{pad}{}: {}",
                    title(field),
                    human_value(field, value)
                )
                .expect("writing to a String cannot fail");
            }
        }
    } else if let Value::Array(values) = value {
        for (index, value) in values.iter().enumerate() {
            write!(
                output,
                "{pad}{}.\n{}",
                index + 1,
                render_record(value, indent + 2)
            )
            .expect("writing to a String cannot fail");
        }
    } else {
        writeln!(output, "{pad}{}", display(value)).expect("writing to a String cannot fail");
    }
    output
}

fn render_table(columns: &[&str], rows: &[Vec<String>], width: usize) -> String {
    if rows.is_empty() {
        return "No items found.\n".into();
    }
    let headers = columns
        .iter()
        .map(|column| title(column))
        .collect::<Vec<_>>();
    let widths = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            rows.iter()
                .map(|row| row[index].chars().count())
                .max()
                .unwrap_or(0)
                .max(header.chars().count())
        })
        .collect::<Vec<_>>();
    let mut result = String::new();
    if widths.iter().sum::<usize>() + 2 * columns.len().saturating_sub(1) > width {
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                result.push('\n');
            }
            for (header, value) in headers.iter().zip(row) {
                writeln!(result, "{header}: {value}").expect("writing to a String cannot fail");
            }
        }
    } else {
        for row in std::iter::once(&headers).chain(rows) {
            for (index, value) in row.iter().enumerate() {
                result.push_str(value);
                if index + 1 < columns.len() {
                    result.push_str(&" ".repeat(widths[index] - value.chars().count() + 2));
                }
            }
            result.push('\n');
        }
    }
    result
}

fn title(value: &str) -> String {
    let mut words = value.split('_').map(str::to_owned).collect::<Vec<_>>();
    if let Some(first) = words.first_mut()
        && let Some(initial) = first.get_mut(0..1)
    {
        initial.make_ascii_uppercase();
    }
    safe(&words.join(" "))
}

pub fn safe(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn page<T: Serialize>(
    value: &stabbur_client::CursorPage<T>,
    json_output: bool,
    columns: &[&str],
) -> Result<(), AppError> {
    if json_output {
        return json(value);
    }
    list(&value.items, false, columns)?;
    next_cursor(value.next_cursor.as_deref());
    Ok(())
}
pub fn log_page(
    value: &stabbur_client::CursorPage<RunLog>,
    json_output: bool,
) -> Result<(), AppError> {
    if json_output {
        return json(value);
    }
    logs(&value.items, false)?;
    next_cursor(value.next_cursor.as_deref());
    Ok(())
}
pub fn next_cursor(cursor: Option<&str>) {
    if let Some(cursor) = cursor {
        eprintln!("Next page: --cursor {} (or use --all)", safe(cursor));
    }
}

pub fn human_error(error: &AppError) -> String {
    if let AppError::Client(stabbur_client::ApiError::Server(problem)) = error {
        let mut message = safe(&problem.detail);
        for field in &problem.validation_errors {
            write!(
                message,
                "\n  {}: {}",
                safe(&field.field),
                safe(&field.message)
            )
            .expect("writing to a String cannot fail");
        }
        let hint = match problem.status {
            401 => "Sign in again with stabbur auth login.",
            403 => "Ask an administrator for permission to perform this action.",
            404 => "Check the resource name or ID with the corresponding list command.",
            409 | 412 => "Read the current resource and review the change before retrying.",
            429 => "Wait before retrying this request.",
            _ => "",
        };
        if !hint.is_empty() {
            write!(message, "\n{hint}").expect("writing to a String cannot fail");
        }
        if !problem.request_id.is_empty() {
            write!(message, "\nReference: {}", safe(&problem.request_id))
                .expect("writing to a String cannot fail");
        }
        message
    } else {
        safe(&error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narrow_tables_preserve_complete_opaque_identities() {
        let id = "01900000-0000-7000-8000-000000000001";
        let rows = vec![vec![id.into(), "An unusually long software name".into()]];
        let output = render_table(&["id", "name"], &rows, 40);
        assert!(output.contains(id));
        assert!(output.contains("Name: An unusually long software name"));
        assert!(!output.contains('\t'));
    }
    #[test]
    fn nested_records_and_empty_values_are_readable() {
        let value = serde_json::json!({"channels":[],"latest_run":{"state":"succeeded"},"version":"01-custom"});
        let output = render_record(&value, 0);
        assert!(output.contains("Channels: None"));
        assert!(output.contains("Latest run:\n  State: succeeded"));
        assert!(output.contains("Version: 01-custom"));
    }
}
