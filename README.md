# flag-cell

[English README](README_en.md)

Apache-2.0 许可证

一个用于对值进行**轻量引用 + 逻辑启用/禁用**管理的 Rust 轻量库。提供 `FlagCell`（值持有者）与 `FlagRef`（轻量共享引用）两大核心类型，并在不违背内存安全与 Rust 借用规则的前提下，实现逻辑启用/禁用与引用计数校验。

> 本仓库当前仅实现单线程版本（`src/local.rs`）；`src/sync.rs` 待实现。

## 主要特性

- 轻量共享引用：类似 `Rc<RefCell>` 但具备**逻辑启用/禁用**语义，仅保留单个所有权持有者（`FlagCell`），其余引用均为弱引用（`FlagRef`）
- 可对托管数据执行临时「逻辑禁用」，阻止引用读取内部数据
- `FlagCell` 被释放时会自动禁用，且可被任意 `FlagRef` 重新复活（仅在其被释放后）
- 支持安全解包 `FlagCell`
- 体积小巧

## 适用场景

- 需要以「软锁/软禁用」方式限制数据访问（例如：逻辑上挂起某资源，但不立即释放内存）
- 需要在单所有权模型下感知所有者是否存活

## 快速开始

将本库作为依赖引入，用法与普通 Rust 库一致：

在 `Cargo.toml` 中添加：
```toml
[dependencies]
flag-cell = "0.0.2"
```

或使用指令：
```bash
crago add flag-cell
```

随后在代码中使用：
```rust
use flag_cell::*;

fn main() {
    // 创建 FlagCell 持有目标值
    let cell = FlagCell::new(String::from("hello"));

    // 从 FlagCell 生成轻量引用（FlagRef）
    let flag_ref = cell.flag_borrow();

    // 查询引用计数与启用状态
    println!("ref_count: {}", flag_ref.ref_count());
    println!("is_enabled: {}", flag_ref.is_enabled());

    // 强制启用（危险操作，需 unsafe）
    // unsafe { flag_ref.enable(); } // 返回 FlagRefOption<()>

    // 当所有 FlagRef 被释放（引用计数为 0 且未被禁用）时，可尝试取出内部值
    // 若存在活跃引用或已被禁用，try_unwrap 会返回 Err(self)
    match cell.try_unwrap() {
        Ok(value) => println!("取到值: {}", value),
        Err(_cell) => println!("存在活跃引用或已被禁用，无法解包"),
    }

    // 注意：调用 unwrap() 时若存在活跃引用或已被禁用，会直接 panic
}
```

## 主要 API 概览

- crate 根导出类型
    - `FlagCell<T>`：持有值的主体类型。核心方法（节选）：
        - `FlagCell::new(value: T) -> FlagCell<T>`
        - `flag_borrow(&self) -> FlagRef<T>`：生成一个 `FlagRef`
        - `ref_count(&self) -> isize`：返回当前引用数量（实现中会减去自身计数，语义详见源码）
        - `is_enabled(&self) -> bool`
        - `enable(&self) -> Option<()>` / `disable(&self) -> Option<()>`
        - `try_unwrap(self) -> Result<T, Self>`：非 panic 版本的内部值取出方法
        - `unwrap(self) -> T`：若存在活跃引用或已被禁用会触发 panic

    - `FlagRef<T>`：由 `FlagCell` 生成的轻量引用（可 Clone）。核心方法（节选）：
        - `ref_count(&self) -> isize`：返回引用计数（实现层对计数的解读与 `FlagCell` 略有差异，详见源码）
        - `is_enabled(&self) -> bool`
        - `unsafe fn enable(&self) -> FlagRefOption<()>`：强制逻辑启用数据（逻辑不安全）

    - `FlagRefOption<T>`：枚举，表示引用读取结果的状态：
        - `Some(T)`、`Conflict`、`Empty`、`Disabled`
        - 实现了 `FlagRefOption<T>` 到 `Option<T>` 的转换

说明：以上 API 概览均摘录自当前 `src/local.rs` 实现。如需更详细的方法签名与行为（如 panic 条件、并发安全约定），请查阅源码注释。

## 设计与注意事项（源自源码核心说明）

- `FlagCell::unwrap()` 在存在任意活跃 `FlagRef`（ref_count > 0）或已被禁用时会触发 panic。
- `try_unwrap()` 提供非 panic 替代方案，返回 `Err(self)` 交由调用方处理。
- `FlagRef` 提供 `unsafe fn enable()` 方法：属于**逻辑不安全**操作（不会产生内存未定义行为，但可能破坏类型的逻辑契约），需谨慎使用。
- 析构行为：`FlagCell` 与 `FlagRef` 的析构逻辑在语义上互斥，源码中使用 `ManuallyDrop`、`RefCell`、`Cell<isize>` 等原语做手工内存管理。

## 示例与调试

仓库源码（`src/local.rs`）包含大量注释与实现细节，建议阅读以理解以下关键点：

- 引用计数的记录方式（正负值分别表示启用/禁用状态）
- `FlagRefOption` 的各类返回状态及其与 `Option` 的转换规则
- `try_unwrap` 与 `unwrap` 在不同条件下的行为差异

## TODO / 未来规划

- 实现多线程/同步版本（`sync.rs`）并完善测试
- 补充更多示例与文档

## 贡献

欢迎提交 PR 与 Issue。

## 联系

详见仓库所有者主页 [Milk-COCO](https://github.com/Milk-COCO/)