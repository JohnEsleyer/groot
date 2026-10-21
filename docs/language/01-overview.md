# 01 — GoScript Overview

GoScript is the scripting language Groot embeds. It looks like Go, compiles to
a custom bytecode chunk, and runs on a small stack VM — with no garbage
collector, no threads and no operating-system access.

```go
// assets/scripts/player.gos
type Player struct {
    Speed float64
    Hp    int
}

var player = Player{
    Speed: 350.0,
    Hp:    100,
}

func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var newX = groot.Clamp(pos[0] + groot.GetAxis("Horizontal")*player.Speed*dt, -580.0, 580.0)
    groot.SetSelfPosition(newX, pos[1])

    player.Hp -= 1
    if player.Hp <= 0 {
        groot.DestroySelf()
    }
}
```

## Why a custom language

Groot's design constraint is *frame-driven determinism*: gameplay code must
run inside the render frame, on the main thread, with a bounded cost. Go's
runtime works against that:

| Go runtime trait | Why it hurts a game engine | GoScript's answer |
| --- | --- | --- |
| Stop-the-world GC | Frame spikes when the heap grows | No GC — values are reference counted (`Rc`) and scoped to the frame |
| Background goroutines | Non-deterministic ordering, hard-to-reason-about races | Nothing runs unless the engine calls it |
| Large runtime + cgo | Binary bloat, FFI pain | Pure Rust, zero dependencies, compiles to WASM/Android/desktop |
| Static compilation | Slow iteration for designers | Interpreted bytecode + hot reload on file save |

The result is a language that keeps Go's *syntax* — the part designers already
know — and throws away the runtime.

## The dialect in one page

GoScript is a **subset** of Go with a few conveniences:

**Kept from Go**

* `var`, `:=`, `func`, `type ... struct`, `if`/`else`, `for` (all three
  forms), `switch`/`case`/`default`, `break`, `continue`, `return`.
* Receiver methods: `func (p *Player) TakeDamage(amount int)`.
* Slices (`[]int{1, 2, 3}`), maps (`map[string]int{"a": 1}`), `for … range`.
* Struct literals with field names: `Player{Hp: 100}`.
* `//` and `/* */` comments, automatic semicolon insertion.

**Dropped from Go**

* Goroutines, channels, `select`.
* `defer`, `panic`/`recover` (runtime errors are reported, not recoverable).
* Interfaces, generics, pointers as a separate type, `unsafe`.
* The full standard library — only a curated game-focused subset exists
  (see [`04-standard-library.md`](04-standard-library.md)).
* The file system, network, and process APIs: the language is sandboxed by
  construction.

**Added for game scripting**

* **Slice ranges** — `xs[1:3]`, `xs[1:]`, `xs[:3]`, `xs[:]`, on slices *and*
  strings.
* **Character indexing for strings** — `"hello"[0]` is `"h"`.
* **Native host bindings** — `groot.*` functions are first-class calls with
  no wrapper cost.
* **Runtime errors carry source line numbers**, so a broken script tells the
  designer *where* it broke.
* **An anti-freeze guard** — a runaway `for {}` raises an error instead of
  hanging the engine.

## Types

| Type | Notes |
| --- | --- |
| `nil` | The absence of a value. Reading an unset global yields `nil`. |
| `int` | 64-bit signed integer. Type annotations are accepted and ignored — `var hp int = 100` and `var hp = 100` are the same. |
| `float64` | 64-bit float. `int` and `float64` mix freely in arithmetic. |
| `string` | UTF-8 text. Immutable. |
| `bool` | `true` / `false`. |
| struct | `type Player struct { … }`; instances carry a dynamic field map. |
| slice | `[]T{…}` — an ordered, growable list. |
| map | `map[K]V{…}` — string-keyed dictionary. |
| function | Declared with `func`; callable by name. |

There is no static type system: annotations are documentation, and the VM
checks values at runtime. See
[`03-values-and-collections.md`](03-values-and-collections.md) for the
semantics of copying and reference types.

## Execution model

```
source (.gos)
   │  lexer          — tokens, automatic semicolon insertion
   │  parser         — AST
   │  resolver       — resolve `import`s, namespace module symbols
   │  compiler       — bytecode chunk + constant pool
   ▼
VirtualMachine
   │  call("OnUpdate", [dt])   ← the engine calls this, once per frame
   ▼
frame results (transform, colour, collider, …) → back into the ECS
```

The engine owns the clock. A script never sees wall-clock time except through
`time.Delta()` (the current frame's delta) and `time.Now()` (monotonic engine
runtime).

## Error handling

There are no exceptions. A runtime error aborts the current call and is
reported to the host with the offending source line:

```
[GROOT SCRIPT ERROR] entity #42: goscript: index 9 out of bounds for slice of length 3 (line 12)
```

The engine logs the error and keeps running — one broken script does not take
the game down.

## Where to go next

* Syntax: [`02-syntax-reference.md`](02-syntax-reference.md)
* Values and collections: [`03-values-and-collections.md`](03-values-and-collections.md)
* Standard library: [`04-standard-library.md`](04-standard-library.md)
* The `groot.*` engine API: [`05-groot-host-api.md`](05-groot-host-api.md)
* How the VM is built: [`../architecture/01-compiler-pipeline.md`](../architecture/01-compiler-pipeline.md)
