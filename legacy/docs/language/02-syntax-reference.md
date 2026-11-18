# 02 — Syntax Reference

Every construct GoScript supports, with the semantics the VM actually
implements.

## Comments

```go
// Line comment — the common case.

/*
   Block comment.
   Can span lines.
*/
```

## Declarations

### Variables

```go
var hp = 100              // type inferred
var hp int = 100          // annotation accepted and ignored
var hp float64 = 100.0    // …also ignored
var name = "bird"        // strings are double-quoted only
var alive = true
var nothing = nil
```

`var` inside a function body declares a **local**; at file scope it declares a
**global**. Locals live in pre-allocated stack slots, globals live in the VM's
global table.

### Short declaration

```go
hp := 100                 // sugar for: var hp = 100
```

`:=` desugars to a `var` declaration and is only valid where a new name is
introduced.

### Constants

There are none — use `var` or a literal.

## Types and structs

```go
type Player struct {
    Hp   int
    Speed float64
    Name string
}
```

A struct *type* is compile-time metadata only. Struct *instances* are dynamic
field maps: the field names exist at runtime, the type name is carried along
for method dispatch and display.

Struct literals use field names:

```go
var p = Player{Hp: 100, Speed: 350.0}
```

Fields are accessed with `.` and mutated in place:

```go
p.Hp = p.Hp - 25
```

## Functions

```go
func Add(a int, b int) int {
    return a + b
}

func NoReturn() {
    // falls through, returns nil
}
```

Parameters are positional. Return values are `nil` unless a `return` provides
one. Functions are first-class *by name* — you can call them, but there are no
closures or function values yet.

### Receiver methods

```go
func (p *Player) TakeDamage(amount int) {
    p.Hp -= amount
    if p.Hp <= 0 {
        groot.DestroySelf()
    }
}
```

A method is stored as a global named `<TypeName>.<Method>` and dispatched on
the receiver's type name at call time. The receiver is the first parameter, so
`p.TakeDamage(25)` passes the receiver, then `25`.

> Method dispatch is by the runtime type name of the receiver, so a struct
> value and a pointer to it dispatch identically.

## Operators

### Arithmetic

| Operator | Meaning |
| --- | --- |
| `+` | addition; string concatenation when either side is a string |
| `-` | subtraction |
| `*` | multiplication |
| `/` | division (integer division by zero is a runtime error) |
| `%` | remainder (integer only) |

`int` and `float64` mix: `1 + 2.5` is `3.5`. Applying arithmetic to a
non-numeric value is a runtime error.

### Logical

`&&`, `||`, `!`. Logical operators short-circuit: the right-hand side is only
evaluated when the left-hand side does not already decide the result. The
result is the deciding operand's value, so `ready && value` yields `value` when
`ready` is true.

### Comparison

`==`, `!=`, `<`, `<=`, `>`, `>=`. Numbers compare numerically (`10 == 10.0` is
true). Structs compare by reference — two aliases of the same instance are
equal, two separate instances are not. Slices and maps are never equal to
anything, not even to themselves; compare their contents element by element
instead.

### Assignment sugar

```go
hp = hp - 25
hp -= 25          // sugar for the line above
hp++              // sugar for hp = hp + 1
hp--              // sugar for hp = hp - 1
```

`+=`, `-=`, `*=`, `/=` and `++`/`--` work on variables, struct fields and
slice elements. They are rejected on method calls, because the receiver would
be evaluated twice.

## Control flow

### Conditionals

```go
if hp > 0 {
    groot.Log("alive")
} else if hp == 0 {
    groot.Log("dead")
} else {
    groot.Log("overkill")
}
```

### Loops

```go
for i := 0; i < 10; i++ { }   // C-style
for hp > 0 { }                // while-style
for { }                       // infinite (guarded by the VM's budget)

for i, v := range names { }   // index + value
for i := range names { }      // index only
```

`break` exits the innermost loop, `continue` jumps to the next iteration.

### Switch

```go
switch state {
case "IDLE":
    label = "waiting"
case "ATTACK":
    label = "burning"
default:
    label = "lost"
}
```

Each case compares the switch expression against the case expression with
`==`; the first match wins, and `default` runs when nothing matched. There is
no fall-through.

### Return

```go
func Score() int {
    if over {
        return 0
    }
    return 42
}
```

## Collections

### Slices

```go
var nums = []int{1, 2, 3}     // literal
var empty = []int{}           // empty slice
nums = append(nums, 4)        // append one element
var n = len(nums)             // element count
var first = nums[0]           // index (negative indexes are an error)
```

### Slice ranges

```go
var letters = []string{"a", "b", "c", "d", "e"}

letters[1:4]   // ["b", "c", "d"]  — half-open, start inclusive, end exclusive
letters[2:]    // ["c", "d", "e"]  — to the end
letters[:3]    // ["a", "b", "c"]  — from the start
letters[:]     // whole slice
```

A range produces a **copy**, not a view: mutating the result never touches the
source. Upper bounds are clamped to the length; a negative bound or an
inverted range (`x[2:1]`) is a runtime error.

### Maps

```go
var inv = map[string]int{"potions": 3, "gold": 100}
inv["gold"] += 20
var potions = inv["potions"]
var size = len(inv)
```

Maps are string-keyed. Reading a missing key yields `nil`, not an error.

### Strings

Strings are immutable UTF-8 text.

```go
var s = "Hello, World!"
s[0]          // "H"  — indexed by character
s[7:12]       // "World" — sliced by character
```

Both index and slice operate on **characters**, not bytes, so multi-byte text
never produces a broken code point.

## Packages and imports

```go
package player                       // optional package declaration

import "assets/scripts/utils/math.gos"
import util "assets/scripts/utils/math.gos"   // alias
import (                                // grouped
    "assets/scripts/a.gos"
    "assets/scripts/b.gos"
)
```

An imported file's top-level functions and variables are namespaced under the
file's stem: `import "utils/math.gos"` gives you `math.Lerp(...)`. Inside the
imported file, its own top-level symbols are rewritten to the same dotted
names, so a module can call its own functions.

Receiver methods are **not** namespaced — `func (z *Zombie) Hit()` stays
`Zombie.Hit`.

Imports are resolved at compile time and flattened into a single chunk: there
is no module loader at runtime. Circular imports are a compile error.

## Automatic semicolon insertion

Semicolons are optional. The lexer inserts one when a line ends with a token
that can end a statement (identifier, literal, `)`, `}`, `]`, `return`,
`break`, `continue`, …). This matches Go's behaviour, so the usual Go style
works unchanged.

## Statements vs expressions

GoScript is statement-oriented: an expression is only valid where a statement
is expected (`Expr` statements discard their value). There are no closures, no
function literals and no `defer`.

## Grammar summary

```
program    := statement*
statement := varDecl | funcDecl | typeDecl | ifStmt | forStmt | switchStmt
           | returnStmt | breakStmt | continueStmt | exprStmt | block
exprStmt  := assignment | expr
assignment := target ("=" | "+=" | "-=" | "*=" | "/=") expr
           | target ("++" | "--")
target    := identifier | selector | index
expr      := orExpr
orExpr    := andExpr ("||" andExpr)*
andExpr   := equality ("&&" equality)*
equality  := comparison (("==" | "!=") comparison)*
comparison:= term (("<" | "<=" | ">" | ">=") term)*
term      := factor (("+" | "-") factor)*
factor    := unary (("*" | "/" | "%") unary)*
unary     := ("-" | "!") unary | postfix
postfix   := primary ("." ident | "[" expr "]" | "[" [expr] ":" [expr] "]")*
primary   := literal | identifier | "(" expr ")" | sliceLit | mapLit
           | structLit | call
```
