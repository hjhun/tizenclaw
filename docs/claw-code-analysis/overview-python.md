# Python Overview

The Python workspace lives under `src/` and `tests/`.

## Package Layout

```text
src/
└── tizenclaw_py/
    ├── __init__.py
    ├── api.py
    ├── cli.py
    ├── plugins.py
    ├── runtime.py
    └── tools.py
```

## Purpose

- mirror Rust-facing concepts for audit and parity
- provide explanation-friendly reference code
- host lightweight contract tests that do not depend on the daemon runtime

Python is not the production runtime target.
