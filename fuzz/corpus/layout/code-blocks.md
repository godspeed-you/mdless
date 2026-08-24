P# Code Blocks

## Rust

```rust
fn main() {
    let greeting = "hello";
    println!("{greeting}, world");
}
```

## Shell

```sh
#!/bin/sh
set -eu
for f in *.md; do
    wc -l "$f"
done
```

## Python

```python
def fib(n: int) -> int:
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a
```

## No language

```
plain preformatted text
  with indentation preserved
```

## Indented code block

    indented code
    second line

## Long lines

```text
this is an extremely long line of code that will certainly not fit into an eighty column terminal and therefore exercises horizontal scrolling or wrapping behavior 1234567890
```

## Inline

Use `cargo build --release` to compile, and `rustup update` to update.

	tab-indented code line
