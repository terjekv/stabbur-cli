use base64::{Engine as _, engine::general_purpose::STANDARD};
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
    println!("Field\tValue");
    for (field, value) in object {
        println!("{}\t{}", safe(field), safe(&display(value)));
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
    println!(
        "{}",
        columns
            .iter()
            .map(|column| title(column))
            .collect::<Vec<_>>()
            .join("\t")
    );
    for value in values {
        let value = serde_json::to_value(value).map_err(|_| AppError::Output)?;
        let object = value.as_object().ok_or(AppError::Output)?;
        println!(
            "{}",
            columns
                .iter()
                .map(|column| {
                    safe(&object.get(*column).map_or_else(|| "-".to_owned(), display))
                })
                .collect::<Vec<_>>()
                .join("\t")
        );
    }
    Ok(())
}

pub fn software_list(
    page: &stabbur_client::CursorPage<Software>,
    json_output: bool,
) -> Result<(), AppError> {
    if json_output {
        return json(page);
    }
    println!("Software\tName\tRevision");
    for value in &page.items {
        println!("{}\t{}\t{}", value.slug, safe(&value.name), value.revision);
    }
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
        Value::Null => "-".to_owned(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(values) => values.iter().map(display).collect::<Vec<_>>().join(","),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_else(|_| "<invalid>".to_owned()),
    }
}

fn title(value: &str) -> String {
    let mut words = value.split('_').map(str::to_owned).collect::<Vec<_>>();
    if let Some(first) = words.first_mut()
        && let Some(initial) = first.get_mut(0..1)
    {
        initial.make_ascii_uppercase();
    }
    words.join(" ")
}

fn safe(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
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
fn next_cursor(cursor: Option<&str>) {
    if let Some(cursor) = cursor {
        eprintln!("Next page: --cursor {} (or use --all)", safe(cursor));
    }
}
