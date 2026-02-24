# flag-cell

[中文 README](README.md)

Apache-2.0 Licensed

A lightweight Rust crate for managing values with **lightweight reference + logical enable/disable** semantics. It provides two core types: `FlagCell` (the value owner) and `FlagRef` (lightweight shared reference), and implements logical enable/disable and reference count checking without violating memory safety or Rust's borrowing rules.

> This crate currently only implements a single-threaded version (`src/local.rs`); `src/sync.rs` is to be implemented.

## Key Features

- Lightweight shared references: Similar to `Rc<RefCell>` but with **logical enable/disable** semantics. Only one owner (`FlagCell`) is kept, and all other references are weak references (`FlagRef`).
- Temporarily "logically disable" the hosted data to prevent references from accessing the inner value.
- `FlagCell` is automatically disabled when dropped, and can be resurrected by any `FlagRef` (only after it has been dropped).
- Supports safe unwrapping of `FlagCell`.
- Minimal and compact.

## When to Use

- When you need to restrict data access with a **soft lock / soft disable** (e.g., logically suspending a resource without immediately freeing memory).
- When you need to detect whether the owner is alive under a single-ownership model.

## Quick Start

Add this crate as a dependency just like a regular Rust library:

Add to `Cargo.toml` (example):
```toml
[dependencies]
flag-cell = "0.0.2"
```

Or use commend:
```bash
crago add flag-cell
```


Then use it in your code:
```rust
use flag_cell::*;

fn main() {
    // Create a FlagCell holding the value
    let cell = FlagCell::new(String::from("hello"));

    // Create a lightweight reference (FlagRef) from the FlagCell
    let flag_ref = cell.flag_borrow();

    // Query reference count and enabled status
    println!("ref_count: {}", flag_ref.ref_count());
    println!("is_enabled: {}", flag_ref.is_enabled());

    // Forcibly enable (dangerous, requires unsafe)
    // unsafe { flag_ref.enable(); } // Returns FlagRefOption<()>

    // When all FlagRefs are dropped (ref count is 0 and not disabled),
    // you can try to take the inner value.
    // If there are active references or the cell is disabled, try_unwrap returns Err(self)
    match cell.try_unwrap() {
        Ok(value) => println!("Extracted value: {}", value),
        Err(_cell) => println!("Active references exist or disabled, cannot unwrap"),
    }

    // Note: Calling unwrap() will panic if there are active references or the cell is disabled
}
```

## Main API Overview

- Exports at crate root
    - `FlagCell<T>`: The main type that owns the value. Core methods (excerpt):
        - `FlagCell::new(value: T) -> FlagCell<T>`
        - `flag_borrow(&self) -> FlagRef<T>`: Creates a `FlagRef`
        - `ref_count(&self) -> isize`: Returns current reference count (implementation subtracts self; see source for semantics)
        - `is_enabled(&self) -> bool`
        - `enable(&self) -> Option<()>` / `disable(&self) -> Option<()>`
        - `try_unwrap(self) -> Result<T, Self>`: Non-panicking method to take inner value
        - `unwrap(self) -> T`: Panics if active references exist or disabled

    - `FlagRef<T>`: Lightweight reference created by `FlagCell` (Cloneable). Core methods (excerpt):
        - `ref_count(&self) -> isize`: Returns reference count (interpretation differs slightly from `FlagCell`; see source)
        - `is_enabled(&self) -> bool`
        - `unsafe fn enable(&self) -> FlagRefOption<()>`: Forcibly enables data logically (logically unsafe)

    - `FlagRefOption<T>`: Enum representing the result state of a reference access:
        - `Some(T)`, `Conflict`, `Empty`, `Disabled`
        - Implements conversion from `FlagRefOption<T>` to `Option<T>`

Note: The above API overview is excerpted from the current implementation in `src/local.rs`. For detailed method signatures and behavior (e.g., panic conditions, concurrency safety contracts), see source code comments.

## Design & Notes (Key Points from Source)

- `FlagCell::unwrap()` panics if any active `FlagRef` exists (ref_count > 0) or the cell is disabled.
- `try_unwrap()` provides a non-panicking alternative, returning `Err(self)` for the caller to handle.
- `FlagRef` provides `unsafe fn enable()`: a **logically unsafe** operation (no memory UB, but may break the type’s logical contract). Use with caution.
- Drop behavior: Drop logic for `FlagCell` and `FlagRef` is semantically exclusive. The source uses primitives like `ManuallyDrop`, `RefCell`, `Cell<isize>` for manual memory management.

## Examples & Debugging

The source code (`src/local.rs`) includes extensive comments and implementation details. Reading it is recommended to understand:

- How reference counts are tracked (positive/negative values represent enabled/disabled states)
- The various return states of `FlagRefOption` and conversion rules to `Option`
- Behavioral differences between `try_unwrap` and `unwrap` under different conditions

## TODO / Future Work

- Implement and fully test a multithreaded / synchronized version (`sync.rs`)
- Add more examples and documentation

## Contribution

PRs and issues are welcome.

## Contact

See the repository owner profile: [Milk-COCO](https://github.com/Milk-COCO/)