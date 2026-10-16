# Class diagrams

```mermaid
classDiagram
  class Shape {
    +area()
  }
  class Box {
    +double width
    +double height
  }
  Shape <|-- Box
  Box --> Shape : recomputes
```
