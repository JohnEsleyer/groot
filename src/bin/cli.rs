use std::fs;
use std::io::{self, Write};
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
    #[serde(default)]
    path: String,
    #[serde(default)]
    repo: String,
}

#[derive(Debug, Clone)]
struct AdbDevice {
    serial: String,
    model: String,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ANSI color helpers
macro_rules! green {
    ($s:expr) => {
        format!("\x1b[32m{}\x1b[0m", $s)
    };
}
macro_rules! red {
    ($s:expr) => {
        format!("\x1b[31m{}\x1b[0m", $s)
    };
}
macro_rules! cyan {
    ($s:expr) => {
        format!("\x1b[36m{}\x1b[0m", $s)
    };
}
macro_rules! bold {
    ($s:expr) => {
        format!("\x1b[1m{}\x1b[0m", $s)
    };
}

fn print_help() {
    println!("{}", bold!("=================================================="));
    println!("{}", bold!("             GROOT GAME ENGINE CLI                "));
    println!("{}", bold!("=================================================="));
    println!("Usage: {} [args]", cyan!("groot <command>"));
    println!();
    println!("{}", bold!("Core Commands:"));
    println!(
        "  {} <name>             - Scaffold a new Groot project",
        cyan!("new")
    );
    println!(
        "  {} [--target <t>]      - Run game (targets: desktop, android)",
        cyan!("run")
    );
    println!(
        "  {} [--target <t>]    - Build release binary or APK",
        cyan!("build")
    );
    println!(
        "  {}                   - Check toolchains (Rust, Android, ADB, Vulkan)",
        cyan!("doctor")
    );
    println!();
    println!("{}", bold!("Asset & Code Generators:"));
    println!(
        "  {} <type> <name>  - Scaffold (prefab, script, scene)",
        cyan!("generate")
    );
    println!(
        "  {} [in] [out]          - Pack & encrypt assets into .gpak",
        cyan!("pack")
    );
    println!();
    println!("{}", bold!("Plugins & Info:"));
    println!(
        "  {} <list|add|rm>      - Manage native plugins",
        cyan!("plugin")
    );
    println!(
        "  {}                    - Print engine and workspace diagnostics",
        cyan!("info")
    );
    println!(
        "  {}                 - Print version",
        cyan!("version")
    );
    println!(
        "  {} <shell>        - Generate shell completions (bash, zsh, fish)",
        cyan!("completions")
    );
    println!("{}", bold!("=================================================="));
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
        "doctor" => cmd_doctor(),
        "generate" | "g" => cmd_generate(&args[2..]),
        "pack" => cmd_pack(&args[2..]),
        "completions" => cmd_completions(&args[2..]),
        "plugin" => cmd_plugin(&args[2..]),
        "info" => cmd_info(),
        "help" | "--help" | "-h" => print_help(),
        "version" | "--version" | "-V" => println!("groot {VERSION}"),
        other => {
            eprintln!("{} Unknown command '{}'", red!("Error:"), other);
            print_help();
            std::process::exit(2);
        }
    }
}

// ---------------------------------------------------------------------------
// `groot doctor` - Environment & Toolchain Diagnostics
// ---------------------------------------------------------------------------
fn cmd_doctor() {
    println!("{}", bold!("Checking Groot Development Environment...\n"));

    // 1. Check Rust compiler
    let has_rustc = check_tool("rustc", &["--version"], "Rust Compiler");

    // 2. Check cargo version
    check_tool("cargo", &["--version"], "Cargo");

    // 3. Check cargo-apk (for Android)
    let has_cargo_apk = check_tool("cargo", &["apk", "--help"], "cargo-apk (Android packager)");
    if !has_cargo_apk {
        println!(
            "      {} Install: {}",
            cyan!("→"),
            cyan!("cargo install cargo-apk")
        );
    }

    // 4. Check Android target
    let targets = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    let has_android_target = targets
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).contains("aarch64-linux-android")
        })
        .unwrap_or(false);
    if has_android_target {
        println!("  [{}] Target aarch64-linux-android installed", green!("✓"));
    } else {
        println!(
            "  [{}] Target aarch64-linux-android missing ({})",
            red!("✗"),
            cyan!("rustup target add aarch64-linux-android")
        );
    }

    // 5. Check ADB
    let has_adb = check_tool("adb", &["version"], "Android Debug Bridge (adb)");
    if !has_adb {
        println!("      {} Install: https://developer.android.com/studio#command-tools", cyan!("→"));
    }

    // 6. Check connected Android devices
    if has_adb {
        let dev_out = Command::new("adb")
            .args(["devices"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let devices: Vec<&str> = dev_out
            .lines()
            .filter(|l| l.ends_with("device") && !l.starts_with("List"))
            .collect();
        println!(
            "  [{}] Connected Android Devices: {}",
            cyan!("i"),
            devices.len()
        );
        for d in &devices {
            println!("        - {}", d);
        }
    }

    // 7. Check Vulkan (via vulkaninfo or wgpu adapter is runtime, so just warn)
    let vulkan_ok = check_tool("vulkaninfo", &["--summary"], "Vulkan (vulkaninfo)");
    if !vulkan_ok {
        println!("      {} Vulkan check skipped (optional, wgpu will probe at runtime)", cyan!("→"));
    }

    if has_rustc {
        println!("\n{}", green!("Environment check complete."));
    } else {
        println!("\n{} Some tools missing. See above.", red!("Check incomplete."));
    }
}

fn check_tool(cmd: &str, args: &[&str], label: &str) -> bool {
    match Command::new(cmd).args(args).output() {
        Ok(out) if out.status.success() => {
            println!("  [{}] {}", green!("✓"), label);
            true
        }
        _ => {
            println!("  [{}] {} not found", red!("✗"), label);
            false
        }
    }
}

// ---------------------------------------------------------------------------
// `groot generate` - Scaffolding for Prefabs, Scripts, and Scenes
// ---------------------------------------------------------------------------
fn cmd_generate(args: &[String]) {
    if args.len() < 2 {
        println!("Usage: {} <prefab|script|scene> <name>", cyan!("groot generate"));
        println!("  {} script <name>  - Create assets/scripts/<name>.gos", cyan!("groot g"));
        println!("  {} prefab <name>  - Create assets/prefabs/<name>.prefab.ron", cyan!("groot g"));
        println!("  {} scene <name>   - Create assets/scenes/<name>.scene.ron", cyan!("groot g"));
        return;
    }

    let kind = args[0].as_str();
    let name = args[1].as_str();
    if !is_valid_project_name(name) {
        eprintln!(
            "{} '{}' is not a valid name (alphanumeric, '-', '_', '.' only).",
            red!("Error:"),
            name
        );
        std::process::exit(1);
    }

    // PascalCase for struct name
    let struct_name = to_pascal_case(name);

    match kind {
        "script" => {
            let path = format!("assets/scripts/{name}.gos");
            let content = format!(
                r#"// {name}.gos - GoScript Controller
type {struct_name}Controller struct {{
    Speed float64
}}

var self = {struct_name}Controller{{
    Speed: 5.0,
}}

func OnUpdate(dt float64) {{
    var pos = groot.GetSelfPosition()
    // TODO: Add logic here...
    groot.SetSelfPosition(pos[0], pos[1], pos[2])
    groot.SetSelfCollider(1.0, 1.0, 1.0)
}}
"#
            );
            write_file(&path, content);
        }
        "prefab" => {
            let path = format!("assets/prefabs/{name}.prefab.ron");
            let content = format!(
                r#"(
    name: "{name}",
    script: Some("assets/scripts/{name}.gos"),
    transform: (
        position: (0.0, 0.0, 0.0),
        rotation: (0.0, 0.0, 0.0),
        scale: (1.0, 1.0, 1.0),
    ),
    visual: Some(Sprite(
        size: (1.0, 1.0),
        color: Rgba(1.0, 1.0, 1.0, 1.0),
        texture: None,
        layer: 0,
    )),
    collider: Some(Box2D(width: 1.0, height: 1.0)),
)
"#
            );
            write_file(&path, content);
        }
        "scene" => {
            let path = format!("assets/scenes/{name}.scene.ron");
            let content = format!(
                r#"(
    name: "{struct_name} Scene",
    environment: (
        ambient_light: Some((
            color: Rgba(1.0, 1.0, 1.0, 1.0),
            brightness: 0.5,
        )),
        camera: Some(Orthographic2D(
            viewport: (21.33, 12.0),
            position: (0.0, 0.0),
        )),
    ),
    entities: [],
)
"#
            );
            write_file(&path, content);
        }
        _ => eprintln!(
            "{} Unknown generator type '{}'. Use prefab, script, or scene.",
            red!("Error:"),
            kind
        ),
    }
}

fn write_file(path: &str, content: String) {
    if let Some(parent) = Path::new(path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    if Path::new(path).exists() {
        eprintln!("{} File '{}' already exists.", red!("Error:"), path);
        return;
    }
    if fs::write(path, content).is_ok() {
        println!("{} Created {}", green!("✓"), bold!(path));
    } else {
        eprintln!("{} Failed to write '{}'", red!("Error:"), path);
    }
}

fn to_pascal_case(s: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' || c == '-' || c == '.' {
            capitalize_next = true;
        } else if capitalize_next {
            out.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "My".to_string()
    } else {
        out
    }
}

// ---------------------------------------------------------------------------
// `groot pack` - Asset Encryption & Archiving
// ---------------------------------------------------------------------------
fn cmd_pack(args: &[String]) {
    if args.first().map(|s| s.as_str()) == Some("--help")
        || args.first().map(|s| s.as_str()) == Some("-h")
        || args.first().map(|s| s.as_str()) == Some("help")
    {
        println!("Usage: {} [input_dir] [output_file]", cyan!("groot pack"));
        println!("  Packs and encrypts assets into a .gpak archive.");
        println!("  Defaults: input_dir=assets  output_file=assets.gpak");
        println!("  Example: {}", cyan!("groot pack assets/ assets.gpak"));
        return;
    }
    let input = args.first().map(|s| s.as_str()).unwrap_or("assets");
    let output = args.get(1).map(|s| s.as_str()).unwrap_or("assets.gpak");

    println!(
        "Packaging & encrypting '{}' into '{}'...",
        cyan!(input),
        cyan!(output)
    );
    match groot::assets::GpakArchive::pack_directory(input, output) {
        Ok(_) => println!("{} Protected archive ready: {}", green!("✓"), bold!(output)),
        Err(e) => eprintln!("{} Failed to pack assets: {e}", red!("Error:")),
    }
}

// ---------------------------------------------------------------------------
// `groot completions` - Shell completions
// ---------------------------------------------------------------------------
fn cmd_completions(args: &[String]) {
    let shell = args.first().map(|s| s.as_str()).unwrap_or("");
    match shell {
        "bash" => {
            println!(
                r#"# Groot bash completions - source this or place in /etc/bash_completion.d/
_groot_completions() {{
    local cur="${{COMP_WORDS[COMP_CWORD]}}"
    local cmds="new run build doctor generate g pack plugin info help version completions"
    COMPREPLY=( $(compgen -W "$cmds" -- "$cur") )
}}
complete -F _groot_completions groot
"#
            );
        }
        "zsh" => {
            println!(
                r#"#compdef groot
_groot() {{
    local -a commands
    commands=(
        'new:Scaffold a new Groot project'
        'run:Run game'
        'build:Build release bundle'
        'doctor:Check toolchains'
        'generate:Scaffold prefab/script/scene'
        'g:Alias for generate'
        'pack:Pack & encrypt assets'
        'plugin:Manage plugins'
        'info:Print engine info'
        'completions:Generate shell completions'
    )
    _describe 'command' commands
}}
_groot
"#
            );
        }
        "fish" => {
            println!(
                r#"# Groot fish completions
complete -c groot -f
complete -c groot -n "__fish_use_subcommand" -a new -d "Scaffold new project"
complete -c groot -n "__fish_use_subcommand" -a run -d "Run game"
complete -c groot -n "__fish_use_subcommand" -a build -d "Build release"
complete -c groot -n "__fish_use_subcommand" -a doctor -d "Check toolchains"
complete -c groot -n "__fish_use_subcommand" -a generate -d "Scaffold asset"
complete -c groot -n "__fish_use_subcommand" -a pack -d "Pack assets"
complete -c groot -n "__fish_use_subcommand" -a plugin -d "Manage plugins"
complete -c groot -n "__fish_use_subcommand" -a info -d "Print info"
complete -c groot -n "__fish_use_subcommand" -a completions -d "Generate completions"
"#
            );
        }
        _ => {
            println!("Usage: groot completions <bash|zsh|fish>");
            println!("  Example: groot completions bash > /etc/bash_completion.d/groot");
            println!("           groot completions zsh > ~/.zsh/completions/_groot");
        }
    }
}

/// Verify that `cargo-apk` is installed before running Android targets.
fn ensure_cargo_apk_installed() {
    // Use --help because cargo-apk does not implement --version
    let installed = Command::new("cargo")
        .args(["apk", "--help"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);

    if !installed {
        eprintln!("{} 'cargo-apk' is not installed on your system.", red!("Error:"));
        eprintln!();
        eprintln!("To build and run Android APKs with Groot, please install 'cargo-apk':");
        eprintln!("  {}", cyan!("cargo install cargo-apk"));
        eprintln!();
        eprintln!("Also ensure the Android Rust compilation target is installed:");
        eprintln!("  {}", cyan!("rustup target add aarch64-linux-android"));
        eprintln!();
        std::process::exit(1);
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

/// Extract `--device <serial_or_index>` or `-d <serial_or_index>` from the arg list.
fn parse_device_arg(args: &[String]) -> Option<String> {
    for i in 0..args.len() {
        if (args[i] == "--device" || args[i] == "-d") && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

/// Resolve explicit `--bin <name>` or inspect `Cargo.toml` for `default-run`.
fn resolve_bin_target(args: &[String], workdir: &Path) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == "--bin" && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }

    let cargo_toml_path = workdir.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(&cargo_toml_path) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("default-run") {
                if let Some(val) = line.split('=').nth(1) {
                    let clean = val.trim().trim_matches('"').trim_matches('\'').trim();
                    if !clean.is_empty() {
                        return Some(clean.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Query connected online devices from `adb devices -l`.
fn get_adb_devices() -> Vec<AdbDevice> {
    let output = match Command::new("adb").args(["devices", "-l"]).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => return Vec::new(),
    };

    let mut devices = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices attached") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "device" {
            let serial = parts[0].to_string();
            let mut model = String::new();
            for part in &parts[2..] {
                if let Some(m) = part.strip_prefix("model:") {
                    model = m.to_string();
                }
            }
            if model.is_empty() {
                model = serial.clone();
            }
            devices.push(AdbDevice { serial, model });
        }
    }
    devices
}

/// Interactively or programmatically select an Android ADB device.
fn select_adb_device(args: &[String]) -> Option<String> {
    let devices = get_adb_devices();

    if devices.is_empty() {
        println!("{} No online Android devices detected via `adb devices`.", cyan!("Notice:"));
        return None;
    }

    if let Some(user_arg) = parse_device_arg(args) {
        if let Ok(idx) = user_arg.parse::<usize>() {
            if idx >= 1 && idx <= devices.len() {
                let dev = &devices[idx - 1];
                println!("Using Android device #{idx}: {} ({})", dev.model, dev.serial);
                return Some(dev.serial.clone());
            }
        }
        for dev in &devices {
            if dev.serial.eq_ignore_ascii_case(&user_arg)
                || dev.model.eq_ignore_ascii_case(&user_arg)
            {
                println!("Using Android device: {} ({})", dev.model, dev.serial);
                return Some(dev.serial.clone());
            }
        }
        println!("Using specified device serial: {user_arg}");
        return Some(user_arg);
    }

    if devices.len() == 1 {
        let dev = &devices[0];
        println!("Targeting Android device: {} ({})", dev.model, dev.serial);
        return Some(dev.serial.clone());
    }

    println!("\nMultiple Android devices detected:");
    for (i, dev) in devices.iter().enumerate() {
        println!("  [{}] {} ({})", i + 1, dev.model, dev.serial);
    }
    print!("Select device [1-{}, default 1]: ", devices.len());
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            let dev = &devices[0];
            println!("Defaulting to device #1: {} ({})", dev.model, dev.serial);
            return Some(dev.serial.clone());
        }
        if let Ok(choice) = trimmed.parse::<usize>() {
            if choice >= 1 && choice <= devices.len() {
                let dev = &devices[choice - 1];
                println!("Selected device #{choice}: {} ({})", dev.model, dev.serial);
                return Some(dev.serial.clone());
            }
        }
    }

    let dev = &devices[0];
    println!("Falling back to device #1: {} ({})", dev.model, dev.serial);
    Some(dev.serial.clone())
}

fn cmd_new(args: &[String]) {
    let Some(name) = args.first() else {
        eprintln!("{} missing project name. Usage: groot new <name>", red!("Error:"));
        std::process::exit(1);
    };
    if !is_valid_project_name(name) {
        eprintln!(
            "{} '{}' is not a valid project name (alphanumeric, '-', '_', '.' only).",
            red!("Error:"),
            name
        );
        std::process::exit(1);
    }
    let project_path = Path::new(name);
    if project_path.exists() {
        eprintln!("{} '{}' already exists.", red!("Error:"), name);
        std::process::exit(1);
    }

    println!("Initializing Groot project: '{}' ...", cyan!(name));

    fs::create_dir_all(project_path.join("src")).unwrap_or_else(|e| {
        eprintln!("{} creating src: {e}", red!("Error:"));
        std::process::exit(1);
    });
    fs::create_dir_all(project_path.join("assets/prefabs")).unwrap_or_else(|e| {
        eprintln!("{} creating assets/prefabs: {e}", red!("Error:"));
        std::process::exit(1);
    });
    fs::create_dir_all(project_path.join("assets/scenes")).unwrap_or_else(|e| {
        eprintln!("{} creating assets/scenes: {e}", red!("Error:"));
        std::process::exit(1);
    });
    fs::create_dir_all(project_path.join("assets/scripts")).unwrap_or_else(|e| {
        eprintln!("{} creating assets/scripts: {e}", red!("Error:"));
        std::process::exit(1);
    });

    write_or_die(
        project_path.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[lib]
name = "{name}"
path = "src/lib.rs"
crate-type = ["cdylib", "rlib"]

[dependencies]
groot = {{ git = "https://github.com/JohnEsleyer/groot" }}

[profile.dev]
opt-level = 1
"#
        ),
    );

    write_or_die(
        project_path.join("src/lib.rs"),
        r#"pub use groot::*;"#.to_string(),
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
    println!("{} Project '{}' created successfully!", green!("✓"), name);
    println!();
    println!("  {}/Cargo.toml", project_path.display());
    println!("  {}/src/lib.rs", project_path.display());
    println!("  {}/src/main.rs", project_path.display());
    println!("  {}/assets/config.ron", project_path.display());
    println!("  {}/assets/prefabs/player.prefab.ron", project_path.display());
    println!("  {}/assets/scenes/main.scene.ron", project_path.display());
    println!("  {}/assets/scripts/player.gos", project_path.display());
    println!("  {}/.gitignore", project_path.display());
    println!();
    println!("Next: {} && {}", cyan!(format!("cd {name}")), cyan!("groot run"));
}

fn cmd_run(args: &[String]) {
    let workdir = resolve_workdir(None);
    ensure_cargo_project(&workdir);

    let target = parse_target(args);
    let bin_target = resolve_bin_target(args, &workdir);

    match target {
        "android" => {
            ensure_cargo_apk_installed();
            let device_serial = select_adb_device(args);
            println!("{}", cyan!("Deploying and running Groot APK on Android device..."));
            let mut cmd = Command::new("cargo");
            cmd.current_dir(&workdir);

            let mut cargo_args = vec!["apk", "run", "--target", "aarch64-linux-android", "--lib"];

            if let Some(ref serial) = device_serial {
                cmd.env("ANDROID_SERIAL", serial);
                cargo_args.push("--device");
                cargo_args.push(serial);
            }

            cmd.args(cargo_args);

            let status = cmd.status().unwrap_or_else(|e| {
                eprintln!("{} failed to launch cargo-apk: {e}", red!("Error:"));
                std::process::exit(1);
            });
            std::process::exit(status.code().unwrap_or(0));
        }
        "desktop" => {
            println!(
                "Running Desktop Groot game in '{}' ...",
                cyan!(workdir.display().to_string())
            );
            let mut cmd = Command::new("cargo");
            cmd.current_dir(&workdir);
            let mut cargo_args = vec!["run"];
            if let Some(ref bin) = bin_target {
                cargo_args.push("--bin");
                cargo_args.push(bin);
            }
            cmd.args(cargo_args);

            let status = cmd.status().unwrap_or_else(|e| {
                eprintln!("{} failed to launch cargo: {e}", red!("Error:"));
                std::process::exit(1);
            });
            std::process::exit(status.code().unwrap_or(0));
        }
        other => {
            eprintln!("{} unknown target '{}'. Use desktop or android.", red!("Error:"), other);
            std::process::exit(2);
        }
    }
}

fn cmd_build(args: &[String]) {
    let workdir = resolve_workdir(None);
    ensure_cargo_project(&workdir);

    let target = parse_target(args);
    let bin_target = resolve_bin_target(args, &workdir);

    match target {
        "android" => {
            ensure_cargo_apk_installed();
            println!("{}", cyan!("Building Android ARM64 APK bundle..."));
            let mut cmd = Command::new("cargo");
            cmd.current_dir(&workdir);
            cmd.args([
                "apk",
                "build",
                "--target",
                "aarch64-linux-android",
                "--release",
                "--lib",
            ]);

            let status = cmd.status().unwrap_or_else(|e| {
                eprintln!("{} failed to launch cargo-apk: {e}", red!("Error:"));
                std::process::exit(1);
            });
            std::process::exit(status.code().unwrap_or(0));
        }
        "desktop" => {
            println!(
                "Building Desktop release binary in '{}' ...",
                cyan!(workdir.display().to_string())
            );
            let mut cmd = Command::new("cargo");
            cmd.current_dir(&workdir);
            let mut cargo_args = vec!["build", "--release"];
            if let Some(ref bin) = bin_target {
                cargo_args.push("--bin");
                cargo_args.push(bin);
            }
            cmd.args(cargo_args);

            let status = cmd.status().unwrap_or_else(|e| {
                eprintln!("{} failed to launch cargo: {e}", red!("Error:"));
                std::process::exit(1);
            });
            std::process::exit(status.code().unwrap_or(0));
        }
        other => {
            eprintln!("{} unknown target '{}'. Use desktop or android.", red!("Error:"), other);
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
                            println!("  {} — {}", cyan!(&p.name), p.description);
                            if !p.repo.is_empty() {
                                println!("    by {} | {} | groot plugin add {}", p.author, p.repo, p.name);
                            } else {
                                println!("    by {} | groot plugin add {}", p.author, p.name);
                            }
                            if !p.path.is_empty() {
                                println!("    local: {}", p.path);
                            }
                            println!();
                        }
                    }
                    Err(e) => {
                        eprintln!("{} parsing plugin registry: {e}", red!("Error:"));
                    }
                }
            } else {
                eprintln!("{} No plugin registry found at ../groot-plugins/index.ron", red!("Notice:"));
                eprintln!("Available plugins:");
                eprintln!("  {}   — Simple audio synthesizer & sound effects", cyan!("audio"));
                eprintln!("  {}  — 2D/3D debug shape line drawer", cyan!("gizmos"));
            }
        }
        "add" => {
            let Some(name) = args.get(1) else {
                eprintln!("{} missing plugin name. Usage: groot plugin add <name>", red!("Error:"));
                return;
            };
            println!("Installing plugin '{}' into Cargo.toml...", cyan!(name));
            // Try to resolve repo/path from registry
            let dep_line = if let Ok(content) = fs::read_to_string("../groot-plugins/index.ron") {
                if let Ok(reg) = ron::from_str::<Registry>(&content) {
                    if let Some(entry) = reg.plugins.iter().find(|p| p.name == *name) {
                        if !entry.repo.is_empty() {
                            format!("groot-plugin-{name} = {{ git = \"{}\" }}", entry.repo)
                        } else if !entry.path.is_empty() {
                            format!("groot-plugin-{name} = {{ path = \"{}\" }}", entry.path)
                        } else {
                            format!("groot-plugin-{name} = {{ path = \"../groot-plugin-{name}\" }}")
                        }
                    } else {
                        eprintln!("{} plugin '{}' not found in registry, using local path fallback", red!("Warning:"), name);
                        format!("groot-plugin-{name} = {{ path = \"../groot-plugin-{name}\" }}")
                    }
                } else {
                    format!("groot-plugin-{name} = {{ path = \"../groot-plugin-{name}\" }}")
                }
            } else {
                // No registry – fallback to path, but prefer git if we can guess
                format!("groot-plugin-{name} = {{ git = \"https://github.com/JohnEsleyer/groot-plugin-{name}\" }}")
            };
            let mut cargo_toml = fs::read_to_string("Cargo.toml").unwrap_or_default();
            if cargo_toml.contains(&format!("groot-plugin-{name}")) {
                println!("Plugin 'groot-plugin-{name}' is already installed.");
                return;
            }
            if let Some(pos) = cargo_toml.find("[profile.dev]") {
                cargo_toml.insert_str(pos, &format!("{dep_line}\n\n"));
            } else {
                cargo_toml.push_str(&format!("\n{dep_line}\n"));
            }
            fs::write("Cargo.toml", cargo_toml).unwrap();
            println!("{} Successfully added 'groot-plugin-{name}' to Cargo.toml!", green!("✓"));
            println!("  {} {}", cyan!("→"), dep_line);
        }
        "remove" | "rm" => {
            let Some(name) = args.get(1) else {
                eprintln!("{} missing plugin name. Usage: groot plugin remove <name>", red!("Error:"));
                return;
            };
            println!("Removing plugin '{}' from Cargo.toml...", cyan!(name));
            let cargo_toml = fs::read_to_string("Cargo.toml").unwrap_or_default();
            let filtered: Vec<&str> = cargo_toml
                .lines()
                .filter(|line| !line.contains(&format!("groot-plugin-{name}")))
                .collect();
            fs::write("Cargo.toml", filtered.join("\n")).unwrap();
            println!("{} Successfully removed 'groot-plugin-{name}'!", green!("✓"));
        }
        _ => println!("Unknown plugin command. Use list, add, or remove."),
    }
}

fn cmd_info() {
    println!("{}", bold!("=================================================="));
    println!("{}", bold!("  GROOT ENGINE INFO"));
    println!("{}", bold!("=================================================="));
    println!("  groot version   : {}", cyan!(VERSION));
    println!("  supported targets: {}", cyan!("desktop, android (arm64)"));
    println!("  cwd             : {}", std::env::current_dir().unwrap_or_default().display());
    println!("  assets dir      : {}", cyan!("assets/"));
    println!("  project config  : {}", cyan!("assets/config.ron"));
    println!("{}", bold!("=================================================="));
}

fn resolve_workdir(dir: Option<&str>) -> PathBuf {
    match dir {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir().unwrap_or_else(|e| {
            eprintln!("{} cannot resolve current directory: {e}", red!("Error:"));
            std::process::exit(1);
        }),
    }
}

fn ensure_cargo_project(workdir: &Path) {
    if !workdir.join("Cargo.toml").exists() {
        eprintln!(
            "{} no Cargo.toml in '{}'. Run this from a Groot/Cargo project.",
            red!("Error:"),
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
        eprintln!("{} writing {}: {e}", red!("Error:"), path.display());
        std::process::exit(1);
    });
}
