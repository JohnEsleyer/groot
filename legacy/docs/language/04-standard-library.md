# 04 — Standard Library

GoScript ships a small, curated standard library. It is implemented as native
Rust functions registered on every VM, so calls cost a normal function call —
there is no interpreter overhead for the library itself.

Everything here is **sandboxed by construction**: there is no filesystem, no
network and no process API. A third-party script cannot reach the machine.

## Built-ins (global scope)

| Function | Signature | Behaviour |
| --- | --- | --- |
| `len` | `len(coll) -> int` | Length of a slice, map or string (bytes for strings). |
| `append` | `append(slice, item) -> slice` | Appends one element to a slice and returns it. |
| `int` | `int(v) -> int` | Casts a float, string or bool to `int`. Strings are parsed; unparsable strings become `0`. |
| `float64` | `float64(v) -> float64` | Casts an int, string or float to `float64`. |
| `string` | `string(v) -> string` | Converts any value to its display form. |
| `bool` | `bool(v) -> bool` | Coerces any value to a boolean by truthiness. |

```go
var n = len(nums)              // 3
var i = int("42")              // 42
var s = string(42)             // "42"
var b = bool(0)                // false
```

## Package `math`

Numeric helpers. `Abs`, `Min`, `Max` and `Clamp` preserve integer arguments
(they return an `int` when given one); everything else returns a `float64`.

| Function | Description |
| --- | --- |
| `math.Pi` | Constant, `3.14159…` |
| `math.Sqrt(x)` | Square root |
| `math.Abs(x)` | Absolute value |
| `math.Sin(x)` / `math.Cos(x)` | Trigonometry (radians) |
| `math.Atan2(y, x)` | Two-argument arctangent |
| `math.Min(a, b)` / `math.Max(a, b)` | Smaller / larger of two values |
| `math.Clamp(v, min, max)` | Constrain `v` to the range |
| `math.Floor(x)` / `math.Ceil(x)` / `math.Round(x)` | Rounding |
| `math.Pow(base, exp)` | Exponentiation |

```go
var falloff = math.Clamp(1.0 / math.Sqrt(distance), 0.1, 1.0)
```

## Package `fmt`

| Function | Description |
| --- | --- |
| `fmt.Println(...)` | Prints the arguments separated by spaces to stdout. |
| `fmt.Sprintf(format, ...)` | Formats a string. |

Supported specifiers: `%d` / `%i` (integer), `%f` (float, 6 decimals), `%s`
(string), `%v` (any value, uses its display form), `%%` (literal percent).

```go
groot.Log(fmt.Sprintf("player pos=(%.1f,%.1f) hp=%d", x, y, hp))
```

`%f` uses six decimals — GoScript has no width/precision specifiers yet.

## Package `rand`

| Function | Description |
| --- | --- |
| `rand.Float() -> float64` | Uniform random in `[0.0, 1.0)`. |
| `rand.Intn(n) -> int` | Uniform random in `[0, n)`. |

Backed by a fast xorshift64\* generator seeded from the clock. It is **not**
cryptographically secure and is not reproducible across runs — do not use it
for anything that must be deterministic.

## Package `time`

| Function | Description |
| --- | --- |
| `time.Delta() -> float64` | The current frame's delta time in seconds (set by the host). |
| `time.Now() -> float64` | Monotonic engine runtime in seconds. |

`time.Delta()` is the frame delta the engine pushed before ticking scripts, so
every script in a frame sees the same value.

## Package `strings`

| Function | Description |
| --- | --- |
| `strings.Contains(s, sub) -> bool` | Substring test |
| `strings.ToLower(s) -> string` | Lowercase |
| `strings.ToUpper(s) -> string` | Uppercase |
| `strings.HasPrefix(s, prefix) -> bool` | Prefix test |
| `strings.HasSuffix(s, suffix) -> bool` | Suffix test |
| `strings.Trim(s, cutset) -> string` | Trim characters from both ends (`strings.Trim(s)` trims whitespace) |
| `strings.Replace(s, old, new, n) -> string` | Replace up to `n` occurrences (all of them if `n` is omitted) |
| `strings.Split(s, sep) -> []string` | Split into a slice |
| `strings.Join(slice, sep) -> string` | Join a slice of strings |

```go
var parts = strings.Split("a,b,c", ",")   // ["a", "b", "c"]
var csv = strings.Join(parts, ";")        // "a;b;c"
```

## Engine packages

Beyond the standard library, Groot registers its own package. It is
documented separately:

* [`05-groot-host-api.md`](05-groot-host-api.md) — `groot.*`: entity context,
  transforms, camera, spawning, collisions, logging.

## Adding a function

From Rust, a host function is a closure that takes `Vec<Value>` and returns a
`Value`:

```rust
vm.register_fn("groot.Spawn", |args| {
    let kind = args.first().and_then(|v| v.as_string()).unwrap_or("").to_string();
    // ... do the work ...
    Value::Nil
});
```

* Dotted names automatically declare a package, so `groot.Spawn(...)` parses
  as a package call rather than a struct field access.
* Closures may capture state (`Rc<dyn Fn>` is used, not a bare `fn`
  pointer) — that is how the engine captures the current input snapshot.
* Functions receive their arguments in order and must return a `Value`.
  Returning `Value::Nil` is the usual "no result".
