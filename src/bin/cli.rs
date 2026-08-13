use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
struct Registry {
    version: String,
    plugins: Vec<PluginEntry>,
}

#[derive(Deserialize)]
struct PluginEntry {
    name: String,
    description: String,
    author: String,
    #[serde(rename = "path")]
    _path: String,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("==================================================");
    println!("            GROOT GAME ENGINE CLI                 ");
    println!("==================================================");
    println!("Usage: groot <command> [args]");
    println!();
    println!("Commands:");
    println!("  new <name>            - Scaffold a new Groot project");
    println!("  run [--target <t>]    - Run Groot game (targets: desktop, android)");
    println!("  build [--target <t>]  - Build release bundle (targets: desktop, android)");
    println!("  plugin                - Manage plugins (list, add, remove)");
    println!("  info                  - Print engine version and workspace info");
    println!("  help                  - Show this help");
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
        "run" => cmd_run(&args[2..]),
        "build" => cmd_build(&args[2..]),
        "plugin" => cmd_plugin(&args[2..]),
        "info" => cmd_info(),
        "help" | "--help" | "-h" => print_help(),
        "version" | "--version" | "-V" => {
            println!("groot {VERSION}");
        }
        other => {
            eprintln!("Error: unknown command '{other}'");
            print_help();
            std::process::exit(2);
        }
    }
}

/// Extract `--target <desktop|android>` from the arg list (defaults to desktop).
fn parse_target(args: &[String]) -> &str {
    for i in 0..args.len() {
        if args[i] == "--target" && i + 1 < args.len() {
            return args[i + 1].as_str();
        }
    }
    "desktop"
}

fn cmd_new(args: &[String]) {
    let Some(name) = args.first() else {
        eprintln!("Error: missing project name. Usage: groot new <name>");
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

    fs::create_dir_all(project_path.join("src")).unwrap_or_else(|e| {
        eprintln!("Error creating src: {e}");
        std::process::exit(1);
    });
    fs::create_dir_all(project_path.join("assets/prefabs")).unwrap_or_else(|e| {
        eprintln!("Error creating assets/prefabs: {e}");
        std::process::exit(1);
    });
    fs::create_dir_all(project_path.join("assets/scenes")).unwrap_or_else(|e| {
        eprintln!("Error creating assets/scenes: {e}");
        std::process::exit(1);
    });
    fs::create_dir_all(project_path.join("assets/scripts")).unwrap_or_else(|e| {
        eprintln!("Error creating assets/scripts: {e}");
        std::process::exit(1);
    });

    write_or_die(
        project_path.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
groot = {{ git = "https://github.com/JohnEsleyer/groot" }}

[profile.dev]
opt-level = 1
"#
        ),
    );

    write_or_die(
        project_path.join("src/main.rs"),
        r#"fn main() {
    groot::run_game("assets/config.ron");
}
"#
        .to_string(),
    );

    write_or_die(
        project_path.join("assets/config.ron"),
        format!(
            r#"(
    project: (
        name: "{name}",
        version: "0.1.0",
    ),
    window: (
        title: "{name}",
        width: 1280.0,
        height: 720.0,
    ),
    render: (
        clear_color: Rgba(0.1, 0.1, 0.15, 1.0),
    ),
    initial_scene: "assets/scenes/main.scene.ron",
)
"#
        ),
    );

    write_or_die(
        project_path.join("assets/prefabs/player.prefab.ron"),
        r#"(
    name: "player",
    script: Some("assets/scripts/player.gos"),
    transform: (
        position: (0.0, 1.0, 0.0),
        rotation: (0.0, 0.0, 0.0),
        scale: (1.0, 1.0, 1.0),
    ),
    visual: Some(MeshPbr(
        shape: Cuboid(x: 1.0, y: 1.0, z: 1.0),
        material: (
            color: Rgba(0.2, 0.8, 0.3, 1.0),
            roughness: 0.5,
            metallic: 0.0,
        ),
    )),
    collider: Some(Box3D(x: 1.0, y: 1.0, z: 1.0)),
    children: [],
)
"#
        .to_string(),
    );

    write_or_die(
        project_path.join("assets/scenes/main.scene.ron"),
        r#"(
    name: "Main Scene",
    environment: (
        ambient_light: Some((
            color: Rgba(1.0, 1.0, 1.0, 1.0),
            brightness: 0.3,
        )),
        camera: Some(Perspective3D(
            fov: 60.0,
            position: (0.0, 5.0, 10.0),
            look_at: (0.0, 0.0, 0.0),
        )),
    ),
    entities: [
        (
            prefab: "assets/prefabs/player.prefab.ron",
            entity_id: Some(1),
            tag: "Player",
            transform_override: None,
        ),
    ],
)
"#
        .to_string(),
    );

    write_or_die(
        project_path.join("assets/scripts/player.gos"),
        r#"type Player struct { Speed float64 }
var self = Player{Speed: 5.0}

func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var moveX = groot.GetAxis("Horizontal")
    var moveZ = -groot.GetAxis("Vertical")
    groot.SetSelfPosition(pos[0] + moveX*self.Speed*dt, pos[1], pos[2] + moveZ*self.Speed*dt)

    groot.SetSelfCollider(1.0, 1.0, 1.0)
    groot.Log("Hello from Groot 3D GoScript!")
}
"#
        .to_string(),
    );

    write_or_die(
        project_path.join(".gitignore"),
        r#"# Build artifacts
/target

# OS files
.DS_Store
Thumbs.db

# Editor files
*.swp
*.swo
*~
.idea/
.vscode/

# Environment files
.env
"#
        .to_string(),
    );

    write_or_die(
        project_path.join("README.md"),
        format!(
            "# {name}\n\nA Groot game project. RON prefabs live in `assets/prefabs/`, scenes in `assets/scenes/`, and GoScript in `assets/scripts/`.\n\n- `groot run` to run\n- `groot build` to build a release bundle\n"
        ),
    );

    println!();
    println!("Project '{name}' created successfully!");
    println!();
    println!("  {}/Cargo.toml", project_path.display());
    println!("  {}/src/main.rs", project_path.display());
    println!("  {}/assets/config.ron", project_path.display());
    println!("  {}/assets/prefabs/player.prefab.ron", project_path.display());
    println!("  {}/assets/scenes/main.scene.ron", project_path.display());
    println!("  {}/assets/scripts/player.gos", project_path.display());
    println!("  {}/.gitignore", project_path.display());
    println!();
    println!("Next: cd {name} && groot run");
}

fn cmd_run(args: &[String]) {
    let workdir = resolve_workdir(None);
    ensure_cargo_project(&workdir);

    let target = parse_target(args);
    match target {
        "android" => {
            println!("Deploying and running Groot APK on Android device...");
            let status = Command::new("cargo")
                .current_dir(&workdir)
                .args(["apk", "run", "--target", "aarch64-linux-android"])
                .status()
                .unwrap_or_else(|e| {
                    eprintln!(
                        "Error: failed to launch cargo-apk: {e}. Install via 'cargo install cargo-apk'"
                    );
                    std::process::exit(1);
                });
            std::process::exit(status.code().unwrap_or(0));
        }
        "desktop" => {
            println!("Running Desktop Groot game in '{}' ...", workdir.display());
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
        other => {
            eprintln!("Error: unknown target '{other}'. Use desktop or android.");
            std::process::exit(2);
        }
    }
}

fn cmd_build(args: &[String]) {
    let workdir = resolve_workdir(None);
    ensure_cargo_project(&workdir);

    let target = parse_target(args);
    match target {
        "android" => {
            println!("Building Android ARM64 APK bundle...");
            let status = Command::new("cargo")
                .current_dir(&workdir)
                .args(["apk", "build", "--target", "aarch64-linux-android", "--release"])
                .status()
                .unwrap_or_else(|e| {
                    eprintln!("Error: failed to launch cargo-apk: {e}");
                    std::process::exit(1);
                });
            std::process::exit(status.code().unwrap_or(0));
        }
        "desktop" => {
            println!("Building Desktop release binary in '{}' ...", workdir.display());
            let status = Command::new("cargo")
                .current_dir(&workdir)
                .args(["build", "--release"])
                .status()
                .unwrap_or_else(|e| {
                    eprintln!("Error: failed to launch cargo: {e}");
                    std::process::exit(1);
                });
            std::process::exit(status.code().unwrap_or(0));
        }
        other => {
            eprintln!("Error: unknown target '{other}'. Use desktop or android.");
            std::process::exit(2);
        }
    }
}

fn cmd_plugin(args: &[String]) {
    let Some(subcmd) = args.first() else {
        println!("Usage: groot plugin <list|add|remove> [name]");
        return;
    };

    match subcmd.as_str() {
        "list" => {
            let registry_path = std::path::Path::new("../groot-plugins/index.ron");
            if let Ok(content) = fs::read_to_string(registry_path) {
                match ron::from_str::<Registry>(&content) {
                    Ok(registry) => {
                        println!("Groot Plugins (registry v{})", registry.version);
                        println!();
                        for p in &registry.plugins {
                            println!("  {} — {}", p.name, p.description);
                            println!("    by {} | groot plugin add {}", p.author, p.name);
                            println!();
                        }
                    }
                    Err(e) => {
                        eprintln!("Error parsing plugin registry: {e}");
                    }
                }
            } else {
                eprintln!("No plugin registry found at ../groot-plugins/index.ron");
                eprintln!("Available plugins:");
                eprintln!("  audio   — Simple audio synthesizer & sound effects");
                eprintln!("  gizmos  — 2D/3D debug shape line drawer");
            }
        }
        "add" => {
            let Some(name) = args.get(1) else {
                eprintln!("Error: missing plugin name. Usage: groot plugin add <name>");
                return;
            };
            println!("Installing plugin '{name}' into Cargo.toml...");
            let dep_line = format!("groot-plugin-{name} = {{ path = \"../groot-plugin-{name}\" }}");
            let mut cargo_toml = fs::read_to_string("Cargo.toml").unwrap_or_default();
            if cargo_toml.contains(&format!("groot-plugin-{name}")) {
                println!("Plugin 'groot-plugin-{name}' is already installed.");
                return;
            }
            // Insert dependency after [dependencies] section header, before [profile.dev]
            if let Some(pos) = cargo_toml.find("[profile.dev]") {
                cargo_toml.insert_str(pos, &format!("{dep_line}\n\n"));
            } else {
                cargo_toml.push_str(&format!("\n{dep_line}\n"));
            }
            fs::write("Cargo.toml", cargo_toml).unwrap();
            println!("Successfully added 'groot-plugin-{name}' to Cargo.toml!");
        }
        "remove" => {
            let Some(name) = args.get(1) else {
                eprintln!("Error: missing plugin name. Usage: groot plugin remove <name>");
                return;
            };
            println!("Removing plugin '{name}' from Cargo.toml...");
            let cargo_toml = fs::read_to_string("Cargo.toml").unwrap_or_default();
            let filtered: Vec<&str> = cargo_toml
                .lines()
                .filter(|line| !line.contains(&format!("groot-plugin-{name}")))
                .collect();
            fs::write("Cargo.toml", filtered.join("\n")).unwrap();
            println!("Successfully removed 'groot-plugin-{name}'!");
        }
        _ => println!("Unknown plugin command. Use list, add, or remove."),
    }
}

fn cmd_info() {
    println!("==================================================");
    println!("  GROOT ENGINE INFO");
    println!("==================================================");
    println!("  groot version   : {VERSION}");
    println!("  supported targets: desktop, android (arm64)");
    println!("  cwd             : {}", std::env::current_dir().unwrap_or_default().display());
    println!("  assets dir      : assets/");
    println!("  project config  : assets/config.ron");
    println!("==================================================");
}

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
