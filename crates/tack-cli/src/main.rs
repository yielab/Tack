use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use serde_json::json;

use tack_cli::client::TackClient;
use tack_cli::config::{self, Config};
use tack_cli::execution;
use tack_cli::git;
use tack_cli::vocab;

mod doctor;
mod local_enrollment;
mod local_runner;
mod secret;
mod service;

// ─── CLI structure ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "tack",
    version,
    about = "Tack — Flexible Project Management CLI"
)]
struct Cli {
    /// Tack API base URL
    #[arg(long, env = "TACK_API_URL")]
    api_url: Option<String>,

    /// Bearer token for authentication
    #[arg(long, env = "TACK_API_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Tack server and web UI (this is the default when you run
    /// `tack` with no subcommand)
    Serve {
        /// Also start an embedded runner in this process, speaking runner-v1
        /// over loopback HTTP exactly like a remote runner. Off by default;
        /// also settable via TACK_LOCAL_RUNNER_ENABLE=1. Refused on startup
        /// if the server is not bound to loopback.
        #[arg(long)]
        with_runner: bool,
    },

    /// Create a new project
    Init {
        /// Project name
        name: String,
        /// Project type (software, web, mobile, construction, personal, homework, maintenance, legal, research, event, custom)
        #[arg(short = 't', long, default_value = "software")]
        r#type: String,
        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// List projects
    Projects {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Add a new item to a project
    Add {
        /// Item title
        title: String,
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Item type (task, epic, feature, bug, subtask, requirement)
        #[arg(short = 't', long, default_value = "task")]
        item_type: String,
        /// Priority (critical, high, medium, low)
        #[arg(long, default_value = "medium")]
        priority: String,
        /// Parent item ID
        #[arg(long)]
        parent: Option<String>,
        /// Assignee name
        #[arg(short, long)]
        assignee: Option<String>,
        /// Sprint ID
        #[arg(short, long)]
        sprint: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// List items in a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,
        /// Filter by item type
        #[arg(long)]
        item_type: Option<String>,
        /// Filter by assignee
        #[arg(short, long)]
        assignee: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Move an item to a new status
    Move {
        /// Item ID
        id: String,
        /// Target status
        status: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Show board view for a project
    Board {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Derive a git branch name from an item (and optionally create it)
    Branch {
        /// Item ID
        id: String,
        /// Create and switch to the branch (runs `git checkout -b`)
        #[arg(short, long)]
        checkout: bool,
        /// Override the type-derived prefix (e.g. `hotfix`, `wip`)
        #[arg(long)]
        prefix: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Run a Model Context Protocol (MCP) server over stdio for AI agents
    Mcp,

    /// Search items
    Search {
        /// Search query
        query: String,
        /// Project ID (optional; searches globally if omitted)
        #[arg(short = 'p', long)]
        project: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Sprint management
    Sprint {
        #[command(subcommand)]
        action: SprintAction,
    },

    /// Write ~/.tackrc with connection settings
    Config {
        /// API base URL to save
        #[arg(long)]
        url: Option<String>,
        /// Bearer token to save (omit to clear)
        #[arg(long)]
        token: Option<String>,
        /// Show current config without changing it
        #[arg(long)]
        show: bool,
    },

    /// Print shell completion script to stdout
    Completions {
        /// Target shell (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },

    /// Download a local backup or trigger a remote cloud backup
    Backup {
        /// Where to save the local backup (default: tack-backup.db). Ignored with --remote.
        path: Option<std::path::PathBuf>,
        /// Upload a new backup to the configured S3-compatible cloud store instead of downloading locally
        #[arg(long)]
        remote: bool,
        /// Output raw JSON (only applies to --remote)
        #[arg(long)]
        json: bool,
    },

    /// List remote cloud backups
    Backups {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Stage a backup file for restore on next server restart
    Restore {
        /// Path to the backup file produced by `tack backup` (omit with --remote)
        path: Option<std::path::PathBuf>,
        /// Restore from remote cloud storage instead of a local file
        #[arg(long)]
        remote: bool,
        /// Specific remote backup key to restore (defaults to latest when using --remote)
        #[arg(long)]
        key: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage project templates
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },

    /// Manage project roles (specialties / disciplines)
    Role {
        #[command(subcommand)]
        action: RoleAction,
    },

    /// Manage item comments
    Comment {
        #[command(subcommand)]
        action: CommentAction,
    },

    /// Manage custom field definitions and item values
    Field {
        #[command(subcommand)]
        action: FieldAction,
    },

    /// Create/list/cancel/reconcile agent-fleet execution requests
    Execution {
        #[command(subcommand)]
        action: ExecutionAction,
    },

    /// Manage runner fleets
    Fleet {
        #[command(subcommand)]
        action: FleetAction,
    },

    /// Enroll and revoke execution runners
    Runner {
        #[command(subcommand)]
        action: RunnerAction,
    },

    /// Manage tack as a background service that outlives the terminal
    /// (a systemd user unit on Linux, a launchd agent on macOS)
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Manage agent profiles (instructions, tool policy, limits)
    AgentProfile {
        #[command(subcommand)]
        action: AgentProfileAction,
    },

    /// Manage model profiles (provider + model id combinations)
    ModelProfile {
        #[command(subcommand)]
        action: ModelProfileAction,
    },

    /// Trigger a docket pipeline run for a project's linked docket project
    Orch {
        #[command(subcommand)]
        action: OrchAction,
    },
}

#[derive(Subcommand)]
enum SprintAction {
    /// Create a new sprint
    Create {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Sprint name
        name: String,
        /// Sprint goal
        #[arg(short, long)]
        goal: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Start a sprint (sets status to active)
    Start {
        /// Sprint ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Move sprint to review
    Review {
        /// Sprint ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Close a sprint
    Close {
        /// Sprint ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List sprints in a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum TemplateAction {
    /// List available templates
    List {
        /// Filter by project type (software, web, mobile, construction, personal, homework, maintenance, legal, research, event, custom)
        #[arg(short = 't', long)]
        project_type: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Show details of a template
    Show {
        /// Template ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Create a new project from a template
    CreateFrom {
        /// Template ID
        id: String,
        /// New project name
        name: String,
        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum RoleAction {
    /// List roles defined in a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Create a new role in a project
    Create {
        /// Role name
        name: String,
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Hex color (e.g. #4A90D9)
        #[arg(long)]
        color: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete a role
    Delete {
        /// Role ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Assign a role to an item
    Assign {
        /// Item ID
        item: String,
        /// Role ID
        role: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a role assignment from an item
    Unassign {
        /// Item ID
        item: String,
        /// Role ID
        role: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum CommentAction {
    /// List comments on an item
    List {
        /// Item ID
        item: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a comment to an item
    Add {
        /// Item ID
        item: String,
        /// Comment text
        content: String,
        /// Author name
        #[arg(short, long)]
        author: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum FieldAction {
    /// List custom field definitions for a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Create a custom field definition in a project
    Create {
        /// Field name
        name: String,
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Field type: text, long_text, number, date, boolean, select, multi_select, url, email
        #[arg(short = 't', long)]
        field_type: String,
        /// Mark the field as required
        #[arg(long)]
        required: bool,
        /// Comma-separated options (for select / multi_select types)
        #[arg(long)]
        options: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete a custom field definition
    Delete {
        /// Field ID
        id: String,
    },
    /// List all custom field values set on an item
    Values {
        /// Item ID
        item: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Set a custom field value on an item
    Set {
        /// Item ID
        item: String,
        /// Field ID
        field: String,
        /// Value (parsed as JSON if possible, otherwise treated as a string)
        value: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a custom field value from an item
    Unset {
        /// Item ID
        item: String,
        /// Field ID
        field: String,
    },
}

/// `execution create`'s flags, boxed at the `ExecutionAction::Create` call
/// site below — `CreateExecution` has by far the widest field set of any
/// command in this CLI, and clippy's `large_enum_variant` is right that
/// inlining it makes every other `ExecutionAction`/`Commands` variant pay
/// for stack space it doesn't use. A plain `#[derive(clap::Args)]` struct
/// behind a `Box` keeps the flags exactly as they'd read inline (`clap`
/// flattens them back out for parsing) without that cost.
#[derive(clap::Args)]
struct ExecutionCreateArgs {
    /// Item ID this execution is for
    item_id: String,
    /// Idempotency key (defaults to a fresh one each call; pass a stable
    /// value to safely retry the exact same request)
    #[arg(long)]
    idempotency_key: Option<String>,
    /// Run on this exact runner ID (mutually exclusive with --fleet)
    #[arg(long)]
    runner: Option<String>,
    /// Run on any eligible runner in this fleet (mutually exclusive with --runner)
    #[arg(long)]
    fleet: Option<String>,
    /// Agent profile ID
    #[arg(long)]
    agent_profile: String,
    /// Requested harness kind (e.g. codex, claude_code)
    #[arg(long)]
    harness: String,
    /// Requested model provider (omit to allow auto-selection)
    #[arg(long)]
    model_provider: Option<String>,
    /// Requested opaque model id (omit to allow auto-selection)
    #[arg(long)]
    model_id: Option<String>,
    /// Resolved agent-profile snapshot, as a JSON object. Required fields:
    /// name, instructions, tool_policy (object), timeout_seconds, budgets
    /// (object) — there is no safe empty default for this field.
    #[arg(long)]
    agent_profile_snapshot: String,
    /// Repository/workspace reference, as a JSON object. Required fields:
    /// kind, remote, base_revision (optional: subdirectory).
    #[arg(long)]
    repository: String,
    /// Permission/tool policy, as a JSON object. Required field: network
    /// (bool). Optional: tools (array of strings, default []) — there is no
    /// safe empty default for this field (network has none server-side).
    #[arg(long)]
    permission_policy: String,
    /// Budgets, as a JSON object (default: {})
    #[arg(long)]
    budgets: Option<String>,
    /// Bounded environment metadata, as a JSON object (default: {})
    #[arg(long)]
    environment: Option<String>,
    /// Free-form metadata, as a JSON object (default: {})
    #[arg(long)]
    metadata: Option<String>,
    /// Execution timeout, in seconds
    #[arg(long)]
    timeout_seconds: u64,
    /// Optional status-map policy ID
    #[arg(long)]
    status_map_policy: Option<String>,
    /// Output raw JSON
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum ExecutionAction {
    /// Create (or idempotently replay) an execution request
    Create {
        #[command(flatten)]
        args: Box<ExecutionCreateArgs>,
    },
    /// List execution requests (newest first)
    List {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Get one execution request's current lifecycle state
    Get {
        /// Execution request ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Request cancellation of an execution (recorded as a request only —
    /// the runner observes and reports the actual outcome)
    Cancel {
        /// Execution request ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Requeue a needs_operator execution after an audited recovery decision
    Reconcile {
        /// Execution request ID
        id: String,
        /// Idempotency key scoping this recovery confirmation
        #[arg(long)]
        recovery_key: String,
        /// Human-readable justification for the recovery decision
        #[arg(long)]
        reason: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum FleetAction {
    /// Create a runner fleet
    Create {
        /// Fleet name
        name: String,
        /// Optional concurrency limit
        #[arg(long)]
        concurrency_limit: Option<i64>,
        /// Default policy, as a JSON object (default: {})
        #[arg(long)]
        policy: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List runner fleets
    List {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum RunnerAction {
    /// Create a pending runner and issue a one-time enrollment token.
    ///
    /// The raw token is shown exactly once — by the server, in this
    /// response — and is never stored anywhere retrievable again. Pass
    /// --out to save it straight to a file (written atomically, owner-only)
    /// instead of printing it to the terminal.
    Enroll {
        /// Runner name
        name: String,
        /// Total execution slot capacity
        #[arg(long)]
        total_capacity: i64,
        /// Currently available execution slot capacity
        #[arg(long)]
        available_capacity: i64,
        /// Labels, as a JSON object (default: {})
        #[arg(long)]
        labels: Option<String>,
        /// Initial capability snapshot, as a JSON object (default: {})
        #[arg(long)]
        capability_snapshot: Option<String>,
        /// Runner protocol version this runner will speak (default: 1)
        #[arg(long)]
        protocol_version: Option<i64>,
        /// How long the enrollment token stays redeemable, in seconds (default: 3600)
        #[arg(long)]
        enrollment_lifetime_secs: Option<i64>,
        /// Write the enrollment response to this file (atomic, owner-only)
        /// instead of printing the raw token to the terminal
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Output raw JSON (to stdout; independent of --out)
        #[arg(long)]
        json: bool,
    },
    /// Revoke a runner (its credential stops authenticating immediately)
    Revoke {
        /// Runner ID
        runner_id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Revoke a specific unredeemed enrollment token
    RevokeToken {
        /// Runner ID
        runner_id: String,
        /// Enrollment token ID
        token_id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Run the runner role in this process, speaking runner-v1 over HTTP
    /// against a Tack server exactly like the standalone tack-runner binary.
    Start {
        /// Optional TOML configuration file.
        #[arg(long)]
        config: Option<std::path::PathBuf>,
        /// Runner protocol endpoint. Overrides file and environment configuration.
        #[arg(long)]
        api_url: Option<String>,
        /// Stable identifier sent to the control plane.
        #[arg(long)]
        runner_id: Option<String>,
        /// Local directory for runner state. Overrides file and environment configuration.
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
        /// Enrollment credential. Prefer TACK_RUNNER_ENROLLMENT_TOKEN so it is not visible in shell history.
        #[arg(long, hide_env_values = true)]
        enrollment_token: Option<String>,
    },
    /// Report which harness binaries this machine has, what each declares
    /// it can do, and where its credentials come from — the same
    /// discovery/capability probe `runner start` performs, without
    /// enrolling anything or requiring a server.
    Doctor {
        /// Output the raw capability snapshot as JSON instead of the
        /// human-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Manage this machine's runner-local secret store — the provider keys
    /// a harness's environment can reference via `secret_reference` without
    /// ever putting them in a request body, a log line, or the board's own
    /// database.
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },
}

#[derive(Subcommand)]
enum SecretAction {
    /// Store a secret under `name` (e.g. `vercel-ai-gateway/default`). The
    /// value is read from TACK_RUNNER_SECRET_VALUE or, if unset, from
    /// stdin — never from argv.
    Set {
        /// Entry name
        name: String,
        /// Local directory for runner state. Overrides file and environment configuration.
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
    },
    /// List the entry names held in the store. Never prints a value.
    List {
        /// Local directory for runner state. Overrides file and environment configuration.
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a secret. Not an error if it was already absent.
    Remove {
        /// Entry name
        name: String,
        /// Local directory for runner state. Overrides file and environment configuration.
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Write the service unit and start it, enabled to start at login
    Install,
    /// Stop the service and remove its unit. The data root is left untouched.
    Uninstall,
    /// Show whether the service is active and where to check its health
    Status,
}

#[derive(Subcommand)]
enum AgentProfileAction {
    /// Create an agent profile
    Create {
        /// Profile name
        name: String,
        /// Agent instructions
        #[arg(long)]
        instructions: String,
        /// Tool policy, as a JSON object (default: {})
        #[arg(long)]
        tool_policy: Option<String>,
        /// Limits, as a JSON object (default: {})
        #[arg(long)]
        limits: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List agent profiles
    List {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ModelProfileAction {
    /// Create a model profile
    Create {
        /// Profile name
        name: String,
        /// Model provider
        #[arg(long)]
        provider: String,
        /// Opaque model id
        #[arg(long)]
        model_id: String,
        /// Optional config reference
        #[arg(long)]
        config_reference: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List model profiles
    List {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum OrchAction {
    /// Trigger a docket pipeline run for a project's linked docket project.
    ///
    /// docket answers before the pipeline itself runs, so this command can
    /// only ever report that the run *started* — never that it was
    /// permitted. Whether it succeeds, fails, or hits a guardrail block only
    /// becomes visible once Tack's reconciler next polls docket and mirrors
    /// that run's outcome.
    Dispatch {
        /// Tack project ID (must already be linked via the project's
        /// docket link)
        project: String,
        /// Pipeline variables, as a JSON object (default: {})
        #[arg(long)]
        variables: Option<String>,
        /// TACK_ORCH_DISPATCH_TOKEN — this server refuses the request
        /// outright when it's unset, the same fail-closed default
        /// TACK_ORCH_APPROVAL_TOKEN already uses
        #[arg(long, env = "TACK_ORCH_DISPATCH_TOKEN")]
        dispatch_token: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Bare `tack` (no subcommand) launches the server + web UI — the primary,
    // UI-first experience. `tack serve` does the same explicitly.
    let Some(command) = cli.command else {
        return run_server(local_runner::with_runner_enabled(false));
    };

    // Serve, Runner Start, Runner Doctor, Service, Config and Completions
    // don't need a live API client: Serve and Runner Start each build their
    // own async runtime and speak the runner-v1/HTTP protocol directly,
    // never through `TackClient`; Runner Doctor only probes this machine's
    // own harness installations; Service only writes/reads a unit file and
    // shells out to the OS's own service manager.
    let command = match command {
        Commands::Serve { with_runner } => {
            return run_server(local_runner::with_runner_enabled(with_runner));
        }
        Commands::Runner {
            action:
                RunnerAction::Start {
                    config,
                    api_url,
                    runner_id,
                    state_dir,
                    enrollment_token,
                },
        } => {
            return local_runner::run_standalone(
                config,
                api_url,
                runner_id,
                state_dir,
                enrollment_token,
            );
        }
        Commands::Runner {
            action: RunnerAction::Doctor { json },
        } => {
            return doctor::run(json);
        }
        Commands::Runner {
            action: RunnerAction::Secret { action },
        } => {
            return secret::run(action);
        }
        Commands::Service { action } => {
            return match action {
                ServiceAction::Install => service::install(),
                ServiceAction::Uninstall => service::uninstall(),
                ServiceAction::Status => service::status(),
            };
        }
        Commands::Config { url, token, show } => {
            return cmd_config(
                url.as_deref(),
                token.as_deref(),
                show,
                &cli.api_url,
                &cli.token,
            );
        }
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "tack", &mut std::io::stdout());
            return Ok(());
        }
        other => other,
    };

    let config = Config::load(cli.api_url, cli.token);
    let client = TackClient::new(&config)?;

    match command {
        Commands::Init {
            name,
            r#type,
            description,
            json,
        } => cmd_init(&client, name, r#type, description, json),

        Commands::Projects { json } => cmd_projects(&client, json),

        Commands::Add {
            title,
            project,
            item_type,
            priority,
            parent,
            assignee,
            sprint,
            json,
        } => cmd_add(
            &client, title, project, item_type, priority, parent, assignee, sprint, json,
        ),

        Commands::List {
            project,
            status,
            item_type,
            assignee,
            json,
        } => cmd_list(&client, project, status, item_type, assignee, json),

        Commands::Move { id, status, json } => cmd_move(&client, id, status, json),

        Commands::Board { project, json } => cmd_board(&client, project, json),

        Commands::Branch {
            id,
            checkout,
            prefix,
            json,
        } => cmd_branch(&client, id, checkout, prefix, json),

        Commands::Mcp => tack_cli::mcp::run(&client),

        Commands::Search {
            query,
            project,
            json,
        } => cmd_search(&client, query, project, json),

        Commands::Backup { path, remote, json } => {
            if remote {
                cmd_backup_remote(&client, json)
            } else {
                cmd_backup(&client, path)
            }
        }

        Commands::Backups { json } => cmd_list_remote_backups(&client, json),

        Commands::Restore {
            path,
            remote,
            key,
            json,
        } => {
            if remote {
                cmd_restore_remote(&client, key, json)
            } else {
                let p = path
                    .ok_or_else(|| anyhow::anyhow!("'path' is required when not using --remote"))?;
                cmd_restore(&client, p, json)
            }
        }

        Commands::Template { action } => match action {
            TemplateAction::List { project_type, json } => {
                cmd_template_list(&client, project_type, json)
            }
            TemplateAction::Show { id, json } => cmd_template_show(&client, id, json),
            TemplateAction::CreateFrom {
                id,
                name,
                description,
                json,
            } => cmd_template_create_from(&client, id, name, description, json),
        },

        Commands::Role { action } => match action {
            RoleAction::List { project, json } => cmd_role_list(&client, project, json),
            RoleAction::Create {
                name,
                project,
                color,
                json,
            } => cmd_role_create(&client, name, project, color, json),
            RoleAction::Delete { id, json } => cmd_role_delete(&client, id, json),
            RoleAction::Assign { item, role, json } => cmd_role_assign(&client, item, role, json),
            RoleAction::Unassign { item, role, json } => {
                cmd_role_unassign(&client, item, role, json)
            }
        },

        Commands::Comment { action } => match action {
            CommentAction::List { item, json } => cmd_comment_list(&client, item, json),
            CommentAction::Add {
                item,
                content,
                author,
                json,
            } => cmd_comment_add(&client, item, content, author, json),
        },

        Commands::Sprint { action } => match action {
            SprintAction::Create {
                project,
                name,
                goal,
                json,
            } => cmd_sprint_create(&client, project, name, goal, json),
            SprintAction::Start { id, json } => cmd_sprint_status(&client, id, "active", json),
            SprintAction::Review { id, json } => cmd_sprint_status(&client, id, "review", json),
            SprintAction::Close { id, json } => cmd_sprint_status(&client, id, "closed", json),
            SprintAction::List { project, json } => cmd_sprint_list(&client, project, json),
        },

        Commands::Field { action } => match action {
            FieldAction::List { project, json } => cmd_field_list(&client, project, json),
            FieldAction::Create {
                name,
                project,
                field_type,
                required,
                options,
                json,
            } => cmd_field_create(&client, name, project, field_type, required, options, json),
            FieldAction::Delete { id } => cmd_field_delete(&client, id),
            FieldAction::Values { item, json } => cmd_field_values(&client, item, json),
            FieldAction::Set {
                item,
                field,
                value,
                json,
            } => cmd_field_set(&client, item, field, value, json),
            FieldAction::Unset { item, field } => cmd_field_unset(&client, item, field),
        },

        Commands::Execution { action } => match action {
            ExecutionAction::Create { args } => {
                let json = args.json;
                cmd_execution_create(
                    &client,
                    ExecutionCreateOpts {
                        item_id: args.item_id,
                        idempotency_key: args.idempotency_key,
                        runner: args.runner,
                        fleet: args.fleet,
                        agent_profile: args.agent_profile,
                        harness: args.harness,
                        model_provider: args.model_provider,
                        model_id: args.model_id,
                        agent_profile_snapshot: args.agent_profile_snapshot,
                        repository: args.repository,
                        permission_policy: args.permission_policy,
                        budgets: args.budgets,
                        environment: args.environment,
                        metadata: args.metadata,
                        timeout_seconds: args.timeout_seconds,
                        status_map_policy: args.status_map_policy,
                    },
                    json,
                )
            }
            ExecutionAction::List { json } => cmd_execution_list(&client, json),
            ExecutionAction::Get { id, json } => cmd_execution_get(&client, id, json),
            ExecutionAction::Cancel { id, json } => cmd_execution_cancel(&client, id, json),
            ExecutionAction::Reconcile {
                id,
                recovery_key,
                reason,
                json,
            } => cmd_execution_reconcile(&client, id, recovery_key, reason, json),
        },

        Commands::Fleet { action } => match action {
            FleetAction::Create {
                name,
                concurrency_limit,
                policy,
                json,
            } => cmd_fleet_create(&client, name, concurrency_limit, policy, json),
            FleetAction::List { json } => cmd_fleet_list(&client, json),
        },

        Commands::Runner { action } => match action {
            RunnerAction::Enroll {
                name,
                total_capacity,
                available_capacity,
                labels,
                capability_snapshot,
                protocol_version,
                enrollment_lifetime_secs,
                out,
                json,
            } => cmd_runner_enroll(
                &client,
                RunnerEnrollOpts {
                    name,
                    total_capacity,
                    available_capacity,
                    labels,
                    capability_snapshot,
                    protocol_version,
                    enrollment_lifetime_secs,
                },
                out,
                json,
            ),
            RunnerAction::Revoke { runner_id, json } => cmd_runner_revoke(&client, runner_id, json),
            RunnerAction::RevokeToken {
                runner_id,
                token_id,
                json,
            } => cmd_runner_revoke_token(&client, runner_id, token_id, json),
            // Already handled above; unreachable but required for exhaustiveness.
            RunnerAction::Start { .. } => unreachable!(),
            RunnerAction::Doctor { .. } => unreachable!(),
            RunnerAction::Secret { .. } => unreachable!(),
        },

        // Already handled above; unreachable but required for exhaustiveness.
        Commands::Service { .. } => unreachable!(),

        Commands::AgentProfile { action } => match action {
            AgentProfileAction::Create {
                name,
                instructions,
                tool_policy,
                limits,
                json,
            } => cmd_agent_profile_create(&client, name, instructions, tool_policy, limits, json),
            AgentProfileAction::List { json } => cmd_agent_profile_list(&client, json),
        },

        Commands::ModelProfile { action } => match action {
            ModelProfileAction::Create {
                name,
                provider,
                model_id,
                config_reference,
                json,
            } => {
                cmd_model_profile_create(&client, name, provider, model_id, config_reference, json)
            }
            ModelProfileAction::List { json } => cmd_model_profile_list(&client, json),
        },

        Commands::Orch { action } => match action {
            OrchAction::Dispatch {
                project,
                variables,
                dispatch_token,
                json,
            } => cmd_orch_dispatch(&client, project, variables, dispatch_token, json),
        },

        // Already handled above; unreachable but required for exhaustiveness.
        Commands::Serve { .. } | Commands::Config { .. } | Commands::Completions { .. } => {
            unreachable!()
        }
    }
}

/// Start the in-process HTTP server (the app + embedded web UI) with an
/// embedded runner always wired in — see `local_runner::serve`'s own doc
/// comment for why even a plain `tack serve` needs one (ADR 0061 decisions
/// 2 and 6: the UI can turn it on later, with no restart). `with_runner`
/// only decides whether it also starts immediately at boot, folded into
/// `TACK_LOCAL_RUNNER_ENABLE` here so `AppConfig::load()` (inside
/// `local_runner::serve`) picks it up the same way it would from the
/// environment — one code path, not a CLI-flag-only branch and a
/// UI-only branch.
///
/// The rest of the CLI is synchronous (it uses a blocking HTTP client), so we
/// build a Tokio runtime on demand here rather than making `main` async.
fn run_server(with_runner: bool) -> anyhow::Result<()> {
    if with_runner {
        // SAFETY: single-threaded at this point in `main` — no other code
        // has spawned a thread yet, so nothing else can race this read/write
        // of the process environment.
        unsafe {
            std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", "1");
        }
    }
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(local_runner::serve())
}

// ─── Command implementations ─────────────────────────────────────────────────

fn cmd_config(
    url: Option<&str>,
    token: Option<&str>,
    show: bool,
    cli_url: &Option<String>,
    cli_token: &Option<String>,
) -> anyhow::Result<()> {
    if show || (url.is_none() && token.is_none()) {
        let cfg = Config::load(cli_url.clone(), cli_token.clone());
        println!("base_url: {}", cfg.base_url);
        match &cfg.token {
            Some(t) => println!("token:    {}", "*".repeat(t.len().min(8))),
            None => println!("token:    (none)"),
        }
        return Ok(());
    }
    let base_url = url.unwrap_or("http://127.0.0.1:3210");
    config::save(base_url, token)?;
    println!("Saved to ~/.tackrc");
    println!("  base_url: {base_url}");
    if token.is_some() {
        println!("  token:    (set)");
    }
    Ok(())
}

fn cmd_init(
    client: &TackClient,
    name: String,
    project_type: String,
    description: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({
        "name": name,
        "project_type": project_type,
        "description": description,
    });
    let resp = client.post("/projects", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!("Created project: {} ({})", name, &id[..8.min(id.len())]);
    println!("  type: {project_type}");
    println!("  id:   {id}");
    Ok(())
}

fn cmd_projects(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/projects")?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let projects = resp.as_array().unwrap_or(&empty);
    if projects.is_empty() {
        println!("No projects found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "TYPE", "UPDATED"]);
    for p in projects {
        print_table_row(&[
            short_id(p["id"].as_str()),
            p["name"].as_str().unwrap_or("?"),
            p["project_type"].as_str().unwrap_or("?"),
            short_date(p["updated_at"].as_str()),
        ]);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_add(
    client: &TackClient,
    title: String,
    project: String,
    item_type: String,
    priority: String,
    parent: Option<String>,
    assignee: Option<String>,
    sprint: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let mut body = json!({
        "title": title,
        "item_type": item_type,
        "priority": priority,
    });
    if let Some(p) = parent {
        body["parent_id"] = json!(p);
    }
    if let Some(a) = assignee {
        body["assignee"] = json!(a);
    }
    if let Some(s) = sprint {
        body["sprint_id"] = json!(s);
    }
    let resp = client.post(&format!("/projects/{project}/items"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    // Translate item_type through the project's vocabulary.
    let v = vocab::fetch(client, &project);
    let type_label = vocab::term(&v, &item_type);
    println!(
        "Created {}: {} ({})",
        type_label,
        title,
        &id[..8.min(id.len())]
    );
    println!("  priority: {priority}");
    println!("  id:       {id}");
    Ok(())
}

fn cmd_list(
    client: &TackClient,
    project: String,
    status: Option<String>,
    item_type: Option<String>,
    assignee: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let mut params = vec![];
    if let Some(s) = &status {
        params.push(format!("status={}", urlenc(s)));
    }
    if let Some(t) = &item_type {
        params.push(format!("item_type={}", urlenc(t)));
    }
    if let Some(a) = &assignee {
        params.push(format!("assignee={}", urlenc(a)));
    }
    let qs = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let resp = client.get(&format!("/projects/{project}/items{qs}"))?;
    // The list endpoint returns a `{ data, total, page, per_page }` envelope;
    // unwrap the `data` array (tolerating a bare array for forward/back compat).
    let items_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&items_val)?);
        return Ok(());
    }
    let empty = vec![];
    let items = items_val.as_array().unwrap_or(&empty);
    if items.is_empty() {
        println!("No items found.");
        return Ok(());
    }
    let v = vocab::fetch(client, &project);
    print_table_header(&["ID", "TITLE", "TYPE", "STATUS", "PRIORITY", "ASSIGNEE"]);
    for item in items {
        let raw_type = item["item_type"].as_str().unwrap_or("task");
        print_table_row(&[
            short_id(item["id"].as_str()),
            item["title"].as_str().unwrap_or("?"),
            vocab::term(&v, raw_type),
            item["status"].as_str().unwrap_or("?"),
            item["priority"].as_str().unwrap_or("?"),
            item["assignee"].as_str().unwrap_or("-"),
        ]);
    }
    Ok(())
}

fn cmd_move(client: &TackClient, id: String, status: String, as_json: bool) -> anyhow::Result<()> {
    let body = json!({ "status": status });
    let resp = client.patch(&format!("/items/{id}"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Moved {}: → {status}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_branch(
    client: &TackClient,
    id: String,
    checkout: bool,
    prefix: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.get(&format!("/items/{id}"))?;
    let item = resp.get("item").unwrap_or(&resp);

    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let item_type = item
        .get("item_type")
        .and_then(|v| v.as_str())
        .unwrap_or("task");
    // The server echoes the canonical id; fall back to the user-supplied one.
    let item_id = item.get("id").and_then(|v| v.as_str()).unwrap_or(&id);

    let branch = git::branch_name(item_type, item_id, title, prefix.as_deref());

    if checkout {
        let status = std::process::Command::new("git")
            .args(["checkout", "-b", &branch])
            .status()
            .map_err(|e| anyhow::anyhow!("failed to run git: {e}"))?;
        if !status.success() {
            anyhow::bail!("git checkout -b {branch} failed");
        }
    }

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "branch": branch,
                "item_id": item_id,
                "checked_out": checkout,
            }))?
        );
        return Ok(());
    }

    if checkout {
        println!("Switched to a new branch '{branch}'");
    } else {
        // Print the command so it can be eval'd or copy-pasted; the bare branch
        // name goes to stderr context-free for scripting via the last word.
        println!("git checkout -b {branch}");
    }
    Ok(())
}

fn cmd_board(client: &TackClient, project: String, as_json: bool) -> anyhow::Result<()> {
    let boards = client.get(&format!("/projects/{project}/boards"))?;
    let empty = vec![];
    let boards = boards.as_array().unwrap_or(&empty);

    let board_id = boards
        .iter()
        .find(|b| b["is_default"].as_bool().unwrap_or(false))
        .or_else(|| boards.first())
        .and_then(|b| b["id"].as_str())
        .ok_or_else(|| anyhow::anyhow!("No boards found for project {project}"))?;

    let view = client.get(&format!("/boards/{board_id}/view"))?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }

    let v = vocab::fetch(client, &project);
    let board_name = view["name"].as_str().unwrap_or("Board");
    println!("── {board_name} ────────────────────────────");

    let cols = view["columns"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    for col in cols {
        let name = col["name"].as_str().unwrap_or("?");
        let items = col["items"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        let wip = col["wip_limit"].as_u64();
        let exceeded = col["wip_exceeded"].as_bool().unwrap_or(false);

        let wip_label = match wip {
            Some(limit) if exceeded => format!(" ({}/{limit} ⚠)", items.len()),
            Some(limit) => format!(" ({}/{})", items.len(), limit),
            None => format!(" ({})", items.len()),
        };

        println!("\n{name}{wip_label}");
        println!("{}", "─".repeat(name.len() + wip_label.len()));

        if items.is_empty() {
            println!("  (empty)");
        }
        for item in items {
            let title = item["title"].as_str().unwrap_or("?");
            let raw_type = item["item_type"].as_str().unwrap_or("task");
            let type_label = vocab::term(&v, raw_type);
            let assignee = item["assignee"].as_str();
            let suffix = match assignee {
                Some(a) => format!(" [{a}]"),
                None => String::new(),
            };
            println!("  · {title} ({type_label}){suffix}");
        }
    }
    Ok(())
}

fn cmd_search(
    client: &TackClient,
    query: String,
    project: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let path = match &project {
        Some(pid) => format!("/projects/{pid}/search?q={}", urlenc(&query)),
        None => format!("/search?q={}", urlenc(&query)),
    };
    let resp = client.get(&path)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let items = resp.as_array().unwrap_or(&empty);
    if items.is_empty() {
        println!("No results for '{query}'.");
        return Ok(());
    }
    println!("{} result(s) for '{query}':", items.len());
    print_table_header(&["ID", "TITLE", "STATUS", "PROJECT"]);
    for item in items {
        print_table_row(&[
            short_id(item["id"].as_str()),
            item["title"].as_str().unwrap_or("?"),
            item["status"].as_str().unwrap_or("?"),
            short_id(item["project_id"].as_str()),
        ]);
    }
    Ok(())
}

fn cmd_sprint_create(
    client: &TackClient,
    project: String,
    name: String,
    goal: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "name": name, "goal": goal });
    let resp = client.post(&format!("/projects/{project}/sprints"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    // Translate "sprint" via project vocabulary.
    let v = vocab::fetch(client, &project);
    let sprint_label = vocab::term(&v, "sprint");
    println!(
        "Created {sprint_label}: {} ({})",
        name,
        &id[..8.min(id.len())]
    );
    println!("  id:     {id}");
    println!("  status: planning");
    Ok(())
}

fn cmd_sprint_status(
    client: &TackClient,
    id: String,
    status: &str,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "status": status });
    let resp = client.patch(&format!("/sprints/{id}/status"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Sprint {}: → {status}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_sprint_list(client: &TackClient, project: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/projects/{project}/sprints"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let sprints = resp.as_array().unwrap_or(&empty);
    if sprints.is_empty() {
        println!("No sprints found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "STATUS", "START", "END"]);
    for s in sprints {
        print_table_row(&[
            short_id(s["id"].as_str()),
            s["name"].as_str().unwrap_or("?"),
            s["status"].as_str().unwrap_or("?"),
            short_date(s["start_date"].as_str()),
            short_date(s["end_date"].as_str()),
        ]);
    }
    Ok(())
}

fn cmd_template_list(
    client: &TackClient,
    project_type: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let path = match &project_type {
        Some(pt) => format!("/templates?project_type={}", urlenc(pt)),
        None => "/templates".to_string(),
    };
    let resp = client.get(&path)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let templates = resp.as_array().unwrap_or(&empty);
    if templates.is_empty() {
        println!("No templates found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "TYPE", "BUILTIN"]);
    for t in templates {
        let builtin = if t["is_builtin"].as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        };
        print_table_row(&[
            short_id(t["id"].as_str()),
            t["name"].as_str().unwrap_or("?"),
            t["project_type"].as_str().unwrap_or("?"),
            builtin,
        ]);
    }
    Ok(())
}

fn cmd_template_show(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/templates/{id}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let name = resp["name"].as_str().unwrap_or("?");
    let desc = resp["description"].as_str().unwrap_or("(no description)");
    let ptype = resp["project_type"].as_str().unwrap_or("?");
    let builtin = if resp["is_builtin"].as_bool().unwrap_or(false) {
        " [built-in]"
    } else {
        ""
    };
    println!("{name}{builtin}");
    println!("  type:        {ptype}");
    println!("  description: {desc}");

    // Workflow statuses
    if let Some(statuses) = resp["workflow"]["statuses"].as_array() {
        let names: Vec<&str> = statuses.iter().filter_map(|s| s["name"].as_str()).collect();
        println!("  statuses:    {}", names.join(" → "));
    }

    // Vocabulary overrides
    if let Some(vocab) = resp["vocabulary"].as_object() {
        let overrides: Vec<String> = vocab
            .iter()
            .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("?")))
            .collect();
        if !overrides.is_empty() {
            println!("  vocabulary:  {}", overrides.join(", "));
        }
    }

    // Custom fields
    if let Some(fields) = resp["custom_fields"].as_array()
        && !fields.is_empty()
    {
        let field_names: Vec<&str> = fields.iter().filter_map(|f| f["name"].as_str()).collect();
        println!("  fields:      {}", field_names.join(", "));
    }

    // Boards
    if let Some(boards) = resp["default_boards"].as_array()
        && !boards.is_empty()
    {
        let board_names: Vec<&str> = boards.iter().filter_map(|b| b["name"].as_str()).collect();
        println!("  boards:      {}", board_names.join(", "));
    }

    println!("  id:          {id}");
    Ok(())
}

fn cmd_template_create_from(
    client: &TackClient,
    template_id: String,
    name: String,
    description: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "name": name, "description": description });
    let resp = client.post(&format!("/projects/from-template/{template_id}"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    let ptype = resp["project_type"].as_str().unwrap_or("?");
    println!("Created project: {} ({})", name, &id[..8.min(id.len())]);
    println!("  type:     {ptype}");
    println!("  template: {}", &template_id[..8.min(template_id.len())]);
    println!("  id:       {id}");
    Ok(())
}

// ─── Role commands ────────────────────────────────────────────────────────────

fn cmd_role_list(client: &TackClient, project: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/projects/{project}/roles"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let roles = resp.as_array().unwrap_or(&empty);
    if roles.is_empty() {
        println!("No roles defined.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "COLOR"]);
    for r in roles {
        print_table_row(&[
            short_id(r["id"].as_str()),
            r["name"].as_str().unwrap_or("?"),
            r["color"].as_str().unwrap_or("-"),
        ]);
    }
    Ok(())
}

fn cmd_role_create(
    client: &TackClient,
    name: String,
    project: String,
    color: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "name": name, "color": color });
    let resp = client.post(&format!("/projects/{project}/roles"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!("Created role: {} ({})", name, &id[..8.min(id.len())]);
    println!("  id:    {id}");
    Ok(())
}

fn cmd_role_delete(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.delete_json(&format!("/roles/{id}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Deleted role: {}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_role_assign(
    client: &TackClient,
    item: String,
    role: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.put_empty(&format!("/items/{item}/roles/{role}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!(
        "Assigned role {} to item {}",
        &role[..8.min(role.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

fn cmd_role_unassign(
    client: &TackClient,
    item: String,
    role: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.delete_json(&format!("/items/{item}/roles/{role}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!(
        "Removed role {} from item {}",
        &role[..8.min(role.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

// ─── Comment commands ─────────────────────────────────────────────────────────

fn cmd_comment_list(client: &TackClient, item: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/items/{item}/comments"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let comments = resp.as_array().unwrap_or(&empty);
    if comments.is_empty() {
        println!("No comments.");
        return Ok(());
    }
    for c in comments {
        let author = c["author"].as_str().unwrap_or("anonymous");
        let created = short_date(c["created_at"].as_str());
        let content = c["content"].as_str().unwrap_or("?");
        println!("[{created}] {author}: {content}");
    }
    Ok(())
}

fn cmd_comment_add(
    client: &TackClient,
    item: String,
    content: String,
    author: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = json!({ "content": content, "author": author });
    let resp = client.post(&format!("/items/{item}/comments"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!("Comment added ({})", &id[..8.min(id.len())]);
    Ok(())
}

// ─── Custom field commands ────────────────────────────────────────────────────

fn cmd_field_list(client: &TackClient, project: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/projects/{project}/custom-fields"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let fields = resp.as_array().unwrap_or(&empty);
    if fields.is_empty() {
        println!("No custom fields defined.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "TYPE", "REQUIRED"]);
    for f in fields {
        let required = if f["required"].as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        };
        print_table_row(&[
            short_id(f["id"].as_str()),
            f["name"].as_str().unwrap_or("?"),
            f["field_type"].as_str().unwrap_or("?"),
            required,
        ]);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_field_create(
    client: &TackClient,
    name: String,
    project: String,
    field_type: String,
    required: bool,
    options: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let opts: Option<Vec<&str>> = options
        .as_deref()
        .map(|s| s.split(',').map(str::trim).collect());
    let body = json!({
        "name": name,
        "field_type": field_type,
        "required": required,
        "options": opts,
    });
    let resp = client.post(&format!("/projects/{project}/custom-fields"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["id"].as_str().unwrap_or("?");
    println!(
        "Created field: {} ({}) [{}]",
        name,
        field_type,
        &id[..8.min(id.len())]
    );
    println!("  id: {id}");
    Ok(())
}

fn cmd_field_delete(client: &TackClient, id: String) -> anyhow::Result<()> {
    client.delete(&format!("/custom-fields/{id}"))?;
    println!("Deleted field: {}", &id[..8.min(id.len())]);
    Ok(())
}

fn cmd_field_values(client: &TackClient, item: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/items/{item}/custom-fields"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let values = resp.as_array().unwrap_or(&empty);
    if values.is_empty() {
        println!("No custom field values.");
        return Ok(());
    }
    for v in values {
        let field_id = short_id(v["field_id"].as_str());
        let value = &v["value"];
        println!("  {field_id}  {value}");
    }
    Ok(())
}

fn cmd_field_set(
    client: &TackClient,
    item: String,
    field: String,
    value: String,
    as_json: bool,
) -> anyhow::Result<()> {
    // Parse as JSON if possible; fall back to a JSON string.
    let json_value: serde_json::Value =
        serde_json::from_str(&value).unwrap_or_else(|_| serde_json::Value::String(value.clone()));
    let resp = client.put_json(&format!("/items/{item}/custom-fields/{field}"), &json_value)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!(
        "Set field {} on item {}",
        &field[..8.min(field.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

fn cmd_field_unset(client: &TackClient, item: String, field: String) -> anyhow::Result<()> {
    client.delete(&format!("/items/{item}/custom-fields/{field}"))?;
    println!(
        "Unset field {} on item {}",
        &field[..8.min(field.len())],
        &item[..8.min(item.len())]
    );
    Ok(())
}

// ─── Execution commands ───────────────────────────────────────────────────────
//
// Every body sent below is built by `tack_cli::execution` — the same module
// `mcp.rs`'s execution tools call into — so the CLI and MCP entry points can
// never construct a differently-shaped request for the same operation:
// a CLI/MCP-issued request produces the same payload
// shape as the UI would. See `execution.rs`'s module doc for why the
// backend's own request structs, not a hand-written contract fixture, are
// the shape authority for this surface.

struct ExecutionCreateOpts {
    item_id: String,
    idempotency_key: Option<String>,
    runner: Option<String>,
    fleet: Option<String>,
    agent_profile: String,
    harness: String,
    model_provider: Option<String>,
    model_id: Option<String>,
    agent_profile_snapshot: String,
    repository: String,
    permission_policy: String,
    budgets: Option<String>,
    environment: Option<String>,
    metadata: Option<String>,
    timeout_seconds: u64,
    status_map_policy: Option<String>,
}

fn cmd_execution_create(
    client: &TackClient,
    opts: ExecutionCreateOpts,
    as_json: bool,
) -> anyhow::Result<()> {
    let selector = execution::selector_from_flags(opts.runner.as_deref(), opts.fleet.as_deref())
        .map_err(|e| anyhow::anyhow!(e))?;
    let args = execution::CreateExecutionArgs {
        item_id: &opts.item_id,
        idempotency_key: opts.idempotency_key.as_deref(),
        agent_profile_id: &opts.agent_profile,
        requested_harness_kind: &opts.harness,
        requested_model_provider: opts.model_provider.as_deref(),
        requested_model_id: opts.model_id.as_deref(),
        agent_profile_snapshot: &opts.agent_profile_snapshot,
        repository_snapshot: &opts.repository,
        permission_policy: &opts.permission_policy,
        budgets: opts.budgets.as_deref(),
        environment: opts.environment.as_deref(),
        metadata: opts.metadata.as_deref(),
        timeout_seconds: opts.timeout_seconds,
        status_map_policy_id: opts.status_map_policy.as_deref(),
    };
    let body =
        execution::build_create_execution_body(&selector, &args).map_err(|e| anyhow::anyhow!(e))?;
    let resp = client.post("/executions", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["request_id"].as_str().unwrap_or("?");
    let state = resp["state"].as_str().unwrap_or("?");
    let replayed = resp["replayed"].as_bool().unwrap_or(false);
    if replayed {
        println!(
            "Replayed existing execution request: {} (idempotency key already used)",
            &id[..8.min(id.len())]
        );
    } else {
        println!("Created execution request: {}", &id[..8.min(id.len())]);
    }
    println!("  state: {state}{}", execution::describe_state(state));
    println!("  id:    {id}");
    Ok(())
}

fn cmd_execution_list(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/executions")?;
    let list_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&list_val)?);
        return Ok(());
    }
    let empty = vec![];
    let rows = list_val.as_array().unwrap_or(&empty);
    if rows.is_empty() {
        println!("No execution requests found.");
        return Ok(());
    }
    print_table_header(&["ID", "ITEM", "STATE", "CREATED"]);
    for r in rows {
        let state = r["state"].as_str().unwrap_or("?");
        let state_label = format!("{state}{}", execution::describe_state(state));
        print_table_row(&[
            short_id(r["request_id"].as_str()),
            short_id(r["item_id"].as_str()),
            &state_label,
            short_date(r["created_at"].as_str()),
        ]);
    }
    Ok(())
}

fn cmd_execution_get(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/executions/{id}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let request_id = resp["request_id"].as_str().unwrap_or(&id);
    let state = resp["state"].as_str().unwrap_or("?");
    println!("Execution request {request_id}");
    println!("  item:    {}", resp["item_id"].as_str().unwrap_or("?"));
    println!("  state:   {state}{}", execution::describe_state(state));
    println!("  created: {}", resp["created_at"].as_str().unwrap_or("?"));
    if let Some(cancel_at) = resp["cancellation_requested_at"].as_str() {
        println!("  cancellation requested at: {cancel_at}");
    }
    // needs_operator/lost are surfaced as visibly
    // distinct outcomes, not collapsed into the same quiet summary as
    // every other state — a single-record view has room to say what to do
    // about it, not just flag that it happened.
    match state {
        "needs_operator" => println!(
            "  \u{26a0} needs_operator: ambiguous outcome — safe retry could not be proven. \
             After confirming it's safe to recover, run \
             `tack execution reconcile {request_id} --recovery-key <key> --reason <text>`."
        ),
        "lost" => println!(
            "  \u{26a0} lost: no known running process was observed for this attempt. \
             Review before requeuing — a lease expiry alone never launches a second process."
        ),
        _ => {}
    }
    Ok(())
}

fn cmd_execution_cancel(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.post(&format!("/executions/{id}/cancel"), &json!({}))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    // The server's `state` here is the literal string "cancellation_requested"
    // — not one of the frozen `tack_orch::execution::ExecutionState` lifecycle states — because cancellation
    // is recorded as a request only; the runner observes and reports the
    // actual outcome (succeeded/failed/cancelled/lost). Say that plainly
    // rather than implying the request is already terminal.
    println!(
        "Cancellation requested for {} — the request is not yet terminal; \
         `tack execution get {id}` will show the outcome once the runner reports it.",
        &id[..8.min(id.len())]
    );
    Ok(())
}

fn cmd_execution_reconcile(
    client: &TackClient,
    id: String,
    recovery_key: String,
    reason: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = execution::build_requeue_body(&recovery_key, &reason);
    let resp = client.post(&format!("/executions/{id}/requeue"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let replayed = resp["replayed"].as_bool().unwrap_or(false);
    if replayed {
        println!(
            "Replayed existing recovery confirmation for {} (recovery key already used)",
            &id[..8.min(id.len())]
        );
    } else {
        println!(
            "Requeued {} from needs_operator — state: queued",
            &id[..8.min(id.len())]
        );
    }
    Ok(())
}

// ─── Fleet commands ───────────────────────────────────────────────────────────

fn cmd_fleet_create(
    client: &TackClient,
    name: String,
    concurrency_limit: Option<i64>,
    policy: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = execution::build_create_fleet_body(&name, concurrency_limit, policy.as_deref())
        .map_err(|e| anyhow::anyhow!(e))?;
    let resp = client.post("/runner-fleets", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["fleet_id"].as_str().unwrap_or("?");
    println!("Created fleet: {} ({})", name, &id[..8.min(id.len())]);
    println!("  id: {id}");
    Ok(())
}

fn cmd_fleet_list(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/runner-fleets")?;
    let list_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&list_val)?);
        return Ok(());
    }
    let empty = vec![];
    let rows = list_val.as_array().unwrap_or(&empty);
    if rows.is_empty() {
        println!("No fleets found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "CONCURRENCY"]);
    for f in rows {
        let concurrency = f["concurrency_limit"]
            .as_i64()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        print_table_row(&[
            short_id(f["fleet_id"].as_str()),
            f["name"].as_str().unwrap_or("?"),
            &concurrency,
        ]);
    }
    Ok(())
}

// ─── Runner commands ──────────────────────────────────────────────────────────

struct RunnerEnrollOpts {
    name: String,
    total_capacity: i64,
    available_capacity: i64,
    labels: Option<String>,
    capability_snapshot: Option<String>,
    protocol_version: Option<i64>,
    enrollment_lifetime_secs: Option<i64>,
}

fn cmd_runner_enroll(
    client: &TackClient,
    opts: RunnerEnrollOpts,
    out: Option<std::path::PathBuf>,
    as_json: bool,
) -> anyhow::Result<()> {
    let args = execution::EnrollRunnerArgs {
        name: &opts.name,
        total_capacity: opts.total_capacity,
        available_capacity: opts.available_capacity,
        labels: opts.labels.as_deref(),
        capability_snapshot: opts.capability_snapshot.as_deref(),
        protocol_version: opts.protocol_version,
        enrollment_lifetime_seconds: opts.enrollment_lifetime_secs,
    };
    let body = execution::build_enroll_runner_body(&args).map_err(|e| anyhow::anyhow!(e))?;
    // The raw enrollment token never flowed in as a CLI argument (it doesn't
    // exist until the server generates it in the response below), so there
    // is nothing here that could have appeared in `ps`/argv on the way in.
    let resp = client.post("/runners/enrollment", &body)?;

    if let Some(path) = &out {
        // Written atomically and owner-only — this file carries the same
        // one-time-secret weight as `~/.tackrc`'s bearer token.
        let contents = serde_json::to_vec_pretty(&resp)?;
        tack_cli::secure_fs::write_owner_only_atomic(path, &contents)
            .map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", path.display()))?;
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    let runner_id = resp["runner_id"].as_str().unwrap_or("?");
    let token_id = resp["token_id"].as_str().unwrap_or("?");
    let expires_at = resp["expires_at"].as_str().unwrap_or("?");
    println!("Enrolled runner: {} ({})", opts.name, runner_id);
    println!("  token id:   {token_id}");
    println!("  expires at: {expires_at}");
    match &out {
        Some(path) => {
            println!(
                "  enrollment token: written to {} (owner-only; not printed here)",
                path.display()
            );
        }
        None => {
            let token = resp["enrollment_token"].as_str().unwrap_or("?");
            println!("  enrollment token: {token}");
            println!(
                "  (shown once — copy it into the runner's TACK_RUNNER_ENROLLMENT_TOKEN now; \
                 it cannot be retrieved again)"
            );
        }
    }
    Ok(())
}

fn cmd_runner_revoke(client: &TackClient, runner_id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.post(&format!("/runners/{runner_id}/revoke"), &json!({}))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Revoked runner: {}", &runner_id[..8.min(runner_id.len())]);
    Ok(())
}

fn cmd_runner_revoke_token(
    client: &TackClient,
    runner_id: String,
    token_id: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.post(
        &format!("/runners/{runner_id}/enrollment-tokens/{token_id}/revoke"),
        &json!({}),
    )?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!(
        "Revoked enrollment token {} for runner {}",
        &token_id[..8.min(token_id.len())],
        &runner_id[..8.min(runner_id.len())]
    );
    Ok(())
}

// ─── Agent profile commands ────────────────────────────────────────────────────

fn cmd_agent_profile_create(
    client: &TackClient,
    name: String,
    instructions: String,
    tool_policy: Option<String>,
    limits: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = execution::build_create_agent_profile_body(
        &name,
        &instructions,
        tool_policy.as_deref(),
        limits.as_deref(),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let resp = client.post("/agent-profiles", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["agent_profile_id"].as_str().unwrap_or("?");
    println!(
        "Created agent profile: {} ({})",
        name,
        &id[..8.min(id.len())]
    );
    println!("  id: {id}");
    Ok(())
}

fn cmd_agent_profile_list(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/agent-profiles")?;
    let list_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&list_val)?);
        return Ok(());
    }
    let empty = vec![];
    let rows = list_val.as_array().unwrap_or(&empty);
    if rows.is_empty() {
        println!("No agent profiles found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME"]);
    for p in rows {
        print_table_row(&[
            short_id(p["agent_profile_id"].as_str()),
            p["name"].as_str().unwrap_or("?"),
        ]);
    }
    Ok(())
}

// ─── Model profile commands ────────────────────────────────────────────────────

fn cmd_model_profile_create(
    client: &TackClient,
    name: String,
    provider: String,
    model_id: String,
    config_reference: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = execution::build_create_model_profile_body(
        &name,
        &provider,
        &model_id,
        config_reference.as_deref(),
    );
    let resp = client.post("/model-profiles", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["model_profile_id"].as_str().unwrap_or("?");
    println!(
        "Created model profile: {} ({})",
        name,
        &id[..8.min(id.len())]
    );
    println!("  provider: {provider}");
    println!("  model:    {model_id}");
    println!("  id:       {id}");
    Ok(())
}

fn cmd_model_profile_list(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/model-profiles")?;
    let list_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&list_val)?);
        return Ok(());
    }
    let empty = vec![];
    let rows = list_val.as_array().unwrap_or(&empty);
    if rows.is_empty() {
        println!("No model profiles found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "PROVIDER", "MODEL"]);
    for m in rows {
        print_table_row(&[
            short_id(m["model_profile_id"].as_str()),
            m["name"].as_str().unwrap_or("?"),
            m["model_provider"].as_str().unwrap_or("?"),
            m["model_id"].as_str().unwrap_or("?"),
        ]);
    }
    Ok(())
}

/// `tack orch dispatch <project>`. `variables` is an optional JSON object
/// string forwarded to docket's pipeline dispatch verbatim; `dispatch_token`
/// is required — the server refuses the request outright without it, so
/// this says so plainly here rather than sending a request that can only
/// come back `403`.
fn cmd_orch_dispatch(
    client: &TackClient,
    project: String,
    variables: Option<String>,
    dispatch_token: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let Some(dispatch_token) = dispatch_token else {
        anyhow::bail!(
            "TACK_ORCH_DISPATCH_TOKEN is not set (and --dispatch-token was not given) — \
             this server refuses to dispatch a docket pipeline without it"
        );
    };
    let variables: serde_json::Value = match variables {
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("--variables must be a JSON object: {e}"))?,
        None => json!({}),
    };
    let body = json!({ "variables": variables });
    let resp = client.post_with_headers(
        &format!("/projects/{project}/orch-dispatch"),
        &body,
        &[("x-tack-dispatch-token", dispatch_token.as_str())],
    )?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let run_id = resp["run_id"].as_str().unwrap_or("?");
    let remote_project = resp["remote_project"].as_str().unwrap_or("?");
    println!("Dispatch started for docket project {remote_project}: run {run_id}");
    println!(
        "  This confirms docket accepted the request and started the run — not that it was \
         permitted. docket's own guardrails run after this response is sent; the outcome \
         (success, failure, or a guardrail block) becomes visible in Tack once the \
         reconciler's next poll mirrors it."
    );
    Ok(())
}

// ─── Formatting helpers ───────────────────────────────────────────────────────

fn print_table_header(cols: &[&str]) {
    print_table_row(cols);
    let divider: Vec<String> = cols.iter().map(|c| "─".repeat(col_width(c))).collect();
    println!("{}", divider.join("  "));
}

fn print_table_row(cols: &[&str]) {
    let widths = [8, 32, 12, 16, 10, 12];
    let parts: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(i, &val)| {
            let w = widths.get(i).copied().unwrap_or(12);
            trunc_pad(val, w)
        })
        .collect();
    println!("{}", parts.join("  "));
}

fn col_width(header: &str) -> usize {
    match header {
        "ID" => 8,
        "NAME" | "TITLE" => 32,
        "TYPE" | "ITEM TYPE" => 12,
        "STATUS" => 16,
        "PRIORITY" => 10,
        "ASSIGNEE" => 12,
        _ => 12,
    }
}

fn trunc_pad(s: &str, width: usize) -> String {
    if s.len() > width {
        format!("{}…", &s[..width - 1])
    } else {
        format!("{:<width$}", s)
    }
}

fn short_id(id: Option<&str>) -> &str {
    id.map(|s| &s[..8.min(s.len())]).unwrap_or("?")
}

fn short_date(dt: Option<&str>) -> &str {
    dt.map(|s| &s[..10.min(s.len())]).unwrap_or("-")
}

fn cmd_backup(client: &TackClient, path: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    let bytes = client.get_bytes("/backup")?;
    let out = path.unwrap_or_else(|| std::path::PathBuf::from("tack-backup.db"));
    std::fs::write(&out, &bytes)
        .map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", out.display()))?;
    println!("Backup saved: {} ({} bytes)", out.display(), bytes.len());
    Ok(())
}

fn cmd_restore(client: &TackClient, path: std::path::PathBuf, as_json: bool) -> anyhow::Result<()> {
    let data =
        std::fs::read(&path).map_err(|e| anyhow::anyhow!("Cannot read {}: {e}", path.display()))?;
    let resp = client.post_bytes("/restore", data)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let msg = resp["message"].as_str().unwrap_or("Restore staged.");
    println!("{msg}");
    Ok(())
}

fn cmd_backup_remote(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.post("/backup/remote", &json!({}))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let key = resp["object_key"].as_str().unwrap_or("?");
    let bytes = resp["bundle_size_bytes"].as_u64().unwrap_or(0);
    let created = resp["created_at"].as_str().unwrap_or("-");
    println!("Remote backup uploaded:");
    println!("  Key:     {key}");
    println!("  Size:    {} bytes", bytes);
    println!("  Created: {created}");
    Ok(())
}

fn cmd_list_remote_backups(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/backup/remote")?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let empty = vec![];
    let backups = resp.as_array().unwrap_or(&empty);
    if backups.is_empty() {
        println!("No remote backups found.");
        return Ok(());
    }
    println!(
        "{:<32} {:>12} {:>8} Key",
        "Created", "Size (bytes)", "Items"
    );
    println!("{}", "-".repeat(90));
    for b in backups {
        let raw_date = b["created_at"].as_str().unwrap_or("-");
        let created = &raw_date[..19.min(raw_date.len())];
        let size = b["bundle_size_bytes"].as_u64().unwrap_or(0);
        let items = b["item_count"].as_u64().unwrap_or(0);
        let key = b["object_key"].as_str().unwrap_or("-");
        println!("{:<32} {:>12} {:>8} {}", created, size, items, key);
    }
    Ok(())
}

fn cmd_restore_remote(
    client: &TackClient,
    key: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = match key {
        Some(k) => json!({ "key": k }),
        None => json!({}),
    };
    let resp = client.post("/backup/remote/restore", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let msg = resp["message"].as_str().unwrap_or("Restore staged.");
    println!("{msg}");
    Ok(())
}

/// Minimal percent-encoding for query parameter values (spaces → %20, etc.)
fn urlenc(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
            ' ' => vec!['%', '2', '0'],
            _ => format!("%{:02X}", c as u32).chars().collect(),
        })
        .collect()
}
