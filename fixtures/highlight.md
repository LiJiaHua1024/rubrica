# Highlighted words written with `==`

A pair of equals signs lays a band of colour behind the words between them:
==the marked words== come out of the reader bright, and the marks themselves
are not printed. The band follows a phrase as it wraps -- ==a pair carried on
to the next line keeps one band under every word it crosses== -- and the two
shades it is drawn in are picked from the page, so the words on it stay as
legible as the words around them.

## What a mark may touch

==A mark may head a paragraph== and nothing need stand before it; it may
follow a space (x ==b== y) or be glued to the words on either side, as in
x==y==z, 中文==高亮==中文 and ==שלום==. What the pair asks of its edges is a
real character: == a ==, ==a == and == a== all stand as typed, while spaces
inside the pair are the author's to keep -- ==two words== are one band all
the same. 标点可以紧贴在闭合标记后面（他说==对==。）.

## The first pair a scan can close is the one it takes

In ==a==b== the surplus run stays with the author and prints, while
==a== ==b== lights two bands. Comparison signs are not a mark (a == b == c
stands as typed), and a run of three or more was meant as a rule, not a
mark: where Pandoc cuts a pair out of ===three=== and ====four====, this
reader keeps the author's rule whole. A backslash-made sign pairs with
nothing either: \==this== prints its four signs, while inside a pair the
backslash is simply spent (==a\=b== lights `a=b`).

## A band lies behind what the words already are

A highlight is a colour, not a voice, so it wraps the styles the words already
wear without taking one away: ==*slanted*== and ==**loud**== and
==[a link](https://example.com)== keep their meaning under the band, and
~~==struck and marked==~~ keeps both of its own. A formula or a script is
wrapped as the one unit it is: ==$x^2$== and ==x^2^== both wear the band.

## Not every page reads this way

An inline `x==y==` is code and keeps its signs, and so does a fence:

```
a == b == c and x ==a== y stay literal here
```

## Cells wear the band too

| mark | reading |
| --- | --- |
| ==table cells== | x==read== |

定义列表里的高亮
: 释义里的 ==荧光标记== 也照样亮起，行内代码 `x==y==` 则保持原样。
