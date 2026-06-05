//! Command-line interface for the OpenHuman core binary.
//!
//! This module handles argument parsing, subcommand dispatching, and help printing
//! for the CLI. It supports commands for running the server, making RPC calls,
//! and invoking domain-specific functionality across various namespaces.

use anyhow::Result;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::core::all;
use crate::core::autocomplete_cli_adapter;
use crate::core::jsonrpc::{default_state, invoke_method, parse_json_params};
use crate::core::logging::CliLogDefault;
use crate::core::{ControllerSchema, TypeSchema};

/// The ASCII banner displayed when the CLI starts.
const CLI_BANNER: &str = r#"

 ▗▄▖ ▄▄▄▄  ▗▞▀▚▖▄▄▄▄  ▗▖ ▗▖█  ▐▌▄▄▄▄  ▗▞▀▜▌▄▄▄▄
▐▌ ▐▌█   █ ▐▛▀▀▘█   █ ▐▌ ▐▌▀▄▄▞▘█ █ █ ▝▚▄▟▌█   █
▐▌ ▐▌█▄▄▄▀ ▝▚▄▄▖█   █ ▐▛▀▜▌     █   █      █   █
▝▚▄▞▘█                ▐▌ ▐▌
     ▀

Contribute & Star us on GitHub: https://github.com/tinyhumansai/openhuman

"#;

/// Dispatches CLI commands based on arguments.
///
/// This is the entry point for CLI argument handling. It performs the following:
/// 1. Prints the ASCII welcome banner to stderr.
/// 2. Resolves and groups available controller schemas.
/// 3. Checks for global help requests.
/// 4. Matches the first argument to a subcommand or a domain namespace.
///
/// # Arguments
///
/// * `args` - A slice of strings containing the command-line arguments.
///
/// # Errors
///
/// Returns an error if the command fails, parameters are invalid, or if
/// the subcommand/namespace is unknown.
pub fn run_from_cli_args(args: &[String]) -> Result<()> {
    // Print the welcome banner to stderr to keep stdout clean for JSON output.
    if !matches!(args.first().map(String::as_str), Some("mcp" | "mcp-server")) {
        eprint!("{CLI_BANNER}");
    }

    load_dotenv_for_cli()?;

    let grouped = grouped_schemas();
    if args.is_empty() || is_help(&args[0]) {
        print_general_help(&grouped);
        return Ok(());
    }

    // Match on the first argument to determine the subcommand.
    match args[0].as_str() {
        "run" | "serve" => run_server_command(&args[1..]),
        "mcp" | "mcp-server" => crate::openhuman::mcp_server::run_stdio_from_cli(&args[1..]),
        "call" => run_call_command(&args[1..]),
        // Domain-specific CLI adapters that don't follow the generic namespace pattern.
        "screen-intelligence" => {
            crate::openhuman::screen_intelligence::cli::run_screen_intelligence_command(&args[1..])
        }
        "text-input" => crate::openhuman::text_input::cli::run_text_input_command(&args[1..]),
        "tree-summarizer" => {
            crate::openhuman::memory_tree::tree_runtime::cli::run_tree_summarizer_command(
                &args[1..],
            )
        }
        "memory" => crate::core::memory_cli::run_memory_command(&args[1..]),
        "agent" => {
            log::debug!(
                "[cli] dispatching to agent subcommand, args={:?}",
                &args[1..]
            );
            crate::core::agent_cli::run_agent_command(&args[1..])
        }
        "undo" => run_undo_command(&args[1..]),
        "sentry-test" => run_sentry_test_command(&args[1..]),
        // DADOU skill lifecycle subcommand: `openhuman-core skill install <url>`
        "skill" => run_dadou_skill_command(&args[1..]),
        // DADOU dashboard: `openhuman-core dashboard`
        "dashboard" => run_dashboard_command(&args[1..]),
        // Generic namespace dispatcher: `openhuman <namespace> <function> ...`
        namespace => run_namespace_command(namespace, &args[1..], &grouped),
    }
}

/// Handles the `undo` subcommand: `openhuman-core undo [--last | --before <timestamp>]`.
///
/// Restores files to their pre-modification state using the rollback history.
fn run_undo_command(args: &[String]) -> Result<()> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("Usage: openhuman-core undo [--last | --before <timestamp>]");
        println!("  --last              Undo the most recent file change");
        println!("  --before <ts>       Rollback all files modified before ISO 8601 timestamp");
        println!("  --list              List recent rollback history entries");
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new()?;
    match args[0].as_str() {
        "--last" => {
            let store = crate::openhuman::rollback::store::global_store()
                .ok_or_else(|| anyhow::anyhow!("Rollback store not initialized"))?;
            rt.block_on(async {
                match crate::openhuman::rollback::ops::undo_last(store).await {
                    Ok(outcome) => {
                        println!("{}", serde_json::to_string_pretty(&outcome.value)?);
                        Ok(())
                    }
                    Err(err) => {
                        eprintln!("Error: {err}");
                        std::process::exit(1);
                    }
                }
            })
        }
        "--before" => {
            let timestamp = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Missing timestamp argument after --before"))?;
            let store = crate::openhuman::rollback::store::global_store()
                .ok_or_else(|| anyhow::anyhow!("Rollback store not initialized"))?;
            rt.block_on(async {
                match crate::openhuman::rollback::ops::undo_before(store, timestamp).await {
                    Ok(outcome) => {
                        println!("{}", serde_json::to_string_pretty(&outcome.value)?);
                        Ok(())
                    }
                    Err(err) => {
                        eprintln!("Error: {err}");
                        std::process::exit(1);
                    }
                }
            })
        }
        "--list" => {
            let store = crate::openhuman::rollback::store::global_store()
                .ok_or_else(|| anyhow::anyhow!("Rollback store not initialized"))?;
            match store.list_recent(20) {
                Ok(entries) => {
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                    Ok(())
                }
                Err(err) => {
                    eprintln!("Error: {err}");
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("Unknown undo option: {other}. Use --last, --before <timestamp>, or --list");
            std::process::exit(1);
        }
    }
}

/// Handles the `skill` subcommand for DADOU WASM skill lifecycle management.
///
/// This provides a user-friendly CLI interface for installing, updating,
/// auditing, removing, and listing WASM skills.
///
/// # Usage
///
/// ```text
/// openhuman-core skill install <git-url>
/// openhuman-core skill update <name>
/// openhuman-core skill audit <name>
/// openhuman-core skill remove <name>
/// openhuman-core skill list
/// openhuman-core skill trust-author <pubkey_file>
/// ```
/// Handles the `dashboard` subcommand: `openhuman-core dashboard [--port <u16>] [--host <addr>]`.
///
/// Starts only the dashboard HTTP server on a dedicated port.
fn run_dashboard_command(args: &[String]) -> Result<()> {
    let mut port: u16 = 7790;
    let mut host = "127.0.0.1".to_string();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse().map_err(|_| anyhow::anyhow!("invalid port"))?;
                }
            }
            "--host" => {
                i += 1;
                if i < args.len() {
                    host = args[i].clone();
                }
            }
            "--help" | "-h" => {
                println!("Usage: openhuman-core dashboard [--port <u16>] [--host <addr>]");
                println!("  --port   Listen port (default: 7790)");
                println!("  --host   Bind address (default: 127.0.0.1)");
                println!();
                println!("Starts the DADOU observability dashboard.");
                return Ok(());
            }
            _ => {
                eprintln!("Unknown flag: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // Ensure the event bus is alive so SSE can subscribe.
        crate::core::event_bus::init_global(crate::core::event_bus::DEFAULT_CAPACITY);

        let mut config = crate::openhuman::config::Config::load_or_init()
            .await
            .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
        config.dashboard.port = port;
        config.dashboard.host = host;

        // Initialise the dashboard event store.
        crate::openhuman::dashboard::store::init_global(&config)
            .unwrap_or_else(|e| {
                log::warn!("[dashboard] store initialisation failed: {e}");
            });

        // Register the dashboard recorder subscriber.
        if let Some(handle) = crate::core::event_bus::subscribe_global(
            std::sync::Arc::new(crate::openhuman::dashboard::bus::DashboardRecorder),
        ) {
            std::mem::forget(handle);
        }

        let shutdown = tokio_util::sync::CancellationToken::new();
        let shutdown_clone = shutdown.clone();

        // Handle Ctrl+C for graceful shutdown.
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            log::info!("[dashboard] Ctrl+C received — shutting down");
            shutdown_clone.cancel();
        });

        match crate::openhuman::dashboard::server::start_dashboard_server(
            &config,
            shutdown,
        )
        .await
        {
            Ok(addr) => {
                log::info!("[dashboard] server started at http://{addr}");
            }
            Err(e) => {
                eprintln!("Dashboard server error: {e}");
                std::process::exit(1);
            }
        }

        Ok(())
    })
}

fn run_dadou_skill_command(args: &[String]) -> Result<()> {
    if args.is_empty() || is_help(&args[0]) {
        println!("Usage: openhuman-core skill <subcommand> [options]");
        println!();
        println!("Subcommands:");
        println!("  install <git-url>       Install a skill from a Git repository");
        println!("  update <name>           Update an installed skill to latest version");
        println!("  audit <name>            Run static analysis on an installed skill");
        println!("  remove <name>           Uninstall a skill");
        println!("  list                    List installed skills with their state");
        println!("  trust-author <pubkey>   Add a GPG public key fingerprint to trusted authors");
        println!();
        println!("Examples:");
        println!("  openhuman-core skill install https://github.com/author/skill-repo.git");
        println!("  openhuman-core skill audit my-skill");
        println!("  openhuman-core skill list");
        return Ok(());
    }

    match args[0].as_str() {
        "install" => {
            if args.len() < 2 {
                return Err(anyhow::anyhow!("Usage: openhuman-core skill install <git-url>"));
            }

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let url = &args[1];
                tracing::info!("[cli] dadou skill install {url}");
                match crate::openhuman::skills::wasm_install::install_skill(url).await {
                    Ok(outcome) => {
                        println!("Installed skill: {} v{}", outcome.name, outcome.version);
                        println!("  GPG: {}", outcome.gpg_status);
                        println!("  Static analysis: {:?}", outcome.analysis_verdict);
                        println!("  Findings: {}", outcome.findings_count);
                        println!("  Path: {}", outcome.path.display());
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("Error installing skill: {e}");
                        std::process::exit(1);
                    }
                }
            })
        }
        "update" => {
            if args.len() < 2 {
                return Err(anyhow::anyhow!("Usage: openhuman-core skill update <name>"));
            }

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let name = &args[1];
                tracing::info!("[cli] dadou skill update {name}");

                let store = crate::openhuman::skills::store::SkillsStore::load()
                    .map_err(|e| anyhow::anyhow!("failed to load store: {e}"))?;
                let trust_store = crate::openhuman::skills::verify::TrustStore::load()
                    .map_err(|e| anyhow::anyhow!("failed to load trust store: {e}"))?;
                let wasm_engine = std::sync::Arc::new(
                    crate::openhuman::skills::wasm::WasmEngine::new()
                        .map_err(|e| anyhow::anyhow!("failed to create engine: {e}"))?,
                );

                let mut installer =
                    crate::openhuman::skills::wasm_install::GitSkillInstaller::new(
                        store, trust_store, wasm_engine,
                    )
                    .map_err(|e| anyhow::anyhow!("failed to create installer: {e}"))?;

                match installer.update_skill(name).await {
                    Ok(outcome) => {
                        println!("Updated skill: {} v{}", outcome.name, outcome.version);
                        println!("  GPG: {}", outcome.gpg_status);
                        println!("  Static analysis: {:?}", outcome.analysis_verdict);
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("Error updating skill: {e}");
                        std::process::exit(1);
                    }
                }
            })
        }
        "audit" => {
            if args.len() < 2 {
                return Err(anyhow::anyhow!("Usage: openhuman-core skill audit <name>"));
            }
            let name = &args[1];
            tracing::info!("[cli] dadou skill audit {name}");
            match crate::openhuman::skills::wasm_install::audit_skill(name) {
                Ok(outcome) => {
                    println!("Audit result for {}: {:?}", outcome.name, outcome.verdict);
                    for finding in &outcome.findings {
                        println!(
                            "  [{:?}] {}:{} -- {}",
                            finding.severity, finding.file, finding.line, finding.pattern
                        );
                    }
                    if outcome.findings.is_empty() {
                        println!("  No suspicious patterns detected.");
                    }
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Error auditing skill: {e}");
                    std::process::exit(1);
                }
            }
        }
        "remove" => {
            if args.len() < 2 {
                return Err(anyhow::anyhow!("Usage: openhuman-core skill remove <name>"));
            }
            let name = &args[1];
            tracing::info!("[cli] dadou skill remove {name}");
            match crate::openhuman::skills::wasm_install::remove_skill(name) {
                Ok(outcome) => {
                    if outcome.removed {
                        println!("Removed skill: {}", outcome.name);
                    } else {
                        println!("Skill '{}' was not installed.", outcome.name);
                    }
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Error removing skill: {e}");
                    std::process::exit(1);
                }
            }
        }
        "list" => {
            tracing::info!("[cli] dadou skill list");
            let store = crate::openhuman::skills::store::SkillsStore::load()
                .map_err(|e| anyhow::anyhow!("failed to load store: {e}"))?;
            let skills = store.list();
            if skills.is_empty() {
                println!("No skills installed.");
            } else {
                println!("Installed skills:");
                for skill in skills {
                    let status = if skill.enabled { "enabled" } else { "disabled" };
                    print!("  {} v{} [{}]", skill.name, skill.version, status);
                    if let Some(gpg) = &skill.gpg_fingerprint {
                        print!(" GPG:{}", &gpg[..gpg.len().min(16)]);
                    }
                    if let Some(audit) = &skill.audit_result {
                        print!(" audit:{}", audit);
                    }
                    println!();
                }
            }
            Ok(())
        }
        "trust-author" => {
            if args.len() < 2 {
                return Err(anyhow::anyhow!(
                    "Usage: openhuman-core skill trust-author <pubkey_fingerprint>"
                ));
            }
            let pubkey_pem = &args[1];
            tracing::info!("[cli] dadou skill trust-author");
            let trust_store = crate::openhuman::skills::verify::TrustStore::load()
                .map_err(|e| anyhow::anyhow!("failed to load trust store: {e}"))?;
            match trust_store.add_author(pubkey_pem) {
                Ok(author) => {
                    println!(
                        "Added trusted author: {} (key_id: {}, fingerprint: {})",
                        author.name, author.key_id, author.fingerprint
                    );
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Error adding trusted author: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!(
                "Unknown skill subcommand: {}. Run 'openhuman-core skill --help' for usage.",
                args[0]
            );
            std::process::exit(1);
        }
    }
}

/// Handles the `sentry-test` subcommand used to verify Sentry wiring end-to-end.
///
/// Captures an Error-level event against the currently initialized Sentry
/// client (see `sentry::init` in the binary entry point), flushes the client,
/// and prints the event UUID to stdout. Optional `--panic` flag additionally
/// triggers a panic so the panic integration is exercised too.
///
/// Requires a DSN resolvable at runtime — either via the
/// `OPENHUMAN_CORE_SENTRY_DSN` env var (or the legacy `OPENHUMAN_SENTRY_DSN`
/// alias) or baked into the binary at build time via `option_env!`. Absent a
/// DSN, the command exits non-zero with a diagnostic instead of silently
/// producing no telemetry.
fn run_sentry_test_command(args: &[String]) -> Result<()> {
    let mut message: Option<String> = None;
    let mut do_panic = false;
    let mut i = 0usize;

    while i < args.len() {
        match args[i].as_str() {
            "--message" => {
                message = Some(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("missing value for --message"))?
                        .clone(),
                );
                i += 2;
            }
            "--panic" => {
                do_panic = true;
                i += 1;
            }
            "-h" | "--help" => {
                println!("Usage: openhuman sentry-test [--message <text>] [--panic]");
                println!();
                println!("  --message <text>  Body of the Error-level event sent to Sentry");
                println!("                    (default: \"openhuman sentry-test ping\")");
                println!("  --panic           After capturing the event, trigger a panic so the");
                println!("                    panic integration reports it as a separate event.");
                println!();
                println!(
                    "Requires OPENHUMAN_CORE_SENTRY_DSN (or the legacy OPENHUMAN_SENTRY_DSN alias)"
                );
                println!("at runtime, or baked into the binary at build time via option_env!. On");
                println!("success, prints the event UUID to stdout.");
                return Ok(());
            }
            other => return Err(anyhow::anyhow!("unknown sentry-test arg: {other}")),
        }
    }

    let client = sentry::Hub::current().client();
    let dsn_host = client
        .as_deref()
        .and_then(|c| c.dsn())
        .map(|d| d.host().to_string());

    match &dsn_host {
        Some(host) => eprintln!("[sentry-test] Sentry client active (dsn host: {host})"),
        None => {
            return Err(anyhow::anyhow!(
                "Sentry is not initialized in this binary — no DSN is resolvable. \
                 Set OPENHUMAN_CORE_SENTRY_DSN (or the legacy OPENHUMAN_SENTRY_DSN alias) \
                 in the environment (or rebuild with it defined at compile time) and try again."
            ));
        }
    }

    let msg = message.unwrap_or_else(|| "openhuman sentry-test ping".to_string());

    sentry::configure_scope(|scope| {
        scope.set_tag("test", "true");
        scope.set_tag("source", "sentry-test-cli");
    });

    let event_id = sentry::capture_message(&msg, sentry::Level::Error);

    if let Some(c) = client {
        if !c.flush(Some(std::time::Duration::from_secs(5))) {
            eprintln!(
                "[sentry-test] WARNING: flush timed out after 5s — event may not have reached Sentry."
            );
        }
    }

    println!("{event_id}");

    if do_panic {
        eprintln!(
            "[sentry-test] Triggering panic as requested — the panic integration should capture it."
        );
        panic!("openhuman sentry-test intentional panic");
    }

    Ok(())
}

/// Loads key/value pairs from a `.env` file into the process environment.
///
/// This is used for all CLI entrypoints so direct namespace commands pick up
/// the same repo-local configuration as `run` / `serve`.
///
/// Precedence:
/// 1. Variables already set in the process environment are **not** overwritten.
/// 2. If `OPENHUMAN_DOTENV_PATH` is set, that file is loaded.
/// 3. Otherwise, it searches for `.env` in the current working directory.
fn load_dotenv_for_cli() -> Result<()> {
    match std::env::var("OPENHUMAN_DOTENV_PATH") {
        Ok(path) if !path.trim().is_empty() => {
            dotenvy::from_path(&path).map_err(|e| {
                anyhow::anyhow!("failed to load dotenv from OPENHUMAN_DOTENV_PATH={path}: {e}")
            })?;
        }
        _ => {
            let _ = dotenvy::dotenv();
        }
    }
    Ok(())
}

/// Handles the `run` subcommand to start the core HTTP/JSON-RPC server.
///
/// This command boots the main application server, including its JSON-RPC
/// endpoint, Socket.IO bridge, and background services (voice, vision, etc.).
///
/// # Arguments
///
/// * `args` - Command-line arguments for the `run` command (e.g., `--port`).
fn run_server_command(args: &[String]) -> Result<()> {
    let mut port: Option<u16> = None;
    let mut host: Option<String> = None;
    let mut socketio_enabled = true;
    let mut verbose = false;
    let mut log_scope = CliLogDefault::Global;
    let mut i = 0usize;

    // Manual argument parsing loop for specific flags.
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                let raw = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --port"))?;
                port = Some(
                    raw.parse::<u16>()
                        .map_err(|e| anyhow::anyhow!("invalid --port: {e}"))?,
                );
                i += 2;
            }
            "--host" => {
                host = Some(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("missing value for --host"))?
                        .clone(),
                );
                i += 2;
            }
            "--jsonrpc-only" => {
                socketio_enabled = false;
                i += 1;
            }
            "-v" | "--verbose" => {
                verbose = true;
                i += 1;
            }
            other if autocomplete_cli_adapter::parse_run_scope_flag(other).is_some() => {
                log_scope = autocomplete_cli_adapter::parse_run_scope_flag(other)
                    .unwrap_or(CliLogDefault::Global);
                i += 1;
            }
            "-h" | "--help" => {
                println!("Usage: openhuman run [--host <addr>] [--port <u16>] [--jsonrpc-only] [--autocomplete-logs] [-v|--verbose]");
                println!();
                println!(
                    "  --host <addr>    Bind address (default: 127.0.0.1 or OPENHUMAN_CORE_HOST)"
                );
                println!(
                    "  --port <u16>     Listen address port (default: 7788 or OPENHUMAN_CORE_PORT)"
                );
                println!("  --jsonrpc-only   HTTP JSON-RPC only; disable Socket.IO");
                autocomplete_cli_adapter::print_run_scope_help_line();
                println!("  -v, --verbose    Shorthand for RUST_LOG=debug when RUST_LOG is unset");
                println!();
                println!("Logging: set RUST_LOG (e.g. RUST_LOG=debug openhuman run). Default level is info.");
                return Ok(());
            }
            other => return Err(anyhow::anyhow!("unknown run arg: {other}")),
        }
    }

    crate::core::logging::init_for_cli_run(verbose, log_scope);

    // Initialize the Tokio multi-threaded runtime.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        crate::core::jsonrpc::run_server(host.as_deref(), port, socketio_enabled).await
    })?;
    Ok(())
}

/// Handles the `call` subcommand to invoke a JSON-RPC method directly from the CLI.
///
/// This is used for one-off commands and debugging, bypassing the HTTP transport
/// and calling the internal `invoke_method` directly.
///
/// # Arguments
///
/// * `args` - Command-line arguments specifying the method and parameters.
fn run_call_command(args: &[String]) -> Result<()> {
    let mut method: Option<String> = None;
    let mut params = "{}".to_string();

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--method" => {
                method = Some(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("missing value for --method"))?
                        .clone(),
                );
                i += 2;
            }
            "--params" => {
                params = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --params"))?
                    .clone();
                i += 2;
            }
            "-h" | "--help" => {
                println!("Usage: openhuman call --method <name> [--params '<json>']");
                return Ok(());
            }
            other => return Err(anyhow::anyhow!("unknown call arg: {other}")),
        }
    }

    let method = method.ok_or_else(|| anyhow::anyhow!("--method is required"))?;
    let params = parse_json_params(&params).map_err(anyhow::Error::msg)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let value = rt
        .block_on(async { invoke_method(default_state(), &method, params).await })
        .map_err(anyhow::Error::msg)?;

    // Output the result as pretty-printed JSON to stdout.
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Dispatches commands that fall under a specific namespace (e.g., `openhuman <namespace> <function>`).
///
/// It looks up the function schema for validation and executes the request.
///
/// # Arguments
///
/// * `namespace` - The namespace for the command.
/// * `args` - Arguments for the function within the namespace.
/// * `grouped` - A map of available schemas grouped by namespace.
fn run_namespace_command(
    namespace: &str,
    args: &[String],
    grouped: &BTreeMap<String, Vec<ControllerSchema>>,
) -> Result<()> {
    let Some(schemas) = grouped.get(namespace) else {
        return Err(anyhow::anyhow!(
            "unknown namespace '{namespace}'. Run `openhuman --help` to see available namespaces."
        ));
    };

    let preparsed = autocomplete_cli_adapter::preparse_namespace(namespace, args);
    let args: &[String] = &preparsed.args;
    if let Some((verbose, scope)) = preparsed.init_logging {
        crate::core::logging::init_for_cli_run(verbose, scope);
    }

    if args.is_empty() || is_help(&args[0]) {
        // If there's a domain-specific CLI handler for this namespace, use it as the default.
        if let Some(cli_handler) = all::cli_handler_for_namespace(namespace) {
            return cli_handler(args);
        }
        print_namespace_help(namespace, schemas);
        return Ok(());
    }

    let function = args[0].as_str();
    let Some(schema) = schemas.iter().find(|s| s.function == function).cloned() else {
        return Err(anyhow::anyhow!(
            "unknown function '{namespace} {function}'. Run `openhuman {namespace} --help`."
        ));
    };

    // Domain adapters can intercept specific namespace/function combinations.
    if args.len() > 1
        && is_help(&args[1])
        && autocomplete_cli_adapter::maybe_print_start_help(namespace, function)
    {
        return Ok(());
    }
    if let Some(value) =
        autocomplete_cli_adapter::maybe_handle_namespace_start(namespace, function, &args[1..])?
    {
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }

    if args.len() > 1 && is_help(&args[1]) {
        print_function_help(namespace, &schema);
        return Ok(());
    }

    // Generic parameter parsing and validation based on schema.
    let params = parse_function_params(&schema, &args[1..]).map_err(anyhow::Error::msg)?;
    let method = all::rpc_method_from_parts(namespace, function)
        .ok_or_else(|| anyhow::anyhow!("unregistered controller '{namespace}.{function}'"))?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let value = rt
        .block_on(async { invoke_method(default_state(), &method, Value::Object(params)).await })
        .map_err(anyhow::Error::msg)?;

    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Parses command-line arguments into a JSON map based on a function's schema.
///
/// # Arguments
///
/// * `schema` - The schema defining expected inputs.
/// * `args` - The command-line arguments to parse.
///
/// # Errors
///
/// Returns an error if arguments are malformed, unknown, or fail validation.
fn parse_function_params(
    schema: &ControllerSchema,
    args: &[String],
) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    let mut i = 0usize;

    while i < args.len() {
        let raw = &args[i];
        if !raw.starts_with("--") {
            return Err(format!("invalid arg '{raw}', expected --<param> <value>"));
        }
        let key = raw.trim_start_matches("--").replace('-', "_");
        let Some(spec) = schema.inputs.iter().find(|input| input.name == key) else {
            return Err(format!(
                "unknown param '{key}' for {}.{}",
                schema.namespace, schema.function
            ));
        };
        let raw_value = args
            .get(i + 1)
            .ok_or_else(|| format!("missing value for --{key}"))?;
        if raw_value.starts_with("--") {
            let next_key = raw_value.trim_start_matches("--").replace('-', "_");
            if schema.inputs.iter().any(|input| input.name == next_key) {
                return Err(format!("missing value for --{key}"));
            }
        }
        let value = parse_input_value(&spec.ty, raw_value)?;
        out.insert(key, value);
        i += 2;
    }

    all::validate_params(schema, &out)?;
    Ok(out)
}

/// Parses a raw string value into a JSON `Value` based on the target `TypeSchema`.
///
/// Supports basic types like string, bool, and numbers, as well as complex JSON
/// structures for advanced types.
///
/// # Arguments
///
/// * `ty` - The expected type schema.
/// * `raw` - The raw string value from the command line.
fn parse_input_value(ty: &TypeSchema, raw: &str) -> Result<Value, String> {
    match ty {
        TypeSchema::String => Ok(Value::String(raw.to_string())),
        TypeSchema::Bool => raw
            .parse::<bool>()
            .map(Value::Bool)
            .map_err(|e| format!("expected bool, got '{raw}': {e}")),
        TypeSchema::I64 => raw
            .parse::<i64>()
            .map(|n| Value::Number(n.into()))
            .map_err(|e| format!("expected i64, got '{raw}': {e}")),
        TypeSchema::U64 => raw
            .parse::<u64>()
            .map(|n| Value::Number(n.into()))
            .map_err(|e| format!("expected u64, got '{raw}': {e}")),
        TypeSchema::F64 => {
            let n = raw
                .parse::<f64>()
                .map_err(|e| format!("expected f64, got '{raw}': {e}"))?;
            serde_json::Number::from_f64(n)
                .map(Value::Number)
                .ok_or_else(|| format!("invalid f64 '{raw}'"))
        }
        TypeSchema::Option(inner) => parse_input_value(inner, raw),
        TypeSchema::Enum { .. } => Ok(Value::String(raw.to_string())),
        TypeSchema::Json
        | TypeSchema::Array(_)
        | TypeSchema::Map(_)
        | TypeSchema::Object { .. }
        | TypeSchema::Ref(_)
        | TypeSchema::Bytes => parse_json_params(raw),
    }
}

/// Aggregates all registered controller schemas and groups them by namespace.
fn grouped_schemas() -> BTreeMap<String, Vec<ControllerSchema>> {
    let mut grouped: BTreeMap<String, Vec<ControllerSchema>> = BTreeMap::new();
    for schema in all::all_controller_schemas() {
        grouped
            .entry(schema.namespace.to_string())
            .or_default()
            .push(schema);
    }
    // Sort functions within each namespace for consistent help output.
    for schemas in grouped.values_mut() {
        schemas.sort_by_key(|s| s.function);
    }
    grouped
}

/// Prints the general help message listing available commands and namespaces.
fn print_general_help(grouped: &BTreeMap<String, Vec<ControllerSchema>>) {
    println!("OpenHuman core CLI\n");
    println!("Usage:");
    println!("  openhuman run [--host <addr>] [--port <u16>] [--jsonrpc-only] [--verbose]");
    println!("  openhuman call --method <name> [--params '<json>']");
    println!(
        "  openhuman mcp [-v|--verbose]              (stdio MCP server; read-only memory tools)"
    );
    println!("  openhuman skills <subcommand> [options]   (skill development runtime)");
    println!("  openhuman agent <subcommand> [options]    (inspect agent definitions & prompts)");
    println!("  openhuman voice [--hotkey <combo>] [--mode <tap|push>]  (voice dictation server)");
    println!("  openhuman tree-summarizer <subcommand> [options]  (summary tree CLI)");
    println!("  openhuman undo [--last|--before <ts>|--list]      (rollback file changes)");
    println!("  openhuman skill <subcommand> [options]             (DADOU WASM skill lifecycle)");
    println!("  openhuman sentry-test [--message <text>] [--panic]  (verify Sentry wiring)");
    println!("  openhuman <namespace> <function> [--param value ...]\n");
    println!("Available namespaces:");
    for namespace in grouped.keys() {
        let description = all::namespace_description(namespace.as_str())
            .unwrap_or("No namespace description available.");
        println!("  {namespace} - {description}");
    }
    println!("\nUse `openhuman <namespace> --help` to see functions.");
}

/// Prints help for a specific namespace, listing its functions.
fn print_namespace_help(namespace: &str, schemas: &[ControllerSchema]) {
    println!("Namespace: {namespace}\n");
    if let Some(description) = all::namespace_description(namespace) {
        println!("{description}\n");
    }
    println!("Functions:");
    for schema in schemas {
        println!("  {} - {}", schema.function, schema.description);
    }
    println!("\nUse `openhuman {namespace} <function> --help` for parameters.");
    autocomplete_cli_adapter::maybe_print_namespace_help_footer(namespace);
}

/// Prints detailed help for a specific function, including its parameters and description.
fn print_function_help(namespace: &str, schema: &ControllerSchema) {
    println!("{} {}\n", namespace, schema.function);
    println!("{}", schema.description);
    println!("\nParameters:");
    if schema.inputs.is_empty() {
        println!("  none");
    } else {
        for input in &schema.inputs {
            let required = if input.required {
                "required"
            } else {
                "optional"
            };
            println!("  --{} ({}) - {}", input.name, required, input.comment);
        }
    }
}

/// Checks if a string represents a help flag.
fn is_help(value: &str) -> bool {
    matches!(value, "-h" | "--help" | "help")
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
