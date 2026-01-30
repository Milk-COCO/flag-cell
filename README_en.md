# flag-cell

A small Rust crate that provides lightweight single-threaded shared references with logical enable/disable semantics. It exposes the core types `FlagCell`, `FlagRef` and `FlagRefOption`. The implementation in this repository focuses on the single-threaded/local variant (see `src/local.rs`). A synchronized/multi-threaded variant is planned (`src/sync.rs` marked TODO).

For the Chinese documentation, see [`README.md`](./README.md).

---

## Features

- Lightweight, cloneable reference type (`FlagRef`) produced from an owning `FlagCell`
- Logical enable/disable semantics: references cannot access the value while disabled
- Borrowing API similar to `RefCell` (`borrow`, `borrow_mut`, `try_borrow`, `try_borrow_mut`)
- Both non-panicking and panicking variants for extracting the value (`try_unwrap`, `unwrap`)
- Minimal runtime dependencies

## Usage Scenarios

Use `flag-cell` when you need lightweight, single-threaded shared references that can be temporarily disabled for access. Suitable for resource lifecycle control, temporary logical "shutdowns", or as a single-threaded Rc/Weak replacement that can be disabled and re-enabled.

## Getting Started

Add this crate as a git dependency (until published on crates.io):

```toml
[dependencies]
flag-cell = { git = "https://github.com/Milk-COCO/flag-cell" }
```

Example usage:

```rust
use flag_cell::{FlagCell, FlagRef, FlagRefOption};

fn main() {
    let cell = FlagCell::new(String::from("hello"));
    let flag_ref = cell.flag_borrow();
    println!("Reference count: {}",&flag_ref.ref_count());
    println!("Enabled: {}",&flag_ref.is_enabled());

    if let Some(borrowed) = cell.try_borrow() {
        println!("Borrowed: {}",&*borrowed);
    }

    match cell.try_unwrap() {
        Ok(value) => println!("Unwrapped value: {}", value),
        Err(_) => println!("Cannot unwrap: cell is disabled or there are active references"),
    }

    // Note: unwrap() will panic if there are active references or if disabled.
}
```

## API Overview

- `FlagCell<T>`:
  - `new(value: T)` — Create a new owner cell
  - `flag_borrow(&self) -> FlagRef<T>` — Create a lightweight reference
  - `is_enabled()`, `enable()`, `disable()` — Logical enable/disable
  - `borrow`, `borrow_mut`, `try_borrow`, `try_borrow_mut` — Access value (like RefCell)
  - `try_unwrap`, `unwrap` — Extract value with/without panicking

- `FlagRef<T>`:
  - Cloneable lightweight reference
  - `ref_count`, `is_enabled`
  - `try_borrow`, `try_borrow_mut`
  - Logical (unsafe) enable/disable

- `FlagRefOption<T>`:
  - Variants: `Some(T)`, `Conflict`, `Empty`, `Disabled`
  - Converts to `Option<T>`

## Safety & Notes

- This crate targets single-threaded use only (see `src/local.rs`).
- Methods like `FlagRef::enable` are `unsafe` and only change logical state, but can break internal invariants if misused. Know what you're doing before using these strong operations.
- `unwrap()` panics on active references or when cell is disabled; use `try_unwrap()` for safe extraction.
- For details on memory management, dropping, and design comments, see the well-commented code in `src/local.rs`.

## Contribution

Contributions and issues are welcome! Typical workflow:
- Fork this repository and create a new branch
- Implement your feature or fix and test it
- Open a pull request describing your change

## License

Apache License 2.0. See the `LICENSE` file for details.