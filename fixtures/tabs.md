# Tabs

A tab inside a fence is a tab stop rather than a character: it carries the pen to
the next column that is a multiple of four, counted from the start of the line. The
three lines below were lined up with tabs, so their last letter is in one column,
and the report says so -- the three lines end at the same `right`.

```text
a	b
ab	b
abc	b
```

A second tab on a line stops at the next multiple of four again, and a tab after a
wide character counts characters rather than pixels:

```text
a	b	c
中	end
```

An indented block is a fence too, and its tabs stop the same way:

    x	y
    xy	y
