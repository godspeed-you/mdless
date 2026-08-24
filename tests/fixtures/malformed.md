# Malformed Markdown

Unclosed **bold and *italic that never end

Unclosed `inline code span

```rust
fn unterminated_fence() {

| broken | table
|---
| a | b | c | d |
missing cells above

> quote without end
>> deeper
>>>> skipped a level

[link with no target]()
[unclosed link](https://example.com

![image with no closing paren](img.png

##### Deep heading after H1

- list item
 - misaligned indent
       - way too deep

1. one
3. three (numbering gap)

[^ref-without-definition]

Text ending without newline
