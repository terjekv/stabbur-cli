//! Saved batch exports; all remote policy and HTTP stay in the supported client.
use crate::{
    AppError,
    cli::{Cli, ExportCommand},
    confirm,
    downloads::VerifiedDownload,
    output, read_json,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use stabbur_client::{
    Authenticated, blocking,
    exports::{ExportDefinition, ExportPlan, MaterializableExport},
};
use std::{fs, path::Path};

#[allow(clippy::too_many_lines)] // Dispatch the saved-export command family.
pub(crate) fn run(
    client: &blocking::Client<Authenticated>,
    command: &ExportCommand,
    cli: &Cli,
) -> Result<(), AppError> {
    let exports = client.exports();
    match command {
        ExportCommand::List(page) => output::page(
            &crate::load_page(page, |cursor, limit| exports.list(cursor, limit))?,
            cli.json,
            &["id", "definition", "revision", "generation"],
        ),
        ExportCommand::Show { export } => output::record(&exports.get(export)?, cli.json),
        ExportCommand::Save {
            file,
            export,
            revision,
        } => {
            let definition: ExportDefinition = read_json(file)?;
            let saved = if let Some(export) = export {
                exports.update(export, &definition, revision.ok_or(AppError::InvalidJson)?)?
            } else {
                exports.create(&definition)?
            };
            output::record(&saved, cli.json)
        }
        ExportCommand::Plan {
            export,
            output: path,
        } => {
            let plan = exports.plan(export)?;
            if let Some(path) = path {
                let file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)?;
                serde_json::to_writer_pretty(file, &plan).map_err(|_| AppError::Output)?;
            }
            if cli.json {
                output::json(&plan)
            } else {
                output::page(
                    &stabbur_client::CursorPage {
                        items: plan.changes,
                        next_cursor: None,
                    },
                    false,
                    &["action", "name", "before", "after", "detail"],
                )?;
                println!(
                    "{}",
                    if plan.ready {
                        "Ready to publish after review."
                    } else {
                        "Blocked: resolve the listed applications before publishing."
                    }
                );
                Ok(())
            }
        }
        ExportCommand::Apply { plan_file } => {
            let plan: ExportPlan = read_json(plan_file)?;
            confirm(cli.yes, "publish the complete saved export plan")?;
            output::record(&exports.apply(&plan)?, cli.json)
        }
        ExportCommand::History {
            export,
            after,
            limit,
        } => output::page(
            &exports.history(export, *after, *limit)?,
            cli.json,
            &["generation", "definition_revision", "created_at"],
        ),
        ExportCommand::Download {
            export,
            generation,
            output: path,
        } => download(client, export, *generation, path, cli.json),
        ExportCommand::Profile {
            export,
            output: path,
            test_all,
        } => {
            confirm(
                cli.yes,
                if *test_all {
                    "issue a profile requesting installation of every exported application on a disposable test Mac"
                } else {
                    "issue a repository access profile for managed Macs"
                },
            )?;
            if path.try_exists()? {
                return Err(AppError::DestinationExists);
            }
            let record = exports.get(export)?;
            let reader = exports.issue_reader(export)?;
            let identifier = format!("org.stabbur.export.{}", record.id);
            let value = serde_json::json!({"PayloadType":"Configuration","PayloadVersion":1,"PayloadScope":"System","PayloadIdentifier":identifier,"PayloadUUID":uuid::Uuid::now_v7(),"PayloadDisplayName":record.definition.data().name,"PayloadContent":[{"PayloadType":"ManagedInstalls","PayloadVersion":1,"PayloadIdentifier":format!("{identifier}.preferences"),"PayloadUUID":uuid::Uuid::now_v7(),"SoftwareRepoURL":exports.repository_url(&record.id.to_string()),"ClientIdentifier":if *test_all{"test-all"}else{"site_default"},"AdditionalHttpHeaders":[format!("Authorization: Basic {}",STANDARD.encode(format!("stabbur:{}",reader.token.expose_secret())))],"InstallAppleSoftwareUpdates":false}]});
            crate::profile::write_secret_file(path, &xml(&value)?)?;
            output::record(
                &serde_json::json!({"output":path,"export":record.id,"credential_saved":true}),
                cli.json,
            )
        }
        ExportCommand::RevokeProfiles { export } => {
            confirm(
                cli.yes,
                "revoke every previously issued device profile for this export",
            )?;
            exports.revoke_readers(export)?;
            output::record(&serde_json::json!({"revoked":true}), cli.json)
        }
    }
}
fn xml(value: &serde_json::Value) -> Result<String, AppError> {
    let mut output = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n",
    );
    crate::munki::plist(value, &mut output)?;
    output.push_str("\n</plist>\n");
    Ok(output)
}
fn download(
    client: &blocking::Client<Authenticated>,
    identity: &str,
    generation: Option<u64>,
    destination: &Path,
    json: bool,
) -> Result<(), AppError> {
    if destination.try_exists()? {
        return Err(AppError::DestinationExists);
    }
    let exports = client.exports();
    let generation = match generation {
        Some(value) => value,
        None => exports.get(identity)?.generation,
    };
    let checked = MaterializableExport::try_from(exports.snapshot(identity, generation)?)?;
    let view = checked.view();
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let staging = tempfile::tempdir_in(parent)?;
    for name in ["pkgs", "pkgsinfo", "catalogs"] {
        fs::create_dir(staging.path().join(name))?;
    }
    let mut seen = std::collections::BTreeSet::new();
    for (item, info) in view.snapshot.items.iter().zip(&view.pkginfo) {
        fs::write(
            staging.path().join("pkgsinfo").join(item.pkginfo_name()),
            xml(info)?,
        )?;
        if seen.insert(item.installer_name()) {
            let output = staging.path().join("pkgs").join(item.installer_name());
            VerifiedDownload::fetch(client, &item.digest, &output, false, false)?
                .publish(&output, false)?;
            if fs::metadata(output)?.len() != item.size {
                return Err(AppError::DigestMismatch);
            }
        }
    }
    let catalog = xml(&serde_json::json!(view.pkginfo))?;
    fs::write(
        staging
            .path()
            .join("catalogs")
            .join(view.snapshot.definition.data().catalog.as_str()),
        &catalog,
    )?;
    fs::write(staging.path().join("catalogs/all"), catalog)?;
    fs::write(
        staging.path().join("export.json"),
        serde_json::to_vec_pretty(&view.snapshot).map_err(|_| AppError::Output)?,
    )?;
    MaterializableExport::try_from(exports.snapshot(identity, generation)?)?;
    // Reserve a fresh destination, and publish its whole repository subtree atomically.
    fs::create_dir(destination)?;
    if let Err(error) = fs::rename(staging.path(), destination.join("repository")) {
        let _ = fs::remove_dir(destination);
        return Err(error.into());
    }
    output::record(
        &serde_json::json!({"output":destination.join("repository"),"export":view.snapshot.export,"generation":generation,"installers":view.snapshot.items.len(),"verified":true,"manifests_included":false}),
        json,
    )
}
