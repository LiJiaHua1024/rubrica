# Languages

A fence names a language and the reader sets its words in the inks that language uses.
The report's `code` line says how many of the fences below this build reads, and names
the ones it does not -- the two belong together, because a fence that was set plain is a
fact about the build rather than about the document.

```julia
function double(x::Int)::Int
    return 2 * x  # a comment
end
```

```haskell
double :: Int -> Int
double x = 2 * x  -- a comment
```

```zig
pub fn double(x: i32) i32 {
    return 2 * x; // a comment
}
```

```solidity
pragma solidity ^0.8.0;

contract Doubler {
    function double(uint x) public pure returns (uint) {
        return 2 * x;
    }
}
```

```r
double <- function(x) {
  return(2 * x)  # a comment
}
```

```hcl
resource "aws_instance" "web" {
  count = 2
  tags = { Name = "web" }
}
```

```latex
\section{Double}
The value is $2x$ and the file is \texttt{a.tex}. % a comment
```

```wat
(module
  (func $double (param i32) (result i32)
    local.get 0
    i32.const 2
    i32.mul))
```

```tcl
proc double {x} {
    return [expr {2 * $x}]  ;# a comment
}
```

```asm
double:
    mov eax, edi
    add eax, edi
    ret
```

```fortran
function double(x) result(y)
    integer, intent(in) :: x
    integer :: y
    y = 2 * x
end function double
```

```ada
function Double (X : Integer) return Integer is
begin
   return 2 * X;
end Double;
```

```applescript
on double(x)
    return 2 * x
end double
```

```crystal
def double(x : Int32) : Int32
  2 * x # a comment
end
```

```odin
double :: proc(x: int) -> int {
	return 2 * x // a comment
}
```

```gleam
pub fn double(x: Int) -> Int {
  2 * x
}
```

```puppet
class doubler {
  $two = 2
  notify { "double": message => $two * 2 }
}
```

```groovy
def double(x) {
    return 2 * x // a comment
}
```

```verilog
module doubler(input wire [7:0] x, output wire [7:0] y);
  assign y = x << 1; // a comment
endmodule
```

```lisp
(defun double (x)
  (* 2 x)) ; a comment
```

```matlab
function y = double(x)
  y = 2 * x; % a comment
end
```

```nim
proc double(x: int): int =
  result = 2 * x # a comment
```

```v
fn double(x int) int {
	return 2 * x // a comment
}
```

```d
int double(int x) {
    return 2 * x; // a comment
}
```

```erlang
double(X) -> 2 * X. % a comment
```

```elixir
def double(x) do
  2 * x # a comment
end
```

```fsharp
let double (x: int) = 2 * x // a comment
```

```ocaml
let double x = 2 * x (* a comment *)
```

```perl
sub double {
    my ($x) = @_;
    return 2 * $x; # a comment
}
```

```awk
function double(x) { return 2 * x } # a comment
```

```nix
{ x }: x * 2 # a comment
```

```proto
message Double {
  int32 value = 1; // a comment
}
```

```graphql
type Double {
  value: Int!
}
```

```starlark
def double(x):
    return 2 * x  # a comment
```

```vim
function! Double(x)
  return 2 * a:x
endfunction
```

```kotlin
fun double(x: Int): Int = 2 * x // a comment
```

```mariadb
SELECT id, 2 * value AS doubled FROM numbers WHERE id > 0;
```

```vue
<template>
  <p>{{ 2 * x }}</p>
</template>
```

Two fences this build has not met, kept here so that the line between what it reads and
what it sets plain stays visible:

```brainfuck
++[>++<-]
```

```cobol
IDENTIFICATION DIVISION.
PROGRAM-ID. DOUBLE.
```
