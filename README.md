# flag-cell

[English README](README_en.md)

Apache-2.0 Licensed

一个用于对值进行“轻量引用 + 逻辑启用/禁用”管理的 Rust 小库。提供 FlagCell（持有者）和 FlagRef（轻量共享引用）两类主要类型，并且实现了在不破坏内存安全、Rust借用规则前提下的逻辑禁用/启用与引用计数检查。

> 本仓库当前仅实现单线程版本（`src/local.rs`）；`src/sync.rs` TODO。

## 主要特性

- 轻量共享引用（类似于 Rc<RefCell> 但带“逻辑禁用/启用”语义，且仅存在单个Rc（`FlagCell`），其余引用皆为Weak（`FlagRef`））
- 可以临时“禁用”被持有的数据，阻止引用读取内部数据
- `FlagCell`被释放时将自动禁用，且`FlagCell`可被任意`FlagRef`复活（仅它**死了之后**）
- 支持安全解包`FlagCell`。
- 小巧。

## 何时使用

- 需要以“软锁/软禁用”方式阻止访问数据（例如：逻辑上挂起某个资源，但不立即释放内存）
- 需要单所有权时得知所有者是否存在

## 快速开始

将本库作为依赖（仓库暂未发布到 crates.io，可使用 git 依赖）：

在 `Cargo.toml` 中添加（示例）：
```toml
[dependencies]
flag-cell = { git = "https://github.com/Milk-COCO/flag-cell" }
```

然后在代码中使用：
```rust
use flag_cell::*;

fn main() {
    // 创建一个 FlagCell 持有一个值
    let cell = FlagCell::new(String::from("hello"));

    // 从 FlagCell 生成一个轻量引用（FlagRef）
    let flag_ref = cell.flag_borrow();

    // 查询引用计数与启用状态
    println!("ref_count (不包含 FlagCell 自身): {}", flag_ref.ref_count());
    println!("is_enabled: {}", flag_ref.is_enabled());

    // 想要强制启用（危险，需 unsafe）
    // unsafe { flag_ref.enable(); } // 返回 FlagRefOption<()>

    // 当所有 FlagRef 被 drop（引用计数为 0 且未被禁用）时，可以尝试取出内部值
    // 如果存在引用或已经被禁用，try_unwrap 会返回 Err(self)
    match cell.try_unwrap() {
        Ok(value) => println!("取到值: {}", value),
        Err(_cell) => println!("存在活动引用或被禁用，无法解包"),
    }

    // 注意：调用 unwrap() 在存在活动引用或被禁用的情况下会 panic
}
```

## 主要 API 概览

- 导出（crate 根）
  - `FlagCell<T>`：持有值的主类型。主要方法（节选）：
    - `FlagCell::new(value: T) -> FlagCell<T>`
    - `flag_borrow(&self) -> FlagRef<T>`：生成一个 `FlagRef`
    - `ref_count(&self) -> isize`：返回当前引用数量（实现上会减去自身，语义请参阅源码）
    - `is_enabled(&self) -> bool`
    - `enable(&self) -> Option<()>` / `disable(&self) -> Option<()>`
    - `try_unwrap(self) -> Result<T, Self>`：非 panic 版本的取出内部值
    - `unwrap(self) -> T`：若存在引用或已禁用会 panic

  - `FlagRef<T>`：由 `FlagCell` 产生的轻量引用（可 Clone）。主要方法（节选）：
    - `ref_count(&self) -> isize`：返回引用计数（实现上对计数的解释与 FlagCell 略有差别，请参见源码）
    - `is_enabled(&self) -> bool`
    - `unsafe fn enable(&self) -> FlagRefOption<()>`：强制将数据逻辑启用（逻辑不安全）

  - `FlagRefOption<T>`：枚举表征引用读取结果的状态：
    - `Some(T)`、`Conflict`、`Empty`、`Disabled`
    - 实现了从 `FlagRefOption<T>` 到 `Option<T>` 的转换

注意：以上 API 概览基于当前 `src/local.rs` 的实现摘录。若需更详细的每个方法签名与行为（例如 panic 条件与并发安全假设），请查阅源码文档注释。

## 设计与注意事项（来自源码的重要点）

- `FlagCell::unwrap()` 会在存在任何活动 `FlagRef`（ref_count > 0）或被禁用时 panic。
- `try_unwrap()` 提供了非 panic 的替代，返回 `Err(self)` 以便调用者处理。
- `FlagRef` 提供了一个 `unsafe fn enable()` 方法：这是一个“逻辑不安全”操作（不会造成本地内存未定义行为，但可能破坏类型的逻辑契约），需谨慎使用。
- `Drop` 行为：`FlagCell` 与 `FlagRef` 的 drop 在语义上互斥，源码中有专门处理内存释放的逻辑（使用 `ManuallyDrop`、`RefCell`、`Cell<isize>` 等原语进行手工管理）。

## 示例与调试

仓库源码（`src/local.rs`）包含了大量注释与实现细节，建议阅读以理解下列重要点：

- 引用计数如何记录（正负值处理表示启用/禁用状态）
- `FlagRefOption` 的几种返回状态以及如何向 `Option` 转换
- `try_unwrap` 与 `unwrap` 在不同条件下的行为差异

## TODO / 未来工作

- 添加多线程/同步版本（`sync.rs`）实现并完善测试
- 发布到 crates.io（目前可通过 git 依赖使用）
- 增加示例与文档

## 贡献

欢迎 PR 与 Issue。建议流程：
1. Fork → 新分支（feature/xxx 或 fix/xxx）
2. 添加测试与实现
3. 提交 PR 并在描述中附上复现步骤或测试用例

## 联系

详见仓库所有者首页 [Milk-COCO](https://github.com/Milk-COCO/)
