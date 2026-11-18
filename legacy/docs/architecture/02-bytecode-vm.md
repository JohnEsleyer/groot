# 02 — Bytecode and the Virtual Machine

GoScript runs on a stack machine. The instruction set is small (49 opcodes),
the operand encoding is explicit, and every runtime error carries the source
line that produced it.

## The chunk

A compiled function is a `CompiledFunction`:

```rust
pub struct CompiledFunction {
    pub name: String,
    pub arity: usize,      // parameter count
    pub max_locals: usize, // total local slots (params + declared)
    pub code: Vec<u8>,     // bytecode
    pub constants: Vec<Value>,
    pub lines: Vec<usize>, // source line per bytecode byte
}
```

`code` and `lines` are parallel arrays: `lines[i]` is the source line of
`code[i]`. The VM uses it to anchor errors.

## The machine

```rust
struct CallFrame {
    function: Rc<CompiledFunction>,
    ip: usize,          // instruction pointer
    slots_offset: usize, // where this frame's locals start on the stack
}
```

There is **one shared operand stack** for the whole VM and a stack of call
frames. Locals live *on the operand stack*: a frame's locals occupy
`slots_offset .. slots_offset + max_locals`, and the arguments a call pushed
become locals `0..arity`.

```
   ┌──────────────────────────────┐
   │ operand stack (Values)       │   ← temporaries + locals of all frames
   ├──────────────────────────────┤
   │ frames: [main, OnUpdate, …]  │   ← each with its own ip + slots_offset
   └──────────────────────────────┘
```

A call therefore works like this:

1. The caller evaluates the arguments and pushes them.
2. The callee's frame is pushed with `slots_offset = stack.len() - arity`, so
   the pushed arguments *are* the callee's locals `0..arity`.
3. The VM pre-fills the remaining local slots (`arity..max_locals`) with `nil`,
   so a local declared inside a branch that never executed is still valid.
4. On `Return`, the result is pushed and the stack is truncated back to
   `slots_offset`.

## Instruction set

| Opcode | # | Stack effect |
| --- | --- | --- |
| `Constant` | 0 | push constant |
| `Nil` / `True` / `False` | 1–3 | push literal |
| `Add` `Sub` `Mul` `Div` `Mod` | 4–8 | pop b, pop a, push a op b |
| `Negate` `Not` | 9–10 | pop, push |
| `Greater` `GreaterEqual` `Less` `LessEqual` `Equal` `NotEqual` | 11–16 | pop b, pop a, push bool |
| `Pop` | 17 | discard the top value |
| `GetGlobal` `SetGlobal` | 18–19 | global read/write by name |
| `GetLocal` `SetLocal` | 20–21 | local read/write by slot |
| `JumpIfFalse` `JumpIfFalseKeep` `JumpIfTrueKeep` `Jump` | 22–25 | conditional/unconditional jump |
| `Call` | 26 | call a global function |
| `Return` | 27 | return from a frame |
| `NewStruct` | 28 | build a struct instance |
| `GetField` `SetField` | 29–30 | struct field read/write |
| `NewSlice` `NewMap` | 31–32 | build a slice/map literal |
| `GetIndex` `SetIndex` | 33–34 | index read/write |
| `CallMethod` | 35 | call a receiver method |
| `SliceRange` | 36 | `obj[start:end]` |
| `ConstantWide` | 37 | push constant (16-bit index) |
| `GetGlobalWide` `SetGlobalWide` | 38–39 | global read/write (16-bit name index) |
| `GetLocalWide` `SetLocalWide` | 40–41 | local read/write (16-bit slot) |
| `CallWide` `CallMethodWide` | 42–43 | call (16-bit name index) |
| `GetFieldWide` `SetFieldWide` | 44–45 | field read/write (16-bit name index) |
| `NewStructWide` | 46 | struct literal (16-bit name index + field count) |
| `NewSliceWide` `NewMapWide` | 47–48 | collection literal (16-bit count) |

## Operand encoding

Two rules keep the bytecode compact without ever truncating an operand.

**Constants, names and slots are 8-bit while they fit, 16-bit beyond.**
The compiler picks the narrow or wide opcode automatically:

| Situation | Encoding |
| --- | --- |
| constant index ≤ 255 | `Constant` + 1 byte |
| constant index > 255 | `ConstantWide` + 2 bytes (little-endian) |
| local slot ≤ 255 | `GetLocal` + 1 byte |
| local slot > 255 | `GetLocalWide` + 2 bytes |
| global/field/callee name index ≤ 255 | `GetGlobal` + 1 byte |
| … > 255 | `GetGlobalWide` + 2 bytes |

The constant pool therefore holds up to **65 536 entries per function** —
a behaviour tree or dialogue table no longer hits a 255-entry ceiling.

**Jump offsets are 16-bit signed**, relative to the instruction *after* the
operand:

```rust
let delta = target as i64 - (operand_pos as i64 + 2);
let bytes = (delta as i16).to_le_bytes();
```

An 8-bit offset wrapped around as soon as a block exceeded 127 bytes, which
silently sent the VM into the middle of unrelated code. The range is now
±32 767 bytes; a jump that does not fit is a **compile-time error**, not
silent corruption.

## Calls

`Call` (and `CallWide`) resolves a callee by name:

1. **Host function?** (`host_fns["groot.Log"]`) — the arguments are popped and
   passed to the Rust closure; the result is pushed.
2. **Native global?** (`Value::NativeFn`) — same, but stored in the global
   table.
3. **Script function?** (`Value::Function`) — a new frame is pushed, with the
   already-pushed arguments becoming its locals.
4. Otherwise: *"callee 'x' is not a function"* — a runtime error.

`CallMethod` pops the receiver and the arguments, resolves
`<TypeName>.<Method>` from the receiver's runtime type name, and dispatches
the same way. Host functions win over script globals, so a Rust-native
receiver method can override a script one.

## Runtime errors

Errors carry the source line of the failing instruction:

```
goscript: index 9 out of bounds for slice of length 3 (line 12)
```

The VM reports, among others:

* index out of bounds (slices, strings),
* negative slice bounds and inverted slice ranges,
* division by zero (integer),
* "callee 'x' is not a function",
* "method 'x' is not defined for 'Type'",
* operand-stack misalignment (an internal consistency check that turns a
  silent memory corruption into a readable error),
* the anti-freeze budget.

## The anti-freeze guard

Every VM has an instruction budget (1 000 000 by default):

```rust
vm.set_max_instructions(Some(1_000_000));   // or None to disable
```

When a script exceeds it, the VM raises

```
instruction budget exceeded (1000000 instructions) - possible infinite loop
```

with the line number of the instruction that blew the budget. A designer
accidentally writing `for { }` gets an error in the log instead of a frozen
game — the engine keeps running other scripts.

## Host functions

```rust
vm.register_fn("groot.Log", |args: Vec<Value>| -> Value {
    println!("{}", args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
    Value::Nil
});
```

* Host functions are stored as `Rc<dyn Fn(Vec<Value>) -> Value>`, so they can
  capture state (the engine captures the input snapshot this way).
* A dotted name automatically registers the package prefix with the parser,
  so `groot.Log(…)` parses as a package call.
* The VM holds one `host_fns` table per instance. Because Groot gives every
  entity its own VM, the host functions are registered per instance — see
  [`04-script-runtime.md`](04-script-runtime.md).
