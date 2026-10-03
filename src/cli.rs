use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Parsed Stabbur command line. Secret values are deliberately absent.
#[derive(Debug, Parser)]
#[command(
    name = "stabbur",
    version,
    about,
    after_help = "Examples:\n  stabbur auth login --username operator\n  stabbur software status firefox\n  stabbur target trigger firefox-hourly --idempotency-key check-001 --watch\n  stabbur --json catalog plan --file catalog.json"
)]
pub struct Cli {
    /// Server origin. Defaults to the protected profile, then localhost.
    #[arg(long, global = true, env = "STABBUR_SERVER_URL")]
    pub server: Option<String>,
    /// Owner-only bearer-token file.
    #[arg(long, global = true, env = "STABBUR_TOKEN_FILE")]
    pub token_file: Option<PathBuf>,
    /// Protected profile file.
    #[arg(long, global = true, env = "STABBUR_PROFILE")]
    pub profile: Option<PathBuf>,
    /// Emit stable JSON rather than human tables.
    #[arg(long, global = true)]
    pub json: bool,
    /// Confirm publication, rejection, credential rotation, or disabling non-interactively.
    #[arg(long, global = true)]
    pub yes: bool,
    /// Resource command.
    #[command(subcommand)]
    pub command: Command,
}

/// Complete v0.0.1 resource catalog.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Saved batch exports, shared with the web console.
    Exports(ExportArgs),
    /// Export a promoted installer and reviewed pkginfo into a new Munki delivery directory.
    MunkiExport(MunkiExportArgs),
    /// Show durable queue and worker measurements.
    Status,
    /// Create the one-time first administrator without an existing login.
    Bootstrap(BootstrapArgs),
    /// Authentication, principals, tokens, and roles.
    Auth(AuthArgs),
    /// Versioned desired-state catalog planning and synchronization.
    Catalog(CatalogArgs),
    /// Desired manual and recurring build targets.
    Target(TargetArgs),
    /// Software catalog operations.
    Software(SoftwareArgs),
    /// Release lifecycle operations.
    Release(ReleaseArgs),
    /// Release variant operations.
    Variant(VariantArgs),
    /// Channel and promotion operations.
    Channel(ChannelArgs),
    /// Recipe and immutable-revision operations.
    Recipe(RecipeArgs),
    /// Run execution, provenance, verification, and logs.
    Run(RunArgs),
    /// Immutable artifact operations.
    Artifact(ArtifactArgs),
    /// Artifact store operations.
    Storage(StorageArgs),
    /// Worker administration.
    Worker(WorkerArgs),
    /// Job inspection.
    Job(JobArgs),
    /// Append-only audit reads.
    Audit(AuditArgs),
    /// Resolve one installable artifact.
    Resolve(ResolveArgs),
}

#[derive(Debug, Args)]
pub struct CatalogArgs {
    #[command(subcommand)]
    pub command: CatalogCommand,
}

#[derive(Debug, Subcommand)]
pub enum CatalogCommand {
    /// Prepare disabled targets from a discovered snapshot; review with catalog plan before sync.
    Import {
        /// Immutable catalog snapshot identity (see catalog snapshots).
        #[arg(long)]
        snapshot: String,
        /// JSON array of recipe import selections, including names, architecture and output variables.
        #[arg(long)]
        selections: PathBuf,
        /// New validated catalog file. Never overwrites an existing file.
        #[arg(long)]
        output: PathBuf,
    },
    /// Propose an exact source pin change and diff without contacting or changing the server.
    ProposeSource {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        source_url: String,
        #[arg(long)]
        source_revision: String,
        /// New file for the validated proposed manifest. Never overwrites an existing file.
        #[arg(long)]
        output: PathBuf,
    },
    /// Validate the manifest and show deterministic changes without mutating the server.
    Plan {
        /// Versioned catalog manifest JSON file.
        #[arg(long)]
        file: PathBuf,
    },
    /// Reconcile the manifest idempotently through the public API.
    Sync {
        /// Optional JSON plan previously reviewed; stale plans are rejected before mutation.
        #[arg(long)]
        plan_file: Option<PathBuf>,
        /// Versioned catalog manifest JSON file.
        #[arg(long)]
        file: PathBuf,
    },
    /// List immutable worker-observed catalog snapshots.
    Snapshots(PageArgs),
    /// Show one immutable catalog snapshot and complete manifest.
    ShowSnapshot { snapshot: String },
    /// Resolve an exact recipe identifier in latest source observations.
    Resolve { identifier: String },
    /// Durable server-requested catalog scans.
    Scan(CatalogScanArgs),
}

#[derive(Debug, Args)]
pub struct CatalogScanArgs {
    #[command(subcommand)]
    pub command: CatalogScanCommand,
}

#[derive(Debug, Subcommand)]
pub enum CatalogScanCommand {
    /// List durable catalog scans.
    List(PageArgs),
    /// Show one durable catalog scan.
    Show { scan: String },
    /// Queue one exact pinned AutoPkg repository scan.
    Request {
        #[arg(long)]
        source_url: String,
        #[arg(long)]
        source_revision: String,
        #[arg(long)]
        idempotency_key: String,
    },
    /// Cancel queued or leased scan work.
    Cancel {
        scan: String,
        #[arg(long)]
        idempotency_key: String,
    },
}

#[derive(Debug, Args)]
pub struct TargetArgs {
    #[command(subcommand)]
    pub command: TargetCommand,
}

#[derive(Debug, Subcommand)]
pub enum TargetCommand {
    /// List desired build targets.
    List(PageArgs),
    /// Show a target by UUIDv7 or exact name.
    Show { target: String },
    /// Create desired manual or recurring build policy.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        software: String,
        #[arg(long)]
        recipe_revision: String,
        /// JSON object containing non-secret builder parameters.
        #[arg(long)]
        parameters: Option<PathBuf>,
        /// Fixed recurring interval. Omit for a manual target.
        #[arg(long, value_parser = clap::value_parser!(u32).range(60..=31_536_000))]
        every_seconds: Option<u32>,
        /// Initial RFC 3339 recurring cursor.
        #[arg(long)]
        next_run_at: Option<String>,
        /// Create the target disabled.
        #[arg(long)]
        disabled: bool,
    },
    /// Partially update desired policy using optimistic concurrency.
    Update {
        target: String,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        recipe_revision: Option<String>,
        /// Replacement JSON object of non-secret builder parameters.
        #[arg(long)]
        parameters: Option<PathBuf>,
        /// Replace the trigger policy with a fixed interval.
        #[arg(long, conflicts_with = "manual", value_parser = clap::value_parser!(u32).range(60..=31_536_000))]
        every_seconds: Option<u32>,
        /// Replace the trigger policy with manual execution.
        #[arg(long, conflicts_with = "every_seconds")]
        manual: bool,
        /// Replacement RFC 3339 recurring cursor.
        #[arg(long)]
        next_run_at: Option<String>,
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
    },
    /// Queue one manual run idempotently.
    #[command(
        after_help = "Example:\n  stabbur target trigger firefox-hourly --idempotency-key check-001 --watch\n\nWatching exits 0 for success, 2 for failure, 3 for cancellation, and 124 for timeout.\nStopping the watch does not cancel the server-side build."
    )]
    Trigger {
        target: String,
        #[arg(long)]
        idempotency_key: String,
        /// Follow logs until the triggered run reaches a terminal outcome.
        #[arg(long)]
        watch: bool,
        /// Overall watch deadline, including reconnections.
        #[arg(long, default_value_t = 3600, requires = "watch")]
        timeout_seconds: u64,
    },
    /// List runs created from one target.
    Runs {
        target: String,
        #[command(flatten)]
        page: PageArgs,
    },
}

#[derive(Debug, Args)]
pub struct BootstrapArgs {
    /// First administrator login name.
    #[arg(long)]
    pub username: String,
    /// Owner-only bootstrap-secret file; otherwise prompt interactively.
    #[arg(long)]
    pub bootstrap_secret_file: Option<PathBuf>,
    /// Owner-only administrator-password file; otherwise prompt interactively.
    #[arg(long)]
    pub password_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct PageArgs {
    /// Fetch every page, preserving a stable JSON page envelope.
    #[arg(long, conflicts_with = "cursor")]
    pub all: bool,
    /// Opaque next-page cursor.
    #[arg(long)]
    pub cursor: Option<String>,
    /// Page size from 1 through 200.
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..=200))]
    pub limit: u32,
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Create the one-time first administrator without an existing login.
    Bootstrap(BootstrapArgs),
    /// Log in interactively and save a protected profile.
    Login(LoginArgs),
    /// Show the current principal.
    Me,
    /// Change the current password and revoke sessions.
    ChangePassword {
        /// Owner-only current-password file; otherwise prompt.
        #[arg(long)]
        current_password_file: Option<PathBuf>,
        /// Owner-only new-password file; otherwise prompt.
        #[arg(long)]
        new_password_file: Option<PathBuf>,
    },
    /// Reset a human password as an authenticated administrator.
    ResetPassword {
        /// Human principal UUIDv7 or exact name.
        principal: String,
        /// Owner-only new-password file; otherwise prompt interactively.
        #[arg(long)]
        password_file: Option<PathBuf>,
    },
    /// Human and service principal administration.
    Principal(PrincipalArgs),
    /// Named long-lived API token administration.
    Token(TokenArgs),
    /// Role and permission policy.
    Role(RoleArgs),
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Human login name.
    #[arg(long)]
    pub username: String,
    /// Owner-only password file; otherwise prompt interactively.
    #[arg(long)]
    pub password_file: Option<PathBuf>,
    /// Return a session without saving it to the local profile.
    #[arg(long)]
    pub no_save: bool,
}

#[derive(Debug, Args)]
pub struct PrincipalArgs {
    #[command(subcommand)]
    pub command: PrincipalCommand,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum PrincipalKind {
    Human,
    Service,
}

#[derive(Debug, Subcommand)]
pub enum PrincipalCommand {
    /// List principals.
    List(PageArgs),
    /// Show a principal by UUIDv7 or exact name.
    Show { principal: String },
    /// Create a human or service principal.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, value_enum)]
        kind: PrincipalKind,
        /// Initial roles; repeat for multiple roles.
        #[arg(long = "role", required = true)]
        roles: Vec<String>,
        /// Owner-only password file for a human; otherwise prompt.
        #[arg(long)]
        password_file: Option<PathBuf>,
    },
    /// Replace all role assignments.
    SetRoles {
        principal: String,
        #[arg(long = "role", required = true)]
        roles: Vec<String>,
        #[arg(long)]
        revision: u64,
    },
    /// Enable authentication for a principal.
    Enable {
        principal: String,
        #[arg(long)]
        revision: u64,
    },
    /// Disable authentication and revoke credentials.
    Disable {
        principal: String,
        #[arg(long)]
        revision: u64,
    },
    /// Reset a human password.
    ResetPassword {
        principal: String,
        /// Owner-only new-password file; otherwise prompt.
        #[arg(long)]
        password_file: Option<PathBuf>,
    },
    /// Revoke all current sessions.
    RevokeSessions { principal: String },
}

#[derive(Debug, Args)]
pub struct TokenArgs {
    #[command(subcommand)]
    pub command: TokenCommand,
}

#[derive(Debug, Subcommand)]
pub enum TokenCommand {
    /// List token metadata for a principal.
    List { principal: String },
    /// Create a token and write its secret to a new owner-only file.
    Create {
        principal: String,
        #[arg(long)]
        name: String,
        /// RFC 3339 expiration.
        #[arg(long)]
        expires_at: Option<String>,
        #[arg(long)]
        output_token_file: PathBuf,
    },
    /// Revoke a token by identity.
    Revoke { token: String },
}

#[derive(Debug, Args)]
pub struct RoleArgs {
    #[command(subcommand)]
    pub command: RoleCommand,
}

#[derive(Debug, Subcommand)]
pub enum RoleCommand {
    /// List built-in and custom roles.
    List,
    /// Create a custom role.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long = "permission", required = true)]
        permissions: Vec<String>,
    },
}

#[derive(Debug, Args)]
pub struct SoftwareArgs {
    #[command(subcommand)]
    pub command: SoftwareCommand,
}

#[derive(Debug, Subcommand)]
pub enum SoftwareCommand {
    /// Show publication, latest build, scheduling, and blocked-worker status.
    Status { software: String },
    /// List software.
    List(PageArgs),
    /// Show software by UUIDv7 or slug.
    Show { software: String },
    /// Create software.
    Create {
        #[arg(long)]
        slug: String,
        #[arg(long)]
        name: String,
        /// JSON file containing installation/detection metadata.
        #[arg(long)]
        installation: Option<PathBuf>,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Replace the display name with optimistic concurrency.
    UpdateName {
        software: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        revision: u64,
    },
    /// Replace installation/detection metadata from JSON.
    UpdateInstallation {
        software: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        revision: u64,
    },
}

#[derive(Debug, Args)]
pub struct ReleaseArgs {
    #[command(subcommand)]
    pub command: ReleaseCommand,
}
#[derive(Debug, Subcommand)]
pub enum ReleaseCommand {
    /// Withdraw publication eligibility while retaining lifecycle and immutable bytes.
    Withdraw {
        release: String,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        reason: String,
    },
    /// List releases for software.
    List {
        software: String,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Show a release.
    Show { release: String },
    /// Promote a release to testing or stable.
    #[command(
        after_help = "Example:\n  stabbur release promote firefox RELEASE_ID --channel testing --current-revision\n\nInteractive use reads the current revision when --revision is omitted.\nAutomation keeps revision 0 as the default; pass an exact --revision or opt into --current-revision."
    )]
    Promote {
        software: String,
        release: String,
        #[arg(long)]
        channel: String,
        /// Expected channel revision; 0 creates a channel. Interactive omission reads current state.
        #[arg(long)]
        revision: Option<u64>,
        /// Read the channel revision before review; concurrent changes are still rejected.
        #[arg(long, conflicts_with = "revision")]
        current_revision: bool,
        #[arg(long)]
        pinned_variant: Option<String>,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Reject a release and remove active channel bindings.
    Reject {
        release: String,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        reason: String,
    },
}

#[derive(Debug, Args)]
pub struct VariantArgs {
    #[command(subcommand)]
    pub command: VariantCommand,
}
#[derive(Debug, Subcommand)]
pub enum VariantCommand {
    /// List variants for a release.
    List { release: String },
    /// Show a variant from its parent release.
    Show { release: String, variant: String },
}

#[derive(Debug, Args)]
pub struct ChannelArgs {
    #[command(subcommand)]
    pub command: ChannelCommand,
}
#[derive(Debug, Subcommand)]
pub enum ChannelCommand {
    /// List channels for software.
    List { software: String },
    /// Show a channel.
    Show { software: String, channel: String },
    /// Create or advance a testing/stable channel.
    Promote {
        software: String,
        channel: String,
        release: String,
        /// Expected channel revision; 0 creates a channel. Interactive omission reads current state.
        #[arg(long)]
        revision: Option<u64>,
        /// Read the channel revision before review; concurrent changes are still rejected.
        #[arg(long, conflicts_with = "revision")]
        current_revision: bool,
        #[arg(long)]
        pinned_variant: Option<String>,
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct RecipeArgs {
    #[command(subcommand)]
    pub command: RecipeCommand,
}
#[derive(Debug, Subcommand)]
pub enum RecipeCommand {
    /// List recipes.
    List(PageArgs),
    /// Show a recipe.
    Show { recipe: String },
    /// Create recipe metadata.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// List immutable revisions.
    Revisions { recipe: String },
    /// Create a validated builder-neutral immutable revision from reviewed JSON.
    CreateRevision {
        recipe: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// List runs associated with a recipe.
    Runs {
        recipe: String,
        #[command(flatten)]
        page: PageArgs,
    },
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[command(subcommand)]
    pub command: RunCommand,
}
#[derive(Debug, Subcommand)]
pub enum RunCommand {
    /// Wait for a terminal outcome and return a failing exit status for failed/cancelled builds.
    Wait {
        run: String,
        #[arg(long, default_value_t = 3600)]
        timeout_seconds: u64,
    },
    /// List runs.
    List(PageArgs),
    /// Queue a run from an immutable recipe revision.
    Create {
        #[arg(long)]
        software: String,
        #[arg(long)]
        recipe_revision: String,
        /// JSON object of non-secret parameters.
        #[arg(long)]
        parameters: Option<PathBuf>,
        #[arg(long)]
        idempotency_key: String,
    },
    /// Show status, provenance, and verification.
    Show { run: String },
    /// List ordered logs.
    Logs {
        run: String,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Replay and watch server-sent log events until terminal.
    Watch {
        run: String,
        /// Overall deadline including reconnections.
        #[arg(long, default_value_t = 3600)]
        timeout_seconds: u64,
        #[arg(long)]
        last_event_id: Option<u64>,
    },
    /// Request cancellation.
    Cancel {
        run: String,
        #[arg(long)]
        idempotency_key: String,
    },
}

#[derive(Debug, Args)]
pub struct ArtifactArgs {
    #[command(subcommand)]
    pub command: ArtifactCommand,
}
#[derive(Debug, Subcommand)]
pub enum ArtifactCommand {
    /// Show immutable artifact metadata.
    Show { digest: String },
    /// List independently tracked locations.
    Locations { digest: String },
    /// Upload and server-verify content from a file.
    Upload {
        digest: String,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = "application/octet-stream")]
        media_type: String,
    },
    /// Download and locally verify immutable content.
    Download(DownloadArgs),
}

#[derive(Debug, Args)]
pub struct DownloadArgs {
    /// Lowercase SHA-256 digest.
    pub digest: String,
    /// Destination path.
    #[arg(long)]
    pub output: PathBuf,
    /// Resume using a byte range.
    #[arg(long, conflicts_with = "overwrite")]
    pub resume: bool,
    /// Replace an existing destination.
    #[arg(long, conflicts_with = "resume")]
    pub overwrite: bool,
}

#[derive(Debug, Args)]
pub struct StorageArgs {
    #[command(subcommand)]
    pub command: StorageCommand,
}
#[derive(Debug, Subcommand)]
pub enum StorageCommand {
    /// List configured stores.
    List,
    /// Show a store.
    Show { store: String },
    /// Run a non-mutating adapter probe.
    Test { store: String },
}

#[derive(Debug, Args)]
pub struct WorkerArgs {
    #[command(subcommand)]
    pub command: WorkerCommand,
}
#[derive(Debug, Subcommand)]
pub enum WorkerCommand {
    /// Pause new claims while current attempts finish.
    Drain {
        worker: String,
        #[arg(long)]
        revision: u64,
    },
    /// Resume claims after maintenance.
    Resume {
        worker: String,
        #[arg(long)]
        revision: u64,
    },
    /// List workers.
    List(PageArgs),
    /// Show a worker.
    Show { worker: String },
    /// Provision a worker and write the one-time token owner-only.
    Provision {
        #[arg(long)]
        name: String,
        #[arg(long = "capability", required = true)]
        capabilities: Vec<String>,
        #[arg(long)]
        output_token_file: PathBuf,
    },
    /// Enable worker authentication and leasing.
    Enable {
        worker: String,
        #[arg(long)]
        revision: u64,
    },
    /// Drain/disable a worker and invalidate its current attempt.
    Disable {
        worker: String,
        #[arg(long)]
        revision: u64,
    },
    /// Replace the server-enforced capability ceiling.
    SetCapabilities {
        worker: String,
        #[arg(long)]
        revision: u64,
        #[arg(long = "capability", required = true)]
        capabilities: Vec<String>,
    },
    /// Rotate a credential and write it owner-only.
    RotateToken {
        worker: String,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        output_token_file: PathBuf,
    },
}

#[derive(Debug, Args)]
pub struct JobArgs {
    #[command(subcommand)]
    pub command: JobCommand,
}
#[derive(Debug, Subcommand)]
pub enum JobCommand {
    /// List lightweight job summaries.
    List(PageArgs),
    /// Show a full builder-neutral job.
    Show { job: String },
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    #[command(subcommand)]
    pub command: AuditCommand,
}
#[derive(Debug, Subcommand)]
pub enum AuditCommand {
    /// List append-only events.
    List(PageArgs),
}

#[derive(Debug, Args)]
pub struct ResolveArgs {
    /// Software UUIDv7 or slug.
    pub software: String,
    /// Channel name.
    #[arg(long, default_value = "stable")]
    pub channel: String,
    /// Target platform.
    #[arg(long)]
    pub platform: ResolvePlatform,
    /// Target architecture.
    #[arg(long)]
    pub architecture: ResolveArchitecture,
    /// Numeric Apple-style macOS version.
    #[arg(long)]
    pub macos: Option<String>,
}

/// Resolver platform names are the server's public wire values.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ResolvePlatform {
    #[value(name = "mac_os", alias = "macos")]
    MacOs,
    Linux,
    Windows,
}
impl ResolvePlatform {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MacOs => "mac_os",
            Self::Linux => "linux",
            Self::Windows => "windows",
        }
    }
}

/// A concrete target CPU, distinct from a universal artifact's architecture.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ResolveArchitecture {
    #[value(name = "aarch64", alias = "arm64")]
    Aarch64,
    #[value(name = "x86_64")]
    X86_64,
}
impl ResolveArchitecture {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aarch64 => "aarch64",
            Self::X86_64 => "x86_64",
        }
    }
}

/// Export one exact promoted delivery snapshot. Does not publish a remote Munki repository.
#[derive(Debug, Args)]
pub struct MunkiExportArgs {
    #[command(flatten)]
    pub target: ResolveArgs,
    /// New directory containing pkgs and pkgsinfo. Existing directories are never replaced.
    #[arg(long)]
    pub output: PathBuf,
    /// Reviewed Munki pkginfo JSON template; otherwise use software installation/detection metadata.
    #[arg(long)]
    pub pkginfo_template: Option<PathBuf>,
    /// Installer format, explicitly reviewed independently of the digest.
    #[arg(long, value_parser = ["pkg", "dmg"])]
    pub extension: String,
}

/// Saved software selections and reviewed Munki snapshots.
#[derive(Debug, Args)]
pub struct ExportArgs {
    #[command(subcommand)]
    pub command: ExportCommand,
}
#[derive(Debug, Subcommand)]
pub enum ExportCommand {
    /// List saved export definitions shared with the console.
    List(PageArgs),
    /// Show a definition and its current publication generation.
    Show { export: String },
    /// Create or replace a draft from JSON; publication is a separate reviewed step.
    Save {
        #[arg(long)]
        file: PathBuf,
        /// Existing export to update; requires --revision from exports show.
        #[arg(long, requires = "revision")]
        export: Option<String>,
        #[arg(long, requires = "export")]
        revision: Option<u64>,
    },
    /// Preview the entire batch and optionally save the exact plan for apply.
    Plan {
        export: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Publish a saved reviewed plan atomically; rejects stale previews.
    Apply {
        #[arg(long)]
        plan_file: PathBuf,
    },
    /// List immutable publication history (after is the previous generation cursor).
    History {
        export: String,
        #[arg(long, default_value_t = 0)]
        after: u64,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Download a verified snapshot into OUTPUT/repository; existing paths are never replaced.
    Download {
        export: String,
        #[arg(long)]
        generation: Option<u64>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Save an owner-only Munki device profile with a new export-only credential.
    Profile {
        export: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        test_all: bool,
    },
    /// Revoke every earlier profile for this export; replacement profiles must be distributed.
    RevokeProfiles { export: String },
}
