use std::alloc::{dealloc, Layout};
use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::ptr::{drop_in_place, NonNull};

macro_rules! dangling_then_return {
    ($ptr:expr , $thing:expr) => {
        if is_dangling($ptr) {
            return $thing;
        }
    };
}

pub fn is_dangling<T: ?Sized>(ptr: *const T) -> bool {
    (ptr.cast::<()>()).addr() == usize::MAX
}

#[repr(transparent)]
#[derive(Debug)]
struct InnerFlag<T>(NonNull<(RefCell<T>, Cell<isize>)>);

// 不可能创建一个空的自己，不作null校验
// 在内存被 dealloc 后，正常使用情况下应当不存在可能的InnerFlag被持有，当InnerFlag存在时，内存应当始终有效，因此不作任何判悬垂校验
// TODO：引用计数理论上可以达到 isize::MAX，但应该不太可能有人做得到，暂时不写溢出检查，直接panic
impl<T> InnerFlag<T> {
    /// 从合法指针创建InnerFlag
    pub fn from_ptr(ptr: NonNull<(RefCell<T>, Cell<isize>)>) -> Self {
        Self(ptr)
    }
    
    /// 获取计数的引用
    ///
    /// 外部应当永远不会调用到此方法
    #[inline]
    pub fn count_ref(&self) -> &Cell<isize> {
        // SAFETY: 仅当指针非空时调用，外部已做is_empty校验，指针必合法
        unsafe { &self.0.as_ref().1 }
    }
    
    /// 获取计数的裸指针
    ///
    /// 外部应当永远不会调用到此方法
    #[inline]
    pub unsafe fn count_ptr_unchecked(&self) -> *const Cell<isize> {
        self.count_ref() as *const _
    }
    
    /// 获取FlagRef数量
    #[inline]
    pub fn ref_count(&self) -> isize {
        self.count_ref().get().abs()
    }
    
    /// 获取当前是否逻辑可用
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.count_ref().get().is_positive()
    }
    
    /// 使引用数量加一，返回当前数量
    ///
    /// 外部应当永远不会调用到此方法
    ///
    /// # Panics
    /// 计数溢出时 panic
    pub fn inc_ref_count(&self) -> isize  {
        let cell = self.count_ref();
        let val = cell.get();
        if val == isize::MAX || val == isize::MIN + 1 {
            panic!("Flag 计数溢出，最大允许 {}",isize::MAX);
        };
        // 不用判断0，因为0时数据会被销毁，从而永远不可能在0时调用该方法
        debug_assert_ne!(val, 0);
        let new_val = if val > 0 {val + 1} else {val - 1};
        cell.set(new_val);
        new_val
    }
    
    /// 使引用数量减一，返回当前数量
    ///
    /// 外部应当永远不会调用到此方法
    ///
    /// # Panics
    /// 计数==0 时 panic
    pub fn dec_ref_count(&self) -> isize {
        let cell = self.count_ref();
        let val = cell.get();
        if val == 0 {
            panic!("Flag 计数为0时递减计数");
        }
        debug_assert_ne!(val, 0);
        let new_val = if val > 0 {val - 1} else {val + 1};
        cell.set(new_val);
        new_val
    }
    
    pub fn enable(&self) -> Option<()>{
        let cell = self.count_ref();
        let val = cell.get();
        if val.is_positive() {
            None
        } else {
            cell.set(-val);
            Some(())
        }
    }
    
    pub fn disable(&self) -> Option<()>{
        let cell = self.count_ref();
        let val = cell.get();
        if val.is_positive() {
            cell.set(-val);
            Some(())
        } else {
            None
        }
    }
    
    /// 获取内部RefCell的只读引用
    #[inline]
    pub unsafe fn as_ref_unchecked(&self) -> &RefCell<T> {
        // SAFETY: 调用者必须保证指针非空+内存未释放
        unsafe { &self.0.as_ref().0 }
    }
    
    /// 获取内部RefCell的裸指针
    #[inline]
    pub unsafe fn as_ptr_unchecked(&self) -> *const RefCell<T> {
        unsafe { self.as_ref_unchecked() as *const _ }
    }
    
    /// 获取内部核心指针
    #[inline]
    pub fn inner_ptr(&self) -> NonNull<(RefCell<T>, Cell<isize>)> {
        self.0
    }
}

/// 带标记+引用计数+内部可变性的智能容器
/// 逻辑上是唯一所有权持有者，逻辑禁用后可通过FlagRef::resurrect复活
///
/// 确保在安全使用时，Cell存在即内部数据存在。
/// 正常使用时，逻辑上是不会有人再访问已经释放的数据的，因为确保访问者死完了数据才会释放。
#[repr(transparent)]
#[derive(Debug)]
pub struct FlagCell<T>(InnerFlag<T>);

impl<T> FlagCell<T> {
    fn from_inner(ptr: NonNull<(RefCell<T>, Cell<isize>)>) -> Self {
        Self(InnerFlag(ptr))
    }
    
    pub fn ref_count(&self) -> isize {
        dangling_then_return!(self.0.inner_ptr().as_ptr(),0);
        // 减去自己
        self.0.ref_count() - 1
    }
    
    pub fn is_enabled(&self) -> bool {
        self.0.is_enabled()
    }
    
    pub fn enable(&self) -> Option<()> {
        self.0.enable()
    }
    
    pub fn disable(&self) -> Option<()> {
        self.0.disable()
    }
    
    pub fn try_borrow(&self) -> Option<std::cell::Ref<'_, T>> {
        self.deref().try_borrow().ok()
    }
    
    pub fn try_borrow_mut(&self) -> Option<std::cell::RefMut<'_, T>> {
        self.deref().try_borrow_mut().ok()
    }
    
    pub fn new(value: T) -> Self {
        // 对标 std::rc，leak 解放堆内存生命周期，手动管理释放
        Self::from_inner(
            NonNull::from(
                Box::leak(Box::new(
                    (RefCell::new(value), Cell::new(1)
                    )
                ))
            )
        )
    }
    
    pub fn as_ref(&self) -> &RefCell<T> {
        // SAFETY：确保正常使用时，FlagCell 存在即数据存在
        unsafe { self.0.as_ref_unchecked() }
    }
    
    pub fn as_ptr(&self) -> *const RefCell<T> {
        // SAFETY：确保正常使用时，FlagCell 存在即数据存在
        unsafe { self.0.as_ptr_unchecked() }
    }
    
    pub fn borrow(&self) -> FlagRef<T> {
        let ref_flag = FlagRef(InnerFlag(self.0.inner_ptr()));
        ref_flag.0.inc_ref_count();
        ref_flag
    }
}

impl<T> Drop for FlagCell<T> {
    // 这drop与FlagRef的drop严格互斥
    fn drop(&mut self) {
        
        let ptr = self.0.inner_ptr();
        
        self.disable();
        let new_count = self.0.dec_ref_count();
        
        if new_count == 0 {
            // SAFETY: 计数0=无其他引用，可以释放。
            // new_count 首次归零意味着，内存未曾释放，这是唯一释放点。
            unsafe {
                // 第一步：原地析构堆上的对象，调用所有成员的Drop
                drop_in_place(ptr.as_ptr());
                // 第二步：释放物理堆内存，把内存还给系统
                dealloc(ptr.as_ptr() as *mut u8, Layout::new::<(RefCell<T>, Cell<isize>)>());
            }
        }
    }
}

impl<T> Deref for FlagCell<T> {
    type Target = RefCell<T>;
    
    fn deref(&self) -> &Self::Target {
        // SAFETY：FlagCell存在则内存有效、指针合法
        unsafe { self.0.as_ref_unchecked() }
    }
}

// impl<T> !Send for FlagCell<T> {}
// impl<T> !Sync for FlagCell<T> {}

/// 从FlagCell产生的轻量共享引用，可Clone，单线程使用
#[repr(transparent)]
pub struct FlagRef<T>(InnerFlag<T>);

#[derive(Debug)]
pub enum FlagRefOption<T> {
    Some(T),
    Conflict,
    Empty,
    Disabled,
    Freed,
}

impl<T> FlagRefOption<T> {
    pub fn unwrap(self) -> T {
        if let FlagRefOption::Some(val) = self {
            val
        }
        else {
            panic!("called `FlagRefOption::unwrap()` on a not `Some` value")
        }
    }
}

impl<T> From<FlagRefOption<T>> for Option<T> {
    fn from(f: FlagRefOption<T>) -> Option<T> {
        match f {
            FlagRefOption::Some(v) => Some(v),
            _ => None,
        }
    }
}

impl<T> FlagRefOption<T> {
    fn from_borrow(opt: Option<T>) -> Self {
        opt.map(Self::Some).unwrap_or(Self::Conflict)
    }
}

impl<T> FlagRef<T> {
    /// 空指针实例
    /// 抄的std::rc::Weak::new()方法。
    pub const EMPTY: Self =
        Self( InnerFlag(NonNull::without_provenance(NonZeroUsize::MAX)) );
    
    pub fn ref_count(&self) -> isize {
        dangling_then_return!(self.0.inner_ptr().as_ptr(),0);
        // 减去可能存在的 FlagCell
        if self.is_enabled() { self.0.ref_count() - 1 } else { self.0.ref_count() }
    }
    
    pub fn is_enabled(&self) -> bool {
        dangling_then_return!(self.0.inner_ptr().as_ptr(),false);
        self.0.is_enabled()
    }
    
    /// 强制将数据逻辑启用
    ///
    /// # SAFETY
    /// 本方法为**逻辑不安全操作**：无内存未定义行为、无 panic 风险。
    /// 暴露此方法是为了满足特定场景的便捷性需求。
    ///
    /// 此方法会虚构出一个 `FlagCell` ，可能造成其他相关类型功能异常。
    pub unsafe fn enable(&self) -> FlagRefOption<()> {
        dangling_then_return!(self.0.inner_ptr().as_ptr(),FlagRefOption::Empty);
        self.0.enable();
        FlagRefOption::Some(())
    }
    
    /// 强制将数据逻辑禁用
    ///
    /// # SAFETY
    /// 本方法为**逻辑不安全操作**：无内存未定义行为、无 panic 风险。
    /// 暴露此方法是为了满足特定场景的便捷性需求。
    ///
    /// 此方法会强制 `RefCell` 失效，可能造成其他相关类型功能异常。
    pub unsafe fn disable(&self) -> FlagRefOption<()> {
        dangling_then_return!(self.0.inner_ptr().as_ptr(),FlagRefOption::Empty);
        self.0.disable();
        FlagRefOption::Some(())
    }
    
    pub fn try_borrow(&self) -> FlagRefOption<std::cell::Ref<'_, T>> {
        dangling_then_return!(self.0.inner_ptr().as_ptr(),FlagRefOption::Empty);
        if !self.is_enabled() {
            return FlagRefOption::Disabled;
        }
        let borrow = unsafe { self.0.as_ref_unchecked().try_borrow().ok() };
        FlagRefOption::from_borrow(borrow)
    }
    
    pub fn try_borrow_mut(&self) -> FlagRefOption<std::cell::RefMut<'_, T>> {
        dangling_then_return!(self.0.inner_ptr().as_ptr(),FlagRefOption::Empty);
        if !self.is_enabled() {
            return FlagRefOption::Disabled;
        }
        let borrow = unsafe { self.0.as_ref_unchecked().try_borrow_mut().ok() };
        FlagRefOption::from_borrow(borrow)
    }
    
    /// 尝试复活 `FlagCell`
    ///
    /// 仅当前对应 `FlagCell` 销毁即数据逻辑禁用时，可复活，否则返回 `Disabled` 。
    pub fn resurrect(&self) -> FlagRefOption<FlagCell<T>> {
        dangling_then_return!(self.0.inner_ptr().as_ptr(),FlagRefOption::Empty);
        if self.is_enabled() {
            return FlagRefOption::Disabled;
        }
        unsafe { self.enable(); }
        self.0.inc_ref_count();
        FlagRefOption::Some(FlagCell::from_inner(self.0.inner_ptr()))
    }
    
    /// 创建一个不指向任何内容的 `FlagRef`
    ///
    /// 尝试调用任何方法都将返回 `Empty`
    pub fn new() -> Self {
        Self::EMPTY
    }
}

impl<T> Drop for FlagRef<T> {
    // 与FlagCell的drop严格互斥
    fn drop(&mut self) {
        let ptr = self.0.inner_ptr();
        dangling_then_return!(ptr.as_ptr(),());
        let new_count = self.0.dec_ref_count();
        if new_count == 0 {
            // SAFETY: 计数0=Cell不存在=无其他引用，指针合法。
            // new_count 首次归零意味着，内存未曾释放，这是唯一释放点。
            unsafe {
                drop_in_place(ptr.as_ptr());
                dealloc(ptr.as_ptr() as *mut u8, Layout::new::<(RefCell<T>, Cell<isize>)>());
            }
        }
    }
}

impl<T> Clone for FlagRef<T> {
    fn clone(&self) -> Self {
        self.0.inc_ref_count();
        Self(InnerFlag(self.0.inner_ptr()))
    }
}

// impl<T> !Send for FlagRef<T> {}
// impl<T> !Sync for FlagRef<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic;
    
    /// 基础功能测试：创建FlagCell、borrow生成FlagRef、引用计数正确性
    #[test]
    fn test_basic_ref_count() {
        let cell = FlagCell::new(114514);
        // FlagCell自身初始化计数是1，对外暴露-1 → 初始引用数0
        assert_eq!(cell.ref_count(), 0);
        assert!(cell.is_enabled());
        
        // borrow生成第一个FlagRef，计数+1 → 对外显示1
        let ref1 = cell.borrow();
        assert_eq!(cell.ref_count(), 1);
        assert_eq!(ref1.ref_count(), 1);
        assert!(ref1.is_enabled());
        
        // 克隆FlagRef，计数再+1 → 对外显示2
        let ref2 = ref1.clone();
        assert_eq!(cell.ref_count(), 2);
        assert_eq!(ref1.ref_count(), 2);
        assert_eq!(ref2.ref_count(), 2);
        
        // 销毁一个FlagRef，计数-1 → 对外显示1
        drop(ref2);
        assert_eq!(cell.ref_count(), 1);
        assert_eq!(ref1.ref_count(), 1);
        
        // 销毁最后一个FlagRef，计数回归0
        drop(ref1);
        assert_eq!(cell.ref_count(), 0);
    }
    
    /// 核心测试：FlagCell销毁自动禁用+计数正确
    #[test]
    fn test_flag_cell_drop_disable() {
        let cell = FlagCell::new(String::from("test"));
        let ref1 = cell.borrow();
        let ref2 = ref1.clone();
        assert_eq!(cell.ref_count(), 2);
        assert!(ref1.is_enabled());
        
        // 销毁FlagCell → 自动disable变为禁用态，计数-1
        drop(cell);
        assert!(!ref1.is_enabled());
        assert!(!ref2.is_enabled());
        // 禁用态下，FlagRef的ref_count直接返回总计数，无需-1 → 值为2
        assert_eq!(ref1.ref_count(), 2);
        assert_eq!(ref2.ref_count(), 2);
        
        // 销毁一个FlagRef，计数-1 → 1
        drop(ref2);
        assert_eq!(ref1.ref_count(), 1);
        
        // 销毁最后一个FlagRef，计数归0 → 触发内存释放
        drop(ref1);
    }
    
    /// 核心特色测试：FlagRef.resurrect 复活 FlagCell 功能完整校验
    #[test]
    fn test_flag_ref_resurrect() {
        let cell = FlagCell::new(50);
        let ref1 = cell.borrow();
        assert_eq!(cell.ref_count(), 1);
        assert!(cell.is_enabled());
        
        // 启用态调用resurrect → 返回Disabled，复活失败（符合逻辑）
        let res = ref1.resurrect();
        assert!(matches!(res, FlagRefOption::Disabled));
        
        // 销毁FlagCell → 变为禁用态
        drop(cell);
        assert!(!ref1.is_enabled());
        assert_eq!(ref1.ref_count(), 1);
        
        // 禁用态调用resurrect → 复活成功，返回FlagCell
        let revived_cell = match ref1.resurrect() {
            FlagRefOption::Some(c) => c,
            other => panic!("resurrect失败，返回: {:?}", other),
        };
        // 复活后恢复启用态
        assert!(revived_cell.is_enabled());
        assert!(ref1.is_enabled());
        // 复活会+1计数 → 引用数保持1
        assert_eq!(revived_cell.ref_count(), 1);
        assert_eq!(ref1.ref_count(), 1);
        
        // 销毁复活的FlagCell，计数-1
        drop(revived_cell);
        assert!(!ref1.is_enabled());
    }
    
    /// 测试：enable/disable 状态切换 + 幂等性（核心设计亮点）
    #[test]
    fn test_enable_disable_idempotent() {
        let cell = FlagCell::new(vec![1,2,3]);
        assert!(cell.is_enabled());
        
        // 启用态调用disable → 返回Some(())，切换成功
        let res1 = cell.disable();
        assert_eq!(res1, Some(()));
        assert!(!cell.is_enabled());
        
        // 禁用态重复调用disable → 返回None，幂等性生效（无重复操作）
        let res2 = cell.disable();
        assert_eq!(res2, None);
        assert!(!cell.is_enabled());
        
        // 禁用态调用enable → 返回Some(())，切换成功
        let res3 = cell.enable();
        assert_eq!(res3, Some(()));
        assert!(cell.is_enabled());
        
        // 启用态重复调用enable → 返回None，幂等性生效
        let res4 = cell.enable();
        assert_eq!(res4, None);
        assert!(cell.is_enabled());
        
        // 禁用后无法borrow数据，返回Disabled
        let ref1 = cell.borrow();
        cell.disable();
        let borrow_res = ref1.try_borrow();
        assert!(matches!(borrow_res, FlagRefOption::Disabled));
        
        // 重新启用后可正常borrow
        cell.enable();
        let borrow_ok = ref1.try_borrow();
        assert!(matches!(borrow_ok, FlagRefOption::Some(_)));
    }
    
    /// 测试：RefCell 互斥借用特性（核心安全保障）
    #[test]
    fn test_try_borrow() {
        let cell = FlagCell::new(RefCell::new(200));
        let ref1 = cell.borrow();
        let ref2 = ref1.clone();
        
        // 先获取不可变借用
        let b1 = ref1.try_borrow().unwrap();
        // 再次获取不可变借用 → 成功（允许多个共享读）
        let b2 = ref2.try_borrow().unwrap();
        assert_eq!(*b1.borrow(), 200);
        assert_eq!(*b2.borrow(), 200);
        
        // 持有不可变借用时，获取可变借用 → 失败，返回Conflict
        let b_mut = ref1.try_borrow_mut();
        assert!(matches!(b_mut, FlagRefOption::Conflict));
        
        // 释放不可变借用后，可变借用成功
        drop(b1);
        drop(b2);
        let b_mut_ok = ref1.try_borrow_mut();
        assert!(matches!(b_mut_ok, FlagRefOption::Some(_)));
        *b_mut_ok.unwrap().borrow_mut() = 300;
        assert_eq!(*ref2.try_borrow().unwrap().borrow(), 300);
    }
    
    /// 测试：悬空指针 FlagRef::EMPTY 边界场景
    #[test]
    fn test_dangling_empty_flag_ref() {
        let empty = FlagRef::<i32>::EMPTY;
        // 所有方法调用都返回Empty，无panic、无内存访问错误
        assert_eq!(empty.ref_count(), 0);
        assert!(!empty.is_enabled());
        assert!(matches!(empty.try_borrow(), FlagRefOption::Empty));
        assert!(matches!(empty.try_borrow_mut(), FlagRefOption::Empty));
        assert!(matches!(empty.resurrect(), FlagRefOption::Empty));
        unsafe {
            assert!(matches!(empty.enable(), FlagRefOption::Empty));
            assert!(matches!(empty.disable(), FlagRefOption::Empty));
        }
        // 销毁空引用无任何问题
        drop(empty);
    }
    
    /// 测试：计数溢出 panic 兜底逻辑
    #[test]
    #[should_panic]
    fn test_count_overflow_panic_while_enabled() {
        let cell = FlagCell::new(());
        let inner = &cell.0;
        // 手动将计数设置为isize::MAX
        inner.count_ref().set(isize::MAX);
        // 调用inc_ref_count触发panic
        inner.inc_ref_count();
    }
    
    #[test]
    #[should_panic]
    fn test_count_overflow_panic_while_disabled() {
        let cell = FlagCell::new(());
        let inner = &cell.0;
        // 手动将计数设置为isize::MIN+1（最多允许+-isize::MAX）
        inner.count_ref().set(isize::MIN+1);
        // 调用inc_ref_count触发panic
        inner.inc_ref_count();
    }
    
    /// 测试：计数下溢 panic 兜底逻辑
    #[test]
    #[should_panic]
    fn test_count_underflow_panic() {
        let cell = FlagCell::new(());
        let inner = &cell.0;
        // 手动将计数设置为0
        inner.count_ref().set(0);
        // 调用drop，内部的dec_ref_count触发panic
        drop(cell);
    }
    
    /// 综合测试：所有引用销毁后内存正常释放（无内存泄漏/双重释放）
    #[test]
    fn test_memory_release_safe() {
        let cell = FlagCell::new(String::from("memory test"));
        let ref1 = cell.borrow();
        let ref2 = ref1.clone();
        let ref3 = ref2.clone();
        
        assert_eq!(cell.ref_count(), 3);
        drop(cell);
        assert_eq!(ref1.ref_count(), 3);
        
        drop(ref1);
        assert_eq!(ref2.ref_count(), 2);
        drop(ref2);
        assert_eq!(ref3.ref_count(), 1);
        // 销毁最后一个引用，计数归0触发释放，无任何panic
        drop(ref3);
    }
}