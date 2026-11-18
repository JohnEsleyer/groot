# 03 — Hot Reloading

Hot reload is the feature that makes GoScript pleasant to iterate with: edit
a `.gos` file, save, and the running game picks up the new behaviour — without
losing the score, the player's position or the state of any entity.

## The goal

Two things must be true after a reload:

1. **New code runs.** Functions are replaced with the freshly compiled ones.
2. **Live state survives.** Global variables keep their current values.

The second one is the hard part: a naive "re-execute the top level" would
reset every global to its initial value.

## The mechanism

`HotReloadEngine` (`src/hot_reload.rs` in the `goscript` crate) does the work:

```rust
let mut engine = HotReloadEngine::new("assets/scripts/player.gos");

// Once per frame:
if engine.reload_if_changed()? {
    println!("script recompiled and live-swapped");
}
engine.vm.call("OnUpdate", vec![Value::Float(dt)])?;
```

### 1. Change detection

`reload_if_changed()` compares the modification time of the script file with
the one recorded at the last successful compile. It also watches **every
imported dependency** — the resolver returns the full set of files that were
inlined, and the engine records their timestamps. Editing an imported
utility file therefore reloads the scripts that use it.

### 2. Recompilation

The file is read, lexed, parsed and resolved exactly like a normal compile.
Before compiling, the engine snapshots the names of the live globals:

```rust
let preserve: HashSet<String> = self.vm.globals.keys().cloned().collect();
let compiler = Compiler::with_preserved_globals("main", preserve);
```

### 3. The preserve set

This is the trick. In the compiler, a top-level `var` whose name is in the
preserve set **does not emit its initialization**:

```rust
if self.scope_depth > 0 {
    // local — always initialised
} else if self.preserve_existing.contains(name) {
    // Hot reload: keep the live runtime value of an existing global.
} else {
    self.compile_expr(init);
    self.emit_set_global(name);
}
```

So re-executing the top level of

```go
var score = 0
func AddPoints(n int) { score = score + n }
```

installs the **new** `AddPoints` but leaves the live `score` alone. The
`SetGlobal` for `score` is simply not emitted.

Note the asymmetry: *functions* are always re-installed (they must be — that
is the point), *data* is preserved when it already exists, and *new* data
declarations are initialised normally.

## What is preserved, and what is not

| Thing | Behaviour on reload |
| --- | --- |
| Top-level `var` values | **Preserved** (the initialization is not re-run) |
| Functions | **Replaced** with the new bytecode |
| Struct types | Compile-time metadata only — nothing to preserve |
| Local variables | Not applicable — locals live on the stack |
| Entity state in the ECS | Untouched — the script never owned it |
| The engine's per-entity VM instances | Untouched (see below) |

## Hot reload in a multi-entity world

Groot runs **one VM instance per entity** (see
[`04-script-runtime.md`](04-script-runtime.md)). Reloading therefore has two
levels:

1. **The compiled template** (one per script file) is recompiled. This is
   what `HotReloadEngine` owns.
2. **Every live instance** receives the new function bodies, while keeping its
   own global state.

```rust
fn sync_instances(loaded: &mut LoadedScript) {
    for instance in loaded.instances.values_mut() {
        for (name, value) in loaded.engine.vm.globals.iter() {
            match value {
                // Functions: always replace — that is the code swap.
                Value::Function(_) => instance.globals.insert(name.clone(), value.clone()),
                // Data: keep the instance's own value; seed only new names.
                _ => instance.globals.entry(name.clone())
                       .or_insert_with(|| value.deep_clone()),
            }
        }
    }
}
```

The effect for a designer: two pipes hit by the bird keep their own `Hit`
flags, and editing the pipe script updates both pipes' behaviour at once —
without either pipe losing its state.

## Release builds

Hot reload only makes sense when the script lives on disk. In release builds
(and on Android) Groot loads the (decrypted, embedded) script source into
memory and compiles it once with `HotReloadEngine::from_str`. There is no
file watching, and the template is compiled exactly once.

## Failure behaviour

A syntax error in the edited file does **not** take the game down:

* The compile fails, the error is logged with the line and column.
* The old bytecode stays installed — the previous version keeps running.
* The next save retries the compile.

That is the whole point of keeping the compiled chunk separate from the
source: the running game always has a valid, working script.

## Dependency watching

The resolver returns the set of files it inlined:

```rust
let (ast, dep_paths) = resolve_imports_with_deps(ast, self.vm.resolver.as_ref())?;
```

The engine stores their timestamps and checks them on the next tick, so
editing `assets/scripts/utils/math.gos` reloads every script that imports it.
