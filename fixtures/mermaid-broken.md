# Mermaid that cannot be drawn

A fence that does not parse stays a code block, with the reason above it:

```mermaid
flowchart TD
  a[oops --> b
```

And one that names a diagram this reader does not draw:

```mermaid
gantt
  title A plan
  section One
  task :a1, 2024-01-01, 30d
```

The document keeps going after both, and a readable fence after the failures
still draws:

```mermaid
flowchart LR
  still --> draws
```
