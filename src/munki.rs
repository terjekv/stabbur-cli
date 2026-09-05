//! Export a reviewed delivery snapshot without sharing a Stabbur bearer token with devices.
use crate::{AppError, cli::MunkiExportArgs, downloads::VerifiedDownload, output, read_json};
use serde_json::{Map, Value};
use stabbur_client::{Authenticated, Resolution, blocking};
use std::{fs, io::Write};

struct MunkiPackage {
    resolution: Resolution,
    info: Map<String, Value>,
}
impl MunkiPackage {
    fn resolve(
        client: &blocking::Client<Authenticated>,
        arguments: &MunkiExportArgs,
    ) -> Result<Self, AppError> {
        let target = &arguments.target;
        let software = client.software().get(&target.software)?;
        let resolution = client.software().resolve(
            &target.software,
            &target.channel,
            target.platform.as_str(),
            target.architecture.as_str(),
            target.macos.as_deref(),
        )?;
        if resolution.variant.platform != "mac_os"
            || !matches!(
                resolution.release.availability,
                stabbur_client::ReleaseAvailability::Available
            )
        {
            return Err(AppError::InvalidMunkiMetadata);
        }
        let mut info = if let Some(path) = &arguments.pkginfo_template {
            read_json::<Map<String, Value>>(path)?
        } else {
            let metadata = software
                .installation
                .ok_or(AppError::InvalidMunkiMetadata)?;
            let mut fields = metadata
                .install
                .as_object()
                .ok_or(AppError::InvalidMunkiMetadata)?
                .clone();
            fields.extend(
                metadata
                    .detection
                    .as_object()
                    .ok_or(AppError::InvalidMunkiMetadata)?
                    .clone(),
            );
            fields
        };
        if !["installs", "receipts"].iter().any(|key| {
            info.get(*key)
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        }) {
            return Err(AppError::InvalidMunkiMetadata);
        }
        // Export-owned identity and routing cannot be overridden by a template.
        for key in [
            "PackageURL",
            "PackageCompleteURL",
            "installer_item_location",
            "installer_item_hash",
            "installer_item_size",
            "name",
            "version",
            "catalogs",
        ] {
            info.remove(key);
        }
        let filename = format!("{}.{}", resolution.artifact_digest, arguments.extension);
        info.insert("name".into(), Value::from(software.slug));
        info.insert("display_name".into(), Value::from(software.name));
        info.insert(
            "version".into(),
            Value::from(resolution.release.version.clone()),
        );
        info.insert("catalogs".into(), serde_json::json!([target.channel]));
        info.insert("installer_item_location".into(), Value::from(filename));
        info.insert(
            "installer_item_hash".into(),
            Value::from(resolution.artifact_digest.to_string()),
        );
        info.insert(
            "installer_item_size".into(),
            Value::from(resolution.artifact_size.div_ceil(1024)),
        );
        let architectures = match resolution.variant.architecture.as_str() {
            "aarch64" => vec!["arm64"],
            "x86_64" => vec!["x86_64"],
            "universal" => vec!["arm64", "x86_64"],
            _ => return Err(AppError::InvalidMunkiMetadata),
        };
        info.insert(
            "supported_architectures".into(),
            serde_json::json!(architectures),
        );
        for (key, value) in [
            ("minimum_os_version", &resolution.variant.minimum_macos),
            ("maximum_os_version", &resolution.variant.maximum_macos),
        ] {
            info.remove(key);
            if let Some(value) = value {
                info.insert(key.into(), Value::from(value.clone()));
            }
        }
        Ok(Self { resolution, info })
    }
}

pub(crate) fn export(
    client: &blocking::Client<Authenticated>,
    arguments: &MunkiExportArgs,
    json: bool,
) -> Result<(), AppError> {
    let package = MunkiPackage::resolve(client, arguments)?;
    let parent = arguments
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let staging = tempfile::tempdir_in(parent)?;
    fs::create_dir(staging.path().join("pkgs"))?;
    fs::create_dir(staging.path().join("pkgsinfo"))?;
    let installer = staging.path().join("pkgs").join(
        package.info["installer_item_location"]
            .as_str()
            .ok_or(AppError::InvalidMunkiMetadata)?,
    );
    VerifiedDownload::fetch(
        client,
        &package.resolution.artifact_digest,
        &installer,
        false,
        false,
    )?
    .publish(&installer, false)?;
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n",
    );
    plist(&Value::Object(package.info.clone()), &mut xml)?;
    xml.push_str("\n</plist>\n");
    let info_path = staging
        .path()
        .join("pkgsinfo")
        .join(format!("{}.plist", package.resolution.artifact_digest));
    let mut file = fs::File::create(info_path)?;
    file.write_all(xml.as_bytes())?;
    file.sync_all()?;
    let current = MunkiPackage::resolve(client, arguments)?;
    if current.resolution != package.resolution {
        return Err(AppError::Client(stabbur_client::ApiError::StalePlan));
    }
    // Reserve the new destination, refusing to replace any existing repository or export.
    fs::create_dir(&arguments.output)?;
    for name in ["pkgs", "pkgsinfo"] {
        fs::rename(staging.path().join(name), arguments.output.join(name))?;
    }
    output::record(
        &serde_json::json!({"output":arguments.output,"release_id":package.resolution.release.id,"digest":package.resolution.artifact_digest,"verified":true,"catalog":arguments.target.channel}),
        json,
    )
}
fn escape(value: &str) -> Result<String, AppError> {
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(AppError::InvalidMunkiMetadata);
    }
    Ok(value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;"))
}
fn plist(value: &Value, output: &mut String) -> Result<(), AppError> {
    use std::fmt::Write as _;
    match value {
        Value::String(value) => {
            write!(output, "<string>{}</string>", escape(value)?).map_err(|_| AppError::Output)?;
        }
        Value::Bool(true) => output.push_str("<true/>"),
        Value::Bool(false) => output.push_str("<false/>"),
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            write!(output, "<integer>{number}</integer>").map_err(|_| AppError::Output)?;
        }
        Value::Array(items) => {
            output.push_str("<array>");
            for item in items {
                plist(item, output)?;
            }
            output.push_str("</array>");
        }
        Value::Object(fields) => {
            output.push_str("<dict>");
            for (key, value) in fields {
                write!(output, "<key>{}</key>", escape(key)?).map_err(|_| AppError::Output)?;
                plist(value, output)?;
            }
            output.push_str("</dict>");
        }
        _ => return Err(AppError::InvalidMunkiMetadata),
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plist_escapes_metadata_and_rejects_invalid_xml_values() {
        let mut xml = String::new();
        plist(
            &serde_json::json!({"name":"A&B <app>","receipts":[{"optional":true}]}),
            &mut xml,
        )
        .unwrap();
        assert!(xml.contains("A&amp;B &lt;app&gt;"));
        assert!(xml.contains("<true/>"));
        assert!(plist(&Value::Null, &mut String::new()).is_err());
        assert!(escape("\0").is_err());
    }
}
