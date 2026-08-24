# Wide Table

A table that is wider than most terminals.

| Identifier | Component | Type | Default value | Constraints | Description | Introduced | Deprecated |
|---|---|---|---|---|---|---|---|
| `max_connections` | server | integer | `100` | 1..=10000 | The maximum number of simultaneous client connections the server will accept before refusing new ones | 0.1.0 | no |
| `idle_timeout_seconds` | server | integer | `300` | >= 0 | How long an idle connection may stay open before the server closes it automatically | 0.2.0 | no |
| `log_format` | logging | string | `"plain"` | plain, json, logfmt | Output format used for all diagnostic log messages emitted on standard error | 0.1.0 | no |
| `enable_experimental_compression` | transport | boolean | `false` | — | Turns on the experimental zstd-based compression negotiation for peers that advertise support | 0.9.0 | yes, use `compression` |
| `compression` | transport | string | `"auto"` | auto, off, zstd, lz4 | Selects the compression codec for peer traffic; `auto` negotiates the best mutually supported codec | 1.0.0 | no |

Trailing paragraph after the wide table.
