// ============================================================================
// GROOT GAME ENGINE CLI
//
// Scaffolds, runs, and builds Groot projects.
//
//   groot-cli new <name>   Scaffold a new Groot project folder
//   groot-cli run          Run the current Groot game (cargo run)
//   groot-cli build        Build the release bundle (cargo build --release)
//   groot-cli info         Print engine version / paths
// ============================================================================

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("==================================================");
    println!("            GROOT GAME ENGINE CLI                 ");
    println!("==================================================");
    println!("Usage: groot-cli <command> [args]");
    println!();
    println!("Commands:");
    println!("  new <name>   - Scaffold a new Groot project");
    println!("  run [path]   - Run the Groot game (cargo run)");
    println!("  build        - Build release binary (cargo build --release)");
    println!("  info         - Print engine version and workspace info");
    println!("  help         - Show this help");
    println!("==================================================");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_help();
        return;
    }

    match args[1].as_str() {
        "new" => cmd_new(&args[2..]),
        "run" => cmd_run(args.get(2).map(|s| s.as_str())),
        "build" => cmd_build(),
        "info" => cmd_info(),
        "help" | "--help" | "-h" => print_help(),
        "version" | "--version" | "-V" => {
            println!("groot-cli {VERSION}");
        }
        other => {
            eprintln!("Error: unknown command '{other}'");
            print_help();
            std::process::exit(2);
        }
    }
}

// ---------------------------------------------------------------------------
// groot-cli new
// ---------------------------------------------------------------------------

fn cmd_new(args: &[String]) {
    let Some(name) = args.first() else {
        eprintln!("Error: missing project name. Usage: groot-cli new <name>");
        std::process::exit(1);
    };
    if !is_valid_project_name(name) {
        eprintln!(
            "Error: '{name}' is not a valid project name (alphanumeric, '-', '_', '.' only)."
        );
        std::process::exit(1);
    }
    let project_path = Path::new(name);
    if project_path.exists() {
        eprintln!("Error: '{name}' already exists.");
        std::process::exit(1);
    }

    println!("Initializing Groot project: '{name}' ...");

    fs::create_dir_all(project_path.join("assets/scripts")).unwrap_or_else(|e| {
        eprintln!("Error creating assets/scripts: {e}");
        std::process::exit(1);
    });
    fs::create_dir_all(project_path.join("assets/sprites")).unwrap_or_else(|e| {
        eprintln!("Error creating assets/sprites: {e}");
        std::process::exit(1);
    });

    write_or_die(
        project_path.join("groot.toml"),
        format!(
            r#"[project]
name = "{name}"
version = "0.1.0"

[window]
title = "{name}"
width = 800
height = 600
"#
        ),
    );

    write_or_die(
        project_path.join("assets/scripts/player.gs"),
        r#"type Player struct { Speed float64 }
var self = Player{Speed: 300.0}

func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var move = groot.GetAxis("Horizontal")
    groot.SetSelfPosition(pos[0] + move*self.Speed*dt, pos[1])
    groot.Log("Hello from Groot GoScript!")
}
"#
        .to_string(),
    );

    write_or_die(
        project_path.join("README.md"),
        format!(
            "# {name}\n\nA Groot game project. Scripts live in `assets/scripts/`, sprites in `assets/sprites/`.\n\n- `groot-cli run` to run\n- `groot-cli build` to build a release bundle\n"
        ),
    );

    println!();
    println!("Project '{name}' created successfully!");
    println!();
    println!("  {}/groot.toml", project_path.display());
    println!("  {}/assets/scripts/player.gs", project_path.display());
    println!();
    println!("Next: run `groot-cli run` from the engine workspace to play.");
}

// ---------------------------------------------------------------------------
// groot-cli run / build / info
// ---------------------------------------------------------------------------

fn cmd_run(dir: Option<&str>) {
    let workdir = resolve_workdir(dir);
    ensure_cargo_project(&workdir);
    println!("Running Groot game in '{}' ...", workdir.display());
    let status = Command::new("cargo")
        .current_dir(&workdir)
        .arg("run")
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to launch cargo: {e}");
            std::process::exit(1);
        });
    std::process::exit(status.code().unwrap_or(0));
}

fn cmd_build() {
    let workdir = resolve_workdir(None);
    ensure_cargo_project(&workdir);
    println!("Building Groot release bundle in '{}' ...", workdir.display());
    let status = Command::new("cargo")
        .current_dir(&workdir)
        .arg("build")
        .arg("--release")
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to launch cargo: {e}");
            std::process::exit(1);
        });
    std::process::exit(status.code().unwrap_or(0));
}

fn cmd_info() {
    println!("==================================================");
    println!("  GROOT ENGINE INFO");
    println!("==================================================");
    println!("  groot-cli version : {VERSION}");
    println!("  cargo package     : groot");
    println!("  cwd               : {}", std::env::current_dir().unwrap_or_default().display());
    println!("  scripts dir       : assets/scripts");
    println!("  project config    : groot.toml");
    println!("==================================================");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_workdir(dir: Option<&str>) -> PathBuf {
    match dir {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir().unwrap_or_else(|e| {
            eprintln!("Error: cannot resolve current directory: {e}");
            std::process::exit(1);
        }),
    }
}

fn ensure_cargo_project(workdir: &Path) {
    if !workdir.join("Cargo.toml").exists() {
        eprintln!(
            "Error: no Cargo.toml in '{}'. Run this from a Groot/Cargo project.",
            workdir.display()
        );
        std::process::exit(1);
    }
}

fn is_valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn write_or_die(path: PathBuf, contents: String) {
    fs::write(&path, contents).unwrap_or_else(|e| {
        eprintln!("Error writing {}: {e}", path.display());
        std::process::exit(1);
    });
}
