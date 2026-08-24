P# Mermaid Diagrams

## Simple flow

```mermaid
graph LR
    A --> B
    B --> C
```

## Top-down with labels

```mermaid
graph TD
    Start[Start here] --> Decide{Ready?}
    Decide -->|yes| Ship(Ship it)
    Decide -->|no| Fix((Fix bugs))
    Fix --> Decide
```

## Chains and fan-in

```mermaid
flowchart RL
    A --> B --> C
    D & E --> F
    %% a comment line
```

## Unsupported diagram type

```mermaid
sequenceDiagram
    Alice->>Bob: Hello Bob
    Bob-->>Alice: Hi Alice
```

Regular paragraph between diagrams.

## Not mermaid

```text
graph LR
    this is just a text block
```
