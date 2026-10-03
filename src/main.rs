#![forbid(unsafe_code)]

pub mod cli;
mod downloads;
mod exports;
mod munki;
mod output;
use downloads::download;
mod profile;

use std::{
    fs::{self, File},
    io::{IsTerminal, Write},
    path::Path,
};

use chrono::{DateTime, Utc};
use clap::Parser;
use serde::de::DeserializeOwned;
use stabbur_client::{
    BuildTargetSchedule, BuildTargetUpdate, CatalogManifest, Credentials, InstallationMetadata,
    NewRecipeRevision, PinnedSource, RunId, SecretToken, Sha256Digest, VariantId, blocking,
};
use thiserror::Error;

use crate::cli::{
    ArtifactCommand, AuthCommand, BootstrapArgs, CatalogCommand, CatalogScanCommand,
    ChannelCommand, Cli, Command, JobCommand, PrincipalCommand, PrincipalKind, RecipeCommand,
    ReleaseCommand, RoleCommand, RunCommand, SoftwareCommand, StorageCommand, TargetCommand,
    TokenCommand, VariantCommand, WorkerCommand,
};

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Client(#[from] stabbur_client::ApiError),
    #[error("credential file must be regular and owner-only (mode 0600 or stricter)")]
    UnsafeCredentialFile,
    #[error("no bearer credential is configured; run `stabbur auth login`")]
    MissingCredential,
    #[error("unable to determine the protected profile directory")]
    NoProfileDirectory,
    #[error("protected profile is malformed")]
    InvalidProfile,
    #[error("input JSON is invalid, not a regular file, or exceeds 2 MiB")]
    InvalidJson,
    #[error("RFC 3339 timestamp is invalid")]
    InvalidTimestamp,
    #[error("output serialization failed")]
    Output,
    #[error("destination already exists; use --resume or --overwrite")]
    DestinationExists,
    #[error(
        "Munki export requires macOS, supported architecture, and reviewed pkginfo with installs or receipts"
    )]
    InvalidMunkiMetadata,
    #[error("downloaded artifact digest does not match the requested SHA-256")]
    DigestMismatch,
    #[error("requested resource was not found in its parent collection")]
    ResourceNotFound,
    #[error("operation was not confirmed")]
    ConfirmationDeclined,
    #[error("protected input file is required when no interactive terminal is available")]
    InteractiveInputRequired,
    #[error("live event stream did not match the released SSE contract")]
    InvalidEventStream,
    #[error("run finished with outcome {0:?}")]
    RunNotSucceeded(stabbur_client::TerminalRunState),
    #[error("server returned a repeated pagination cursor")]
    PaginationCycle,
    #[error("local I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(&cli) {
        if cli.json {
            eprintln!(
                "{}",
                serde_json::json!({"error": {"message": error.to_string(), "exit_code": error.exit_code()}})
            );
        } else {
            eprintln!("error: {}", output::human_error(&error));
        }
        std::process::exit(error.exit_code());
    }
}

#[allow(clippy::too_many_lines)]
fn run(cli: &Cli) -> Result<(), AppError> {
    if let Command::Catalog(arguments) = &cli.command {
        if let CatalogCommand::ProposeSource {
            file,
            source_url,
            source_revision,
            output,
        } = &arguments.command
        {
            let manifest = stabbur_client::ValidatedCatalogManifest::new(read_json(file)?)?;
            let pin = stabbur_client::ValidatedSourcePin::try_from(PinnedSource {
                url: source_url.clone(),
                commit: source_revision.clone(),
            })?;
            let proposal = manifest.propose_source_update(&pin)?;
            let mut destination = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            serde_json::to_writer_pretty(&mut destination, proposal.manifest())
                .map_err(|_| AppError::Output)?;
            destination.write_all(b"\n")?;
            destination.sync_all()?;
            return output::record(&proposal, cli.json);
        }
    }
    let profile_path = profile::path(cli.profile.clone())?;
    let saved = profile::load(&profile_path)?;
    let server = cli
        .server
        .clone()
        .or_else(|| saved.as_ref().map(|value| value.server.clone()))
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_owned());

    match &cli.command {
        Command::Bootstrap(arguments) => return bootstrap(cli, arguments, &server),
        Command::Auth(arguments) => match &arguments.command {
            AuthCommand::Bootstrap(arguments) => return bootstrap(cli, arguments, &server),
            AuthCommand::Login(arguments) => {
                return login(cli, arguments, &server, &profile_path);
            }
            _ => {}
        },
        _ => {}
    }

    let token = credential(cli, saved)?;
    let client = blocking::Client::from_url(&server)?.authenticate(token);
    match &cli.command {
        Command::Status => output::record(&client.operational_status()?, cli.json),
        Command::Bootstrap(_) => unreachable!("bootstrap handled before authentication"),
        Command::Auth(arguments) => run_auth(cli, &client, &arguments.command),
        Command::Catalog(arguments) => run_catalog(cli, &client, &arguments.command),
        Command::Target(arguments) => run_target(cli, &client, &arguments.command),
        Command::Software(arguments) => match &arguments.command {
            SoftwareCommand::Status { software } => {
                output::record(&client.software().status(software)?, cli.json)
            }
            SoftwareCommand::List(page) => {
                let page = load_page(page, |cursor, limit| client.software().list(cursor, limit))?;
                output::software_list(&page, cli.json)
            }
            SoftwareCommand::Show { software } => {
                output::record(&client.software().get(software)?, cli.json)
            }
            SoftwareCommand::Create {
                slug,
                name,
                installation,
                idempotency_key,
            } => {
                let installation = installation.as_deref().map(read_json).transpose()?;
                output::record(
                    &client.software().create_with_installation(
                        slug,
                        name,
                        installation.as_ref(),
                        idempotency_key.as_deref(),
                    )?,
                    cli.json,
                )
            }
            SoftwareCommand::UpdateName {
                software,
                name,
                revision,
            } => output::record(
                &client.software().update_name(software, name, *revision)?,
                cli.json,
            ),
            SoftwareCommand::UpdateInstallation {
                software,
                file,
                revision,
            } => {
                let metadata: InstallationMetadata = read_json(file)?;
                output::record(
                    &client
                        .software()
                        .update_installation(software, &metadata, *revision)?,
                    cli.json,
                )
            }
        },
        Command::Release(arguments) => match &arguments.command {
            ReleaseCommand::Withdraw {
                release,
                revision,
                reason,
            } => {
                confirm(cli.yes, &format!("withdraw release {release}"))?;
                output::record(
                    &client
                        .software()
                        .withdraw(release.parse()?, reason, *revision)?,
                    cli.json,
                )
            }
            ReleaseCommand::List { software, page } => {
                let page = load_page(page, |cursor, limit| {
                    client.software().releases(software, cursor, limit)
                })?;
                output::page(&page, cli.json, &["id", "version", "state", "revision"])
            }
            ReleaseCommand::Show { release } => {
                output::record(&client.software().release(release.parse()?)?, cli.json)
            }
            ReleaseCommand::Promote {
                software,
                release,
                channel,
                revision,
                current_revision,
                pinned_variant,
                reason,
            } => {
                let revision = promotion_revision(
                    cli,
                    &client,
                    software,
                    channel,
                    *revision,
                    *current_revision,
                )?;
                confirm(
                    cli.yes,
                    &format!(
                        "promote release {release} to {software}/{channel} at revision {revision}"
                    ),
                )?;
                output::record(
                    &client.software().promote(
                        software,
                        channel,
                        release.parse()?,
                        parse_optional(pinned_variant.as_ref())?,
                        reason.as_deref(),
                        revision,
                    )?,
                    cli.json,
                )
            }
            ReleaseCommand::Reject {
                release,
                revision,
                reason,
            } => {
                confirm(cli.yes, &format!("reject release {release}"))?;
                output::record(
                    &client
                        .software()
                        .reject(release.parse()?, reason, *revision)?,
                    cli.json,
                )
            }
        },
        Command::Variant(arguments) => match &arguments.command {
            VariantCommand::List { release } => {
                let values = client.software().variants(release.parse()?)?;
                output::list(
                    &values,
                    cli.json,
                    &["id", "platform", "architecture", "resolution_priority"],
                )
            }
            VariantCommand::Show { release, variant } => {
                let id: VariantId = variant.parse()?;
                let value = client
                    .software()
                    .variants(release.parse()?)?
                    .into_iter()
                    .find(|value| value.id == id)
                    .ok_or(AppError::ResourceNotFound)?;
                output::record(&value, cli.json)
            }
        },
        Command::Channel(arguments) => match &arguments.command {
            ChannelCommand::List { software } => output::list(
                &client.software().channels(software)?,
                cli.json,
                &["name", "release_id", "pinned_variant_id", "revision"],
            ),
            ChannelCommand::Show { software, channel } => {
                output::record(&client.software().channel(software, channel)?, cli.json)
            }
            ChannelCommand::Promote {
                software,
                channel,
                release,
                revision,
                current_revision,
                pinned_variant,
                reason,
            } => {
                let revision = promotion_revision(
                    cli,
                    &client,
                    software,
                    channel,
                    *revision,
                    *current_revision,
                )?;
                confirm(
                    cli.yes,
                    &format!(
                        "move channel {software}/{channel} to release {release} at revision {revision}"
                    ),
                )?;
                output::record(
                    &client.software().promote(
                        software,
                        channel,
                        release.parse()?,
                        parse_optional(pinned_variant.as_ref())?,
                        reason.as_deref(),
                        revision,
                    )?,
                    cli.json,
                )
            }
        },
        Command::Recipe(arguments) => match &arguments.command {
            RecipeCommand::List(page) => {
                let page = load_page(page, |cursor, limit| client.recipes().list(cursor, limit))?;
                output::page(&page, cli.json, &["id", "name", "revision", "created_at"])
            }
            RecipeCommand::Show { recipe } => {
                output::record(&client.recipes().get(recipe)?, cli.json)
            }
            RecipeCommand::Create {
                name,
                idempotency_key,
            } => output::record(
                &client.recipes().create(name, idempotency_key.as_deref())?,
                cli.json,
            ),
            RecipeCommand::Revisions { recipe } => output::list(
                &client.recipes().revisions(recipe)?,
                cli.json,
                &["id", "sequence", "builder", "required_capabilities"],
            ),
            RecipeCommand::CreateRevision {
                recipe,
                file,
                idempotency_key,
            } => {
                let revision: NewRecipeRevision = read_json(file)?;
                output::record(
                    &client.recipes().create_revision(
                        recipe,
                        &revision,
                        idempotency_key.as_deref(),
                    )?,
                    cli.json,
                )
            }
            RecipeCommand::Runs { recipe, page } => {
                let page = load_page(page, |cursor, limit| {
                    client.recipes().runs(recipe, cursor, limit)
                })?;
                output::page(
                    &page,
                    cli.json,
                    &["id", "state", "software_id", "created_at", "completed_at"],
                )
            }
        },
        Command::Run(arguments) => run_run(cli, &client, &arguments.command),
        Command::Artifact(arguments) => match &arguments.command {
            ArtifactCommand::Show { digest } => {
                output::record(&client.artifacts().get(&digest.parse()?)?, cli.json)
            }
            ArtifactCommand::Locations { digest } => output::list(
                &client.artifacts().locations(&digest.parse()?)?,
                cli.json,
                &["id", "store_id", "state", "verified_at"],
            ),
            ArtifactCommand::Upload {
                digest,
                input,
                media_type,
            } => {
                let digest: Sha256Digest = digest.parse()?;
                let metadata = fs::metadata(input)?;
                if !metadata.is_file() {
                    return Err(AppError::InvalidJson);
                }
                output::record(
                    &client.artifacts().upload_reader(
                        &digest,
                        File::open(input)?,
                        metadata.len(),
                        media_type,
                    )?,
                    cli.json,
                )
            }
            ArtifactCommand::Download(arguments) => download(&client, arguments, cli.json),
        },
        Command::Storage(arguments) => match &arguments.command {
            StorageCommand::List => output::list(
                &client.stores().list()?,
                cli.json,
                &["id", "name", "role", "kind", "enabled"],
            ),
            StorageCommand::Show { store } => {
                output::record(&client.stores().get(store.parse()?)?, cli.json)
            }
            StorageCommand::Test { store } => {
                output::record(&client.stores().test(store.parse()?)?, cli.json)
            }
        },
        Command::Worker(arguments) => run_worker(cli, &client, &arguments.command),
        Command::Job(arguments) => match &arguments.command {
            JobCommand::List(page) => {
                let page = load_page(page, |cursor, limit| client.jobs().list(cursor, limit))?;
                output::page(
                    &page,
                    cli.json,
                    &[
                        "id",
                        "run_id",
                        "recipe_catalog_scan_id",
                        "state",
                        "attempt_count",
                        "maximum_attempts",
                    ],
                )
            }
            JobCommand::Show { job } => output::record(&client.jobs().get(job.parse()?)?, cli.json),
        },
        Command::Audit(arguments) => match &arguments.command {
            cli::AuditCommand::List(page) => {
                let page = load_page(page, |cursor, limit| client.audit().list(cursor, limit))?;
                output::page(
                    &page,
                    cli.json,
                    &[
                        "occurred_at",
                        "action",
                        "resource_kind",
                        "resource_id",
                        "request_id",
                    ],
                )
            }
        },
        Command::Exports(arguments) => exports::run(&client, &arguments.command, cli),
        Command::MunkiExport(arguments) => munki::export(&client, arguments, cli.json),
        Command::Resolve(arguments) => output::record(
            &client.software().resolve(
                &arguments.software,
                &arguments.channel,
                arguments.platform.as_str(),
                arguments.architecture.as_str(),
                arguments.macos.as_deref(),
            )?,
            cli.json,
        ),
    }
}

fn import_catalog(
    cli: &Cli,
    client: &blocking::Client<stabbur_client::Authenticated>,
    snapshot: &str,
    selections: &std::path::Path,
    output_path: &std::path::Path,
) -> Result<(), AppError> {
    let snapshot = client.catalog().snapshot(snapshot.parse()?)?;
    let selections: Vec<stabbur_client::RecipeImportSelection> = read_json(selections)?;
    let manifest = stabbur_client::prepare_recipe_import(&snapshot.manifest, &selections)?;
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)?;
    serde_json::to_writer_pretty(&mut destination, &manifest).map_err(|_| AppError::Output)?;
    destination.write_all(b"\n")?;
    destination.sync_all()?;
    output::record(&manifest, cli.json)
}

fn run_catalog(
    cli: &Cli,
    client: &blocking::Client<stabbur_client::Authenticated>,
    command: &CatalogCommand,
) -> Result<(), AppError> {
    match command {
        CatalogCommand::ProposeSource { .. } => {
            unreachable!("local source proposals are handled before authentication")
        }
        CatalogCommand::Plan { file } => {
            let manifest: CatalogManifest = read_json(file)?;
            output::record(&client.catalog().plan(&manifest)?, cli.json)
        }
        CatalogCommand::Sync { file, plan_file } => {
            let manifest: CatalogManifest = read_json(file)?;
            let manifest = stabbur_client::ValidatedCatalogManifest::new(manifest)?;
            if !manifest.as_manifest().targets.is_empty() {
                confirm(cli.yes, "apply reviewed build-target execution policy")?;
            }
            let reviewed: Option<stabbur_client::CatalogPlan> =
                plan_file.as_deref().map(read_json).transpose()?;
            output::record(
                &client
                    .catalog()
                    .sync_validated(&manifest, reviewed.as_ref())?,
                cli.json,
            )
        }
        CatalogCommand::Import {
            snapshot,
            selections,
            output,
        } => import_catalog(cli, client, snapshot, selections, output),
        CatalogCommand::Snapshots(page) => {
            let page = load_page(page, |cursor, limit| {
                client.catalog().snapshots(cursor, limit)
            })?;
            output::page(
                &page,
                cli.json,
                &[
                    "id",
                    "producer",
                    "source",
                    "recipe_count",
                    "diagnostic_count",
                    "observed_at",
                ],
            )
        }
        CatalogCommand::ShowSnapshot { snapshot } => {
            output::record(&client.catalog().snapshot(snapshot.parse()?)?, cli.json)
        }
        CatalogCommand::Resolve { identifier } => {
            output::record(&client.catalog().resolve(identifier)?, cli.json)
        }
        CatalogCommand::Scan(arguments) => match &arguments.command {
            CatalogScanCommand::List(page) => {
                let page = load_page(page, |cursor, limit| client.catalog().scans(cursor, limit))?;
                output::page(
                    &page,
                    cli.json,
                    &[
                        "id",
                        "producer",
                        "state",
                        "snapshot_id",
                        "requested_at",
                        "completed_at",
                    ],
                )
            }
            CatalogScanCommand::Show { scan } => {
                output::record(&client.catalog().scan(scan.parse()?)?, cli.json)
            }
            CatalogScanCommand::Request {
                source_url,
                source_revision,
                idempotency_key,
            } => output::record(
                &client.catalog().request_autopkg_scan(
                    &PinnedSource {
                        url: source_url.clone(),
                        commit: source_revision.clone(),
                    },
                    idempotency_key,
                )?,
                cli.json,
            ),
            CatalogScanCommand::Cancel {
                scan,
                idempotency_key,
            } => {
                confirm(cli.yes, &format!("cancel catalog scan {scan}"))?;
                output::record(
                    &client
                        .catalog()
                        .cancel_scan(scan.parse()?, idempotency_key)?,
                    cli.json,
                )
            }
        },
    }
}

#[allow(clippy::too_many_lines)] // Keeping one resource family together makes CLI/API parity auditable.
fn run_target(
    cli: &Cli,
    client: &blocking::Client<stabbur_client::Authenticated>,
    command: &TargetCommand,
) -> Result<(), AppError> {
    match command {
        TargetCommand::List(page) => {
            let page = load_page(page, |cursor, limit| {
                client.build_targets().list(cursor, limit)
            })?;
            output::page(
                &page,
                cli.json,
                &[
                    "id",
                    "name",
                    "software_id",
                    "recipe_revision_id",
                    "schedule",
                    "enabled",
                    "next_run_at",
                    "revision",
                ],
            )
        }
        TargetCommand::Show { target } => {
            output::record(&client.build_targets().get(target)?, cli.json)
        }
        TargetCommand::Create {
            name,
            software,
            recipe_revision,
            parameters,
            every_seconds,
            next_run_at,
            disabled,
        } => {
            let parameters = parameters
                .as_deref()
                .map(read_json)
                .transpose()?
                .unwrap_or_default();
            let schedule = every_seconds.map_or(BuildTargetSchedule::Manual, |every_seconds| {
                BuildTargetSchedule::Interval { every_seconds }
            });
            let next_run_at = next_run_at.as_deref().map(parse_timestamp).transpose()?;
            output::record(
                &client.build_targets().create(
                    name,
                    software,
                    recipe_revision.parse()?,
                    &parameters,
                    schedule,
                    next_run_at.as_ref(),
                    !disabled,
                )?,
                cli.json,
            )
        }
        TargetCommand::Update {
            target,
            revision,
            name,
            recipe_revision,
            parameters,
            every_seconds,
            manual,
            next_run_at,
            enable,
            disable,
        } => {
            if *disable {
                confirm(cli.yes, &format!("disable build target {target}"))?;
            }
            let schedule = every_seconds
                .map(|every_seconds| BuildTargetSchedule::Interval { every_seconds })
                .or_else(|| manual.then_some(BuildTargetSchedule::Manual));
            let parameters = parameters.as_deref().map(read_json).transpose()?;
            let next_run_at = next_run_at.as_deref().map(parse_timestamp).transpose()?;
            let enabled = if *enable {
                Some(true)
            } else if *disable {
                Some(false)
            } else {
                None
            };
            output::record(
                &client.build_targets().update(
                    target,
                    &BuildTargetUpdate {
                        name: name.clone(),
                        recipe_revision: recipe_revision.as_deref().map(str::parse).transpose()?,
                        parameters,
                        schedule,
                        next_run_at,
                        enabled,
                    },
                    *revision,
                )?,
                cli.json,
            )
        }
        TargetCommand::Trigger {
            target,
            idempotency_key,
            watch: follow,
            timeout_seconds,
        } => {
            let run = client.build_targets().trigger(target, idempotency_key)?;
            if *follow {
                if !cli.json {
                    eprintln!(
                        "Following run {}. Press Ctrl-C to stop watching; the build continues.",
                        run.id
                    );
                }
                watch(client, run.id, None, *timeout_seconds, cli.json)
            } else {
                output::record(&run, cli.json)
            }
        }
        TargetCommand::Runs { target, page } => {
            let page = load_page(page, |cursor, limit| {
                client.build_targets().runs(target, cursor, limit)
            })?;
            output::page(
                &page,
                cli.json,
                &["id", "state", "software_id", "created_at", "completed_at"],
            )
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run_auth(
    cli: &Cli,
    client: &blocking::Client<stabbur_client::Authenticated>,
    command: &AuthCommand,
) -> Result<(), AppError> {
    match command {
        AuthCommand::Bootstrap(_) => unreachable!("bootstrap handled before authentication"),
        AuthCommand::Login(_) => unreachable!("login handled before authentication"),
        AuthCommand::Me => output::record(&client.me()?, cli.json),
        AuthCommand::ChangePassword {
            current_password_file,
            new_password_file,
        } => {
            let current = password(current_password_file.as_deref(), "Current password: ")?;
            let replacement = password(new_password_file.as_deref(), "New password: ")?;
            client.change_password(&current, &replacement)?;
            output::message("Password changed; existing sessions were revoked", cli.json)
        }
        AuthCommand::ResetPassword {
            principal,
            password_file,
        } => {
            let replacement = password(password_file.as_deref(), "New password: ")?;
            client.identity().reset_password(principal, &replacement)?;
            output::message("Password reset; existing sessions were revoked", cli.json)
        }
        AuthCommand::Principal(arguments) => match &arguments.command {
            PrincipalCommand::List(page) => {
                let page = load_page(page, |cursor, limit| {
                    client.identity().list_principals(cursor, limit)
                })?;
                output::page(
                    &page,
                    cli.json,
                    &["id", "name", "kind", "enabled", "roles", "revision"],
                )
            }
            PrincipalCommand::Show { principal } => {
                output::record(&client.identity().get_principal(principal)?, cli.json)
            }
            PrincipalCommand::Create {
                name,
                kind,
                roles,
                password_file,
            } => {
                let value = match kind {
                    PrincipalKind::Human => {
                        let password = password(password_file.as_deref(), "Initial password: ")?;
                        client.identity().create_human(name, &password, roles)?
                    }
                    PrincipalKind::Service => {
                        if password_file.is_some() {
                            return Err(AppError::UnsafeCredentialFile);
                        }
                        client.identity().create_service(name, roles)?
                    }
                };
                output::record(&value, cli.json)
            }
            PrincipalCommand::SetRoles {
                principal,
                roles,
                revision,
            } => output::record(
                &client.identity().set_roles(principal, roles, *revision)?,
                cli.json,
            ),
            PrincipalCommand::Enable {
                principal,
                revision,
            } => output::record(
                &client.identity().set_enabled(principal, true, *revision)?,
                cli.json,
            ),
            PrincipalCommand::Disable {
                principal,
                revision,
            } => {
                confirm(cli.yes, &format!("disable principal {principal}"))?;
                output::record(
                    &client.identity().set_enabled(principal, false, *revision)?,
                    cli.json,
                )
            }
            PrincipalCommand::ResetPassword {
                principal,
                password_file,
            } => {
                let replacement = password(password_file.as_deref(), "New password: ")?;
                client.identity().reset_password(principal, &replacement)?;
                output::message("Password reset; existing sessions were revoked", cli.json)
            }
            PrincipalCommand::RevokeSessions { principal } => {
                client.identity().revoke_sessions(principal)?;
                output::message("Sessions revoked", cli.json)
            }
        },
        AuthCommand::Token(arguments) => match &arguments.command {
            TokenCommand::List { principal } => output::list(
                &client.identity().list_tokens(principal)?,
                cli.json,
                &["id", "name", "created_at", "expires_at", "revoked_at"],
            ),
            TokenCommand::Create {
                principal,
                name,
                expires_at,
                output_token_file,
            } => {
                let expires_at = expires_at.as_deref().map(parse_timestamp).transpose()?;
                let created = client
                    .identity()
                    .create_token(principal, name, expires_at)?;
                profile::write_secret_file(output_token_file, created.secret.expose_secret())?;
                output::record(&created.token, cli.json)
            }
            TokenCommand::Revoke { token } => {
                confirm(cli.yes, &format!("revoke API token {token}"))?;
                output::record(&client.identity().revoke_token(token)?, cli.json)
            }
        },
        AuthCommand::Role(arguments) => match &arguments.command {
            RoleCommand::List => output::list(
                &client.identity().list_roles()?,
                cli.json,
                &["name", "built_in", "permissions", "revision"],
            ),
            RoleCommand::Create { name, permissions } => {
                output::record(&client.identity().create_role(name, permissions)?, cli.json)
            }
        },
    }
}

fn run_run(
    cli: &Cli,
    client: &blocking::Client<stabbur_client::Authenticated>,
    command: &RunCommand,
) -> Result<(), AppError> {
    match command {
        RunCommand::Wait {
            run,
            timeout_seconds,
        } => {
            let run = client.runs().wait(
                run.parse()?,
                std::time::Duration::from_millis(500),
                std::time::Duration::from_secs(*timeout_seconds),
            )?;
            output::record(&run, cli.json)?;
            check_run_state(&run.state)
        }
        RunCommand::List(page) => {
            let page = load_page(page, |cursor, limit| client.runs().list(cursor, limit))?;
            output::page(
                &page,
                cli.json,
                &[
                    "id",
                    "state",
                    "software_id",
                    "recipe_revision_id",
                    "created_at",
                    "completed_at",
                ],
            )
        }
        RunCommand::Create {
            software,
            recipe_revision,
            parameters,
            idempotency_key,
        } => {
            let parameters = parameters
                .as_deref()
                .map(read_json)
                .transpose()?
                .unwrap_or_default();
            output::record(
                &client.runs().create(
                    software,
                    recipe_revision.parse()?,
                    &parameters,
                    idempotency_key,
                )?,
                cli.json,
            )
        }
        RunCommand::Show { run } => output::record(&client.runs().get(run.parse()?)?, cli.json),
        RunCommand::Logs { run, page } => {
            let page = load_page(page, |cursor, limit| {
                client.runs().logs(run.parse()?, cursor, limit)
            })?;
            output::log_page(&page, cli.json)
        }
        RunCommand::Watch {
            run,
            last_event_id,
            timeout_seconds,
        } => watch(
            client,
            run.parse()?,
            *last_event_id,
            *timeout_seconds,
            cli.json,
        ),
        RunCommand::Cancel {
            run,
            idempotency_key,
        } => {
            confirm(cli.yes, &format!("cancel run {run}"))?;
            output::record(
                &client.runs().cancel(run.parse()?, idempotency_key)?,
                cli.json,
            )
        }
    }
}

fn run_worker(
    cli: &Cli,
    client: &blocking::Client<stabbur_client::Authenticated>,
    command: &WorkerCommand,
) -> Result<(), AppError> {
    match command {
        WorkerCommand::Drain { worker, revision } | WorkerCommand::Resume { worker, revision } => {
            let draining = matches!(command, WorkerCommand::Drain { .. });
            confirm(
                cli.yes,
                &format!(
                    "{} worker {worker}",
                    if draining { "drain" } else { "resume" }
                ),
            )?;
            output::record(
                &client
                    .workers()
                    .set_draining(worker.parse()?, draining, *revision)?,
                cli.json,
            )
        }
        WorkerCommand::List(page) => {
            let page = load_page(page, |cursor, limit| client.workers().list(cursor, limit))?;
            output::page(
                &page,
                cli.json,
                &[
                    "id",
                    "name",
                    "enabled",
                    "allowed_capabilities",
                    "advertised_capabilities",
                    "last_seen_at",
                    "revision",
                ],
            )
        }
        WorkerCommand::Show { worker } => {
            output::record(&client.workers().get(worker.parse()?)?, cli.json)
        }
        WorkerCommand::Provision {
            name,
            capabilities,
            output_token_file,
        } => provision_worker(client, name, capabilities, output_token_file, cli.json),
        WorkerCommand::Enable { worker, revision } => output::record(
            &client
                .workers()
                .set_enabled(worker.parse()?, true, *revision)?,
            cli.json,
        ),
        WorkerCommand::Disable { worker, revision } => {
            confirm(
                cli.yes,
                &format!("disable worker and invalidate its active leases: {worker}"),
            )?;
            output::record(
                &client
                    .workers()
                    .set_enabled(worker.parse()?, false, *revision)?,
                cli.json,
            )
        }
        WorkerCommand::SetCapabilities {
            worker,
            revision,
            capabilities,
        } => {
            confirm(
                cli.yes,
                &format!("replace worker {worker} capability ceiling"),
            )?;
            output::record(
                &client.workers().set_allowed_capabilities(
                    worker.parse()?,
                    capabilities,
                    *revision,
                )?,
                cli.json,
            )
        }
        WorkerCommand::RotateToken {
            worker,
            revision,
            output_token_file,
        } => {
            confirm(cli.yes, &format!("rotate worker {worker} credential"))?;
            let rotated = client.workers().rotate_token(worker.parse()?, *revision)?;
            let credential_file = serde_json::to_string(&serde_json::json!({
                "worker_id": rotated.worker.id,
                "token": rotated.token.expose_secret(),
            }))
            .map_err(|_| AppError::Output)?;
            profile::write_secret_file(output_token_file, &credential_file)?;
            output::record(&rotated.worker, cli.json)
        }
    }
}

fn provision_worker(
    client: &blocking::Client<stabbur_client::Authenticated>,
    name: &str,
    capabilities: &[String],
    output_token_file: &Path,
    json: bool,
) -> Result<(), AppError> {
    let credential = client.workers().provision(name, capabilities)?;
    let credential_file = serde_json::to_string(&serde_json::json!({
        "worker_id": credential.worker_id,
        "token": credential.token.expose_secret(),
    }))
    .map_err(|_| AppError::Output)?;
    profile::write_secret_file(output_token_file, &credential_file)?;
    output::message(
        &format!(
            "Worker {} provisioned; token written owner-only",
            credential.worker_id
        ),
        json,
    )
}

fn login(
    cli: &Cli,
    arguments: &cli::LoginArgs,
    server: &str,
    profile_path: &Path,
) -> Result<(), AppError> {
    let password = password(arguments.password_file.as_deref(), "Password: ")?;
    let client = blocking::Client::from_url(server)?
        .login(&Credentials::new(&arguments.username, password))?;
    let principal = client.me()?;
    if !arguments.no_save {
        profile::save(
            profile_path,
            &profile::Profile {
                server: server.to_owned(),
                token: client.token().expose_secret().to_owned(),
            },
        )?;
    }
    output::record(&principal, cli.json)
}

fn bootstrap(cli: &Cli, arguments: &BootstrapArgs, server: &str) -> Result<(), AppError> {
    let secret = secret_token(
        arguments.bootstrap_secret_file.as_deref(),
        "Bootstrap secret: ",
    )?;
    let password = password(
        arguments.password_file.as_deref(),
        "Administrator password: ",
    )?;
    let principal = blocking::Client::from_url(server)?
        .bootstrap(&secret, &Credentials::new(&arguments.username, password))?;
    output::record(&principal, cli.json)
}

fn credential(cli: &Cli, saved: Option<profile::Profile>) -> Result<SecretToken, AppError> {
    if let Ok(value) = std::env::var("STABBUR_TOKEN") {
        return SecretToken::new(value).map_err(AppError::Client);
    }
    if let Some(path) = &cli.token_file {
        return profile::token_from_file(path);
    }
    saved
        .map(|profile| SecretToken::new(profile.token).map_err(AppError::Client))
        .transpose()?
        .ok_or(AppError::MissingCredential)
}

fn password(path: Option<&Path>, prompt: &str) -> Result<String, AppError> {
    if let Some(path) = path {
        return profile::secret_from_file(path);
    }
    if !std::io::stdin().is_terminal() {
        return Err(AppError::InteractiveInputRequired);
    }
    rpassword::prompt_password(prompt).map_err(AppError::Io)
}

fn secret_token(path: Option<&Path>, prompt: &str) -> Result<SecretToken, AppError> {
    if let Some(path) = path {
        return profile::token_from_file(path);
    }
    if !std::io::stdin().is_terminal() {
        return Err(AppError::InteractiveInputRequired);
    }
    SecretToken::new(rpassword::prompt_password(prompt)?).map_err(AppError::Client)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let metadata = fs::metadata(path).map_err(|_| AppError::InvalidJson)?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err(AppError::InvalidJson);
    }
    serde_json::from_reader(File::open(path)?).map_err(|_| AppError::InvalidJson)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| AppError::InvalidTimestamp)
}

fn parse_optional<T>(value: Option<&String>) -> Result<Option<T>, AppError>
where
    T: std::str::FromStr<Err = stabbur_client::ApiError>,
{
    value
        .map(String::as_str)
        .map(str::parse)
        .transpose()
        .map_err(AppError::Client)
}

fn promotion_revision(
    cli: &Cli,
    client: &blocking::Client<stabbur_client::Authenticated>,
    software: &str,
    channel: &str,
    explicit: Option<u64>,
    fetch_current: bool,
) -> Result<u64, AppError> {
    let interactive = !cli.json && !cli.yes && std::io::stdin().is_terminal();
    if !fetch_current && (explicit.is_some() || !interactive) {
        return Ok(explicit.unwrap_or(0));
    }
    match client.software().channel(software, channel) {
        Ok(value) => Ok(value.revision),
        Err(stabbur_client::ApiError::Server(problem)) if problem.status == 404 => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn confirm(preconfirmed: bool, operation: &str) -> Result<(), AppError> {
    if preconfirmed {
        return Ok(());
    }
    eprint!("Confirm {operation} by typing 'yes': ");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if input.trim() == "yes" {
        Ok(())
    } else {
        Err(AppError::ConfirmationDeclined)
    }
}

fn watch(
    client: &blocking::Client<stabbur_client::Authenticated>,
    run: RunId,
    last_event_id: Option<u64>,
    timeout_seconds: u64,
    json_output: bool,
) -> Result<(), AppError> {
    for event in client.runs().watch(
        run,
        last_event_id,
        std::time::Duration::from_secs(timeout_seconds),
    ) {
        let event = event?;
        if json_output {
            output::json(&event)?;
        }
        match event {
            stabbur_client::RunEvent::Log(log) => {
                if !json_output {
                    output::logs(&[log], false)?;
                }
            }
            stabbur_client::RunEvent::Complete(completion) => {
                if !json_output {
                    output::record(&completion, false)?;
                }
                return if completion.state() == stabbur_client::TerminalRunState::Succeeded {
                    Ok(())
                } else {
                    Err(AppError::RunNotSucceeded(completion.state()))
                };
            }
        }
    }
    Err(AppError::Client(
        stabbur_client::ApiError::EventStreamInterrupted,
    ))
}

fn check_run_state(state: &str) -> Result<(), AppError> {
    match state {
        "succeeded" => Ok(()),
        "failed" => Err(AppError::RunNotSucceeded(
            stabbur_client::TerminalRunState::Failed,
        )),
        "cancelled" => Err(AppError::RunNotSucceeded(
            stabbur_client::TerminalRunState::Cancelled,
        )),
        _ => Err(AppError::InvalidEventStream),
    }
}
impl AppError {
    const fn exit_code(&self) -> i32 {
        match self {
            Self::RunNotSucceeded(stabbur_client::TerminalRunState::Failed) => 2,
            Self::RunNotSucceeded(stabbur_client::TerminalRunState::Cancelled) => 3,
            Self::Client(stabbur_client::ApiError::WaitTimeout) => 124,
            _ => 1,
        }
    }
}

fn load_page<T>(
    arguments: &cli::PageArgs,
    mut fetch: impl FnMut(
        Option<&str>,
        u32,
    ) -> Result<stabbur_client::CursorPage<T>, stabbur_client::ApiError>,
) -> Result<stabbur_client::CursorPage<T>, AppError> {
    let mut page = fetch(arguments.cursor.as_deref(), arguments.limit)?;
    if arguments.all {
        let mut seen = std::collections::BTreeSet::new();
        while let Some(cursor) = page.next_cursor.take() {
            if !seen.insert(cursor.clone()) {
                return Err(AppError::PaginationCycle);
            }
            let next = fetch(Some(&cursor), arguments.limit)?;
            page.items.extend(next.items);
            page.next_cursor = next.next_cursor;
        }
    }
    Ok(page)
}
