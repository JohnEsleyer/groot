# 03 — Values and Collections

GoScript is dynamically typed, but it is *not* a "everything is a copy"
language. Knowing which values alias which is the difference between a game
that works and a game where every entity shares one health bar.

## The value model

```
Value::Nil
Value::Int(i64)
Value::Float(f64)
Value::String(String)
Value::Bool(bool)
Value::Struct(Rc<RefCell<StructInstance>>)
Value::Slice(Rc<RefCell<Vec<Value>>>)
Value::Map(Rc<RefCell<HashMap<String, Value>>>)
Value::Function(Rc<CompiledFunction>)
Value::NativeFn(fn(Vec<Value>) -> Value)
```

**Primitives are values.** Copying an `Int`, `Float`, `String` or `Bool`
copies the data.

**Compound types are references.** A struct, slice or map is a handle to a
shared, mutable instance (`Rc<RefCell<…>>`).

```
var p = Player{Hp: 100}
var alias = p        // alias is another handle to the SAME Player
alias.Hp = 50
// p.Hp is now 50 too
```

That is Go-like aliasing for structs and is intentional: it lets a script hand
a struct to a function without copying the whole thing.

## Structs

```go
type Player struct {
    Hp    int
    Score int
}

var p = Player{Hp: 100, Score: 0}
p.Hp = p.Hp - 25
```

* Fields live in a hash map inside the instance, keyed by name.
* The struct *type* is metadata: it is not enforced, and a field may be added
  or removed without a compile error.
* Reading a field that was never assigned yields `nil`.
* The type name is retained on the instance — it is what method dispatch and
  the `to_string()` representation use.

### Structs inside structs

```go
type Weapon struct {
    Damage int
}

type Player struct {
    Hp     int
    Sword  Weapon
}

var p = Player{Hp: 100, Sword: Weapon{Damage: 12}}
p.Sword.Damage = 20
```

Nested structs are nested references: `p.Sword` is a handle to the `Weapon`
instance stored in `p`'s field map.

## Slices

```go
var nums = []int{1, 2, 3}
nums = append(nums, 4)
var n = len(nums)
var first = nums[0]
```

* Backed by a growable `Vec<Value>` behind an `Rc<RefCell<…>>`.
* `append` mutates the shared instance and returns it.
* Indexing out of range is a **runtime error** (not `nil`); negative indexes
  are rejected too, because they would otherwise wrap around.

### Slices are references

```go
var a = []int{1, 2, 3}
var b = a
b[0] = 99
// a[0] is 99 as well — same instance
```

Use a slice range to take a private copy when you need one:

```go
var window = a[0:2]   // a copy: mutating window does not touch a
```

### Slice ranges

| Expression | Result |
| --- | --- |
| `a[1:3]` | elements 1 and 2 (start inclusive, end exclusive) |
| `a[1:]` | element 1 to the end |
| `a[:3]` | start to element 2 |
| `a[:]` | a copy of the whole slice |

Rules:

* The result is always a **copy** — never a view onto the original.
* Upper bounds are clamped to the length (`a[0:99]` is legal).
* A negative bound is an error (`a[-1:2]`).
* An inverted range is an error (`a[2:1]`).
* The same syntax works on strings and yields a string.

## Maps

```go
var inv = map[string]int{"potions": 3, "gold": 100}
inv["gold"] += 20
var count = len(inv)
var potions = inv["potions"]
```

* String-keyed only.
* Reading a missing key yields `nil` — it is *not* an error.
* Writing a missing key inserts it.
* Assignment to a map element works on the shared instance, so aliases see
  the update.

## Strings

Strings are immutable UTF-8.

```go
var s = "Hello"
var c = s[0]       // "H"  — by character
var w = s[1:4]     // "ell" — by character
var len = len(s)   // byte length
```

Indexing and slicing work on characters, so multi-byte text is safe.
`len(s)` is the byte length, which differs from the character count for
non-ASCII text.

## Copying: `deep_clone`

Compound values alias by default. When a host engine (or a script) needs an
independent copy, `Value::deep_clone()` produces one:

```rust
let copy = original.deep_clone();
```

* Structs, slices and maps are copied recursively.
* Functions are shared — they are immutable.
* **Cycles are preserved**: a struct whose field points back at itself is
  cloned into a copy with the same shape, instead of recursing forever. This
  matters for scene-graph-like data (parent ↔ child references).

This is the mechanism behind per-entity state isolation in the engine: each
entity gets a deep copy of the script's top-level state, so `var self =
PipeController{…}` is private to that entity
(see [`../architecture/04-script-runtime.md`](../architecture/04-script-runtime.md)).

## Truthiness

Used by `if`, `&&`, `||` and `!`:

| Value | Truthy |
| --- | --- |
| `nil` | no |
| `false` | no |
| `0` | no |
| `0.0` | no |
| `""` | no |
| everything else (including structs, slices, maps) | yes |

A non-empty slice or struct is always truthy — there is no "empty collection
is false" rule.

## Numbers

* `int` is a 64-bit signed integer; arithmetic wraps on overflow.
* `float64` is an IEEE-754 double.
* Mixing the two in arithmetic promotes to `float64`.
* Integer division and remainder by zero are runtime errors; float division by
  zero yields `inf`/`NaN` as usual.
* Comparisons between `int` and `float64` coerce to `float64`.

## Display

Every value has a readable representation, used by `fmt.Sprintf("%v", …)` and
by the engine's logs:

| Value | Printed as |
| --- | --- |
| `nil` | `nil` |
| `42` | `42` |
| `3.5` | `3.5` (whole floats print without a decimal point) |
| `"hi"` | `hi` |
| `true` | `true` |
| `Player{Hp: 100}` | `Player {Hp: 100}` |
| `[]int{1,2}` | `[1, 2]` |
| `map[string]int{"a":1}` | `{a: 1}` |
| function | `<func OnUpdate>` |
