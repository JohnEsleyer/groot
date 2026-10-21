# 01 — The Compiler Pipeline

GoScript turns a `.gos` file into bytecode in four passes. Each pass has one
job and produces a well-defined output, which makes the whole thing easy to
trace when something misbehaves.

```
source text
   │
   ├─ lexer      → Vec<Token>       tokens + automatic semicolon insertion
   │
   ├─ parser     → Vec<Stmt>        the AST (a flat statement list)
   │
   ├─ resolver   → Vec<Stmt>        imports inlined, module symbols namespaced
   │
   └─ compiler   → CompiledFunction bytecode + constant pool
```

The pipeline lives in the `goscript` crate (`src/lexer.rs`, `src/parser.rs`,
`src/resolver.rs`, `src/compiler.rs`). It is completely independent of Groot —
the engine only ever sees the compiled chunk.

## 1. Lexer

`lexer.rs` turns characters into tokens. Each token carries `kind`, `line` and
`col` — there is no separate lexeme field; the payload lives inside the token
kind (`TokenKind::Identifier(String)`, `TokenKind::Str(String)`, …).

**Automatic semicolon insertion.** The lexer inserts a `Semicolon` token when a
line ends with a token that can end a statement (identifier, literal, `)`,
`}`, `]`, `return`, `break`, `continue`). This is what makes Go-style
line-oriented code parse without semicolons.

**Comments.** `//` to end of line, `/* … */` block comments (nesting is not
supported).

**Package names are resolved at parse time.** `declare_package("groot")` adds
a name to a process-wide registry; the parser then treats `groot.Log(...)` as a
package call instead of a struct field access. Registering a host function with
a dotted name does this automatically.

## 2. Parser

`parser.rs` is a hand-written recursive-descent parser producing a flat
`Vec<Stmt>` — there is no nested block structure in the AST, blocks are just
nested `Vec<Stmt>`s.

The expression grammar is a precedence cascade:

```
parse_expression
  └─ or          ||
      └─ and     &&
          └─ equality    == !=
              └─ comparison  < <= > >=
                  └─ term    + -
                      └─ factor  * / %
                          └─ unary    - !
                              └─ postfix   .field  [index]  [start:end]
                                  └─ primary
```

Postfix parsing handles three forms:

* `expr.field` — struct field access, or a package member access when the base
  is a registered package name.
* `expr[index]` — slice, map or string index.
* `expr[start:end]`, `expr[start:]`, `expr[:end]`, `expr[:]` — slice range.
  A missing bound becomes `None` in the AST and compiles to `nil`.

**Desugaring.** Several Go constructs are rewritten during parsing:

| Written | Becomes |
| --- | --- |
| `x := expr` | `var x = expr` (`Stmt::VarDecl`) |
| `x++` / `x--` | `x = x + 1` / `x = x - 1` |
| `x += expr` | `x = x + expr` |
| `for i, v := range xs` | an index loop over the slice |

Compound assignment and `++`/`--` are rejected on method-call targets, because
the receiver would be evaluated twice.

## 3. Resolver

`resolver.rs` implements the module system. It runs **after** parsing, because
import paths are only known then.

**Import resolution.** For every `import` statement:

1. The path is handed to a `ScriptResolver` — the host decides where files come
   from. `DiskScriptResolver` reads the filesystem; `MemoryScriptResolver` reads
   a map; Groot can supply encrypted/embedded bytes.
2. A directory import resolves to *all* `.go`/`.gos` files in it, which are
   parsed and concatenated into one package unit (Go's package model).
3. The module's namespace is chosen by priority: **explicit alias** →
   **declared `package` name** → **file stem**.

**Namespacing.** Inside an imported module, every top-level function and
variable is renamed to `module.symbol`:

```go
// utils/math.gos
func Lerp(a float64, b float64, t float64) float64 { … }
var scale = 2.0
```

becomes `math.Lerp` and `math.scale` in the global table.

**Self-qualification.** References inside the module to its *own* top-level
symbols are rewritten to the same dotted names, so the module can call itself
without knowing its namespace.

**Dotted-reference rewrite.** References in the importing file to an imported
module (`math.Lerp(…)`) are rewritten to dotted global names
(`"math.Lerp"`), which is what the compiler emits as a call target.

**Receiver methods are not namespaced** — `func (z *Zombie) Hit()` stays
`Zombie.Hit`, because method dispatch is by runtime type name.

**Circular imports are rejected** with a compile error naming the offending
path.

The resolver returns a **flattened** statement list: all modules are spliced
into one program. There is no module loader at runtime — the VM never knows
that modules exist.

## 4. Compiler

`compiler.rs` walks the statement list and emits bytecode.

**Function compilation.** Each `func` declaration becomes its own
`CompiledFunction` (code + constants + line map), stored as a constant in the
enclosing chunk and installed as a global.

**Scopes and locals.** The compiler tracks a stack of local names. Every local
gets a slot index; the total number of slots a function may use
(`max_locals`) is recorded at the end of compilation, so the VM can
pre-allocate them — that way a local declared inside a loop that never runs
still has a valid slot.

**Constant pool.** Every literal and every name used by an operand is stored
in the function's constant pool. The pool is addressed with an 8-bit operand
while it fits and with a 16-bit operand beyond 255 entries — see
[`02-bytecode-vm.md`](02-bytecode-vm.md).

**Hot-reload awareness.** The compiler takes an optional set of global names to
preserve. When it encounters a top-level `var` whose name is in that set, it
**does not emit the initialization** — the live value stays untouched while the
new code is installed. See [`03-hot-reload.md`](03-hot-reload.md).

**Line mapping.** Every emitted byte records the source line it came from, in
a parallel `lines` vector. That is what lets a runtime error report the
offending line.

## Entry points

```rust
// Compile a source string (used by tests and the standalone CLI).
let chunk = vm.compile(source)?;      // Rc<CompiledFunction>
vm.execute(chunk)?;                    // runs the top level

// Compile with a custom file source.
vm.set_resolver(Rc::new(MyResolver));
```

`HotReloadEngine` uses the same pipeline but adds file watching and the
preserve-set logic.
