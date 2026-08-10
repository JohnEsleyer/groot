use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("==================================================");
    println!("            GROOT GAME ENGINE CLI                 ");
    println!("==================================================");
    println!("Usage: groot <command> [args]");
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
            println!("groot {VERSION}");
        }
        other => {
            eprintln!("Error: unknown command '{other}'");
            print_help();
            std::process::exit(2);
        }
    }
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

    fs::create_dir_all(project_path.join(".vscode")).unwrap_or_else(|e| {
        eprintln!("Error creating .vscode: {e}");
        std::process::exit(1);
    });

    write_or_die(
        project_path.join(".vscode/settings.json"),
        r#"{
  "files.associations": {
    "*.gos": "go"
  }
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
.vscode/*
!.vscode/settings.json

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
    println!("  {}/assets/config.ron", project_path.display());
    println!("  {}/assets/prefabs/player.prefab.ron", project_path.display());
    println!("  {}/assets/scenes/main.scene.ron", project_path.display());
    println!("  {}/assets/scripts/player.gos", project_path.display());
    println!("  {}/.vscode/settings.json", project_path.display());
    println!("  {}/.gitignore", project_path.display());
    println!();
    println!("Next: run `groot run` to play.");
}

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
    println!("  groot version : {VERSION}");
    println!("  cargo package     : groot");
    println!("  cwd               : {}", std::env::current_dir().unwrap_or_default().display());
    println!("  assets dir        : assets/");
    println!("  project config    : assets/config.ron");
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
