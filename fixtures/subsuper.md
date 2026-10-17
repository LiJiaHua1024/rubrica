# Scripts written with `~` and `^`

Pandoc's own marks set a word half a line up or down: H~2~O, x^2^, e~x~tra and 2^10^
carry their scripts wherever they sit in the word, and the citation mark here[^1] is
set with the same pair of numbers -- the scale and the shift are read from the formula
face's MATH table, so `H~2~O` and a footnote's own `¹` cannot drift apart.

## 中文与西文

中文~下~标 与 记号^上^标 照读：标点贴着标点也能配对，一个标记两边都能靠着一个字。
A raised word keeps its own voice: 2^*x*^ is still slanted under the raise.

## The space a script may hold

A plain space ends the reading, so x^a b^ and x~a b~ stand as they were written. The
escaped space is the one a script may hold, and it is printed as the non-breaking
space it stands for: x^a\ b^ and y~c\ d~.

## Marks the author owns

An escaped mark is a character and pairs with nothing: \~this\~ and \^that\^ print
their tildes and carets, and a lone one is printed too (A^ and B~ stay as typed).

## Not a script

A run of two or more is left exactly as it was written rather than read as Pandoc's
degenerate empty script: x^^2^^ and a~~~b~~~c stand as typed, and the writer's own
deletion is untouched by any of this (~~struck~~, and ~~a ~b~ c~~ keeps both
readings). Nor is a box wrapped: x^$a$^ keeps its marks, because a raised formula has
no run of its own to lift.

## Code is not prose

An inline `H~2~O` is code and keeps its tildes, and so does a fence:

```
H~2~O and x^2^ and ~~struck~~ stay literal here
```

[^1]: 引注号和上下标共用一套 MATH 常量：同一尺寸、同一升降，行高也照样算进升部。
