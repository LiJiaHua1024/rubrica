# Entity relationships

```mermaid
erDiagram
  CUSTOMER {
    string name
  }
  ORDER {
    int id
  }
  CUSTOMER ||--o{ ORDER : places
```
