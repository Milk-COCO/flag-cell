use std::alloc::{dealloc, Layout};
use std::cell::{Cell, RefCell, RefMut, Ref};
use std::mem;
use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};
use std::ptr::{drop_in_place, NonNull};

macro_rules! dangling_then_return {
    ($ptr:expr , $thing:expr) => {
        if is_dangling($ptr) {
            return $thing;
        }
    };
    ($ptr:expr) => {
        if is_dangling($ptr) {
            return;
        }
    };
}

pub fn is_dangling<T: ?Sized>(ptr: *const T) -> bool {
    ptr.cast::<()>().addr() == usize::MAX
}

#[repr(transparent)]
#[derive(Debug)]
struct InnerFlag<T>(NonNull<(RefCell<ManuallyDrop<T>>, Cell<isize>)>);

// 不可能创建一个空的自己，不作null校验
// 在内存被 dealloc 后，正常使用情况下应当不存在可能的InnerFlag被持有，当InnerFlag存在时，内存应当始终有效，因此不作任何判悬垂校验
// TODO：引用计数理论上可以达到 isize::MAX，但应该不太可能有人做得到，暂时不写溢出检查，直接panic
impl<T> InnerFlag<T> {
    /// 从合法指针创建InnerFlag
    #[allow(dead_code)]
    // TODO：允许外部得到数据引用时暴露此方法
    pub fn from_ptr(ptr: NonNull<(RefCell<ManuallyDrop<T>>, Cell<isize>)>) -> Self {
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
    #[allow(dead_code)]
    // TODO：允许外部得到数据引用时暴露此方法
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
    pub unsafe fn as_ref_unchecked(&self) -> &RefCell<ManuallyDrop<T>> {
        // SAFETY: 调用者必须保证指针非空+内存未释放
        unsafe { &self.0.as_ref().0 }
    }
    
    /// 获取内部RefCell的裸指针
    #[inline]
    pub unsafe fn as_ptr_unchecked(&self) -> *const RefCell<ManuallyDrop<T>> {
        unsafe { self.as_ref_unchecked() as *const _ }
    }
    
    /// 获取内部核心指针
    #[inline]
    pub fn inner_ptr(&self) -> NonNull<(RefCell<ManuallyDrop<T>>, Cell<isize>)> {
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
    fn from_inner(ptr: NonNull<(RefCell<ManuallyDrop<T>>, Cell<isize>)>) -> Self {
        Self(InnerFlag(ptr))
    }
    
    /// 获取当前 [`FlagRef`] 引用数量
    pub fn ref_count(&self) -> isize {
        // 减去自己
        debug_assert!(self.0.ref_count() >= 1);
        self.0.ref_count() - 1
    }
    
    /// 获取数据是否逻辑启用
    pub fn is_enabled(&self) -> bool {
        self.0.is_enabled()
    }
    
    /// 将数据逻辑启用
    pub fn enable(&self) -> Option<()> {
        self.0.enable()
    }
    
    /// 将数据逻辑禁用
    ///
    /// 这将禁止所有对应 [`FlagRef`] 使用内部数据，直到调用 [`enable`]
    pub fn disable(&self) -> Option<()> {
        self.0.disable()
    }
    
    /// Immutably borrows the wrapped value.
    ///
    /// The borrow lasts until the returned `Ref` exits scope. Multiple
    /// immutable borrows can be taken out at the same time.
    ///
    /// # Panics
    ///
    /// Panics if the value is currently mutably borrowed. For a non-panicking variant, use
    /// [`try_borrow`](#method.try_borrow).
    ///
    pub fn borrow(&self) -> Ref<'_, T> {
        Ref::map(self.deref().borrow(),|md| md.deref())
    }
    
    /// Mutably borrows the wrapped value.
    ///
    /// The borrow lasts until the returned `RefMut` or all `RefMut`s derived
    /// from it exit scope. The value cannot be borrowed while this borrow is
    /// active.
    ///
    /// # Panics
    ///
    /// Panics if the value is currently borrowed. For a non-panicking variant, use
    /// [`try_borrow_mut`](#method.try_borrow_mut).
    ///
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        RefMut::map(self.deref().borrow_mut(),|md| md.deref_mut())
    }
    
    /// Immutably borrows the wrapped value, returning an error if the value is currently mutably
    /// borrowed.
    ///
    /// The borrow lasts until the returned `Ref` exits scope. Multiple immutable borrows can be
    /// taken out at the same time.
    ///
    /// This is the non-panicking variant of [`borrow`](#method.borrow).
    ///
    pub fn try_borrow(&self) -> Option<Ref<'_, T>> {
        self.deref().try_borrow().ok().map(|r| {
            Ref::map(r, |md| md.deref()) // 解包ManuallyDrop
        })
    }
    
    /// Mutably borrows the wrapped value, returning an error if the value is currently borrowed.
    ///
    /// The borrow lasts until the returned `RefMut` or all `RefMut`s derived
    /// from it exit scope. The value cannot be borrowed while this borrow is
    /// active.
    ///
    /// This is the non-panicking variant of [`borrow_mut`](#method.borrow_mut).
    ///
    pub fn try_borrow_mut(&self) -> Option<RefMut<'_, T>> {
        self.deref().try_borrow_mut().ok().map(|r| {
            RefMut::map(r, |md| md.deref_mut()) // 解包ManuallyDrop
        })
    }
    
    /// Creates a new `FlagCell` containing `value`.
    pub fn new(value: T) -> Self {
        // 对标 std::rc，leak 解放堆内存生命周期，手动管理释放
        Self::from_inner(
            NonNull::from(
                Box::leak(Box::new(
                    (RefCell::new(ManuallyDrop::new(value)), Cell::new(1)
                    )
                ))
            )
        )
    }
    
    /// 得到内部[`RefCell`]的引用
    pub fn as_ref_cell_ref(&self) -> &RefCell<ManuallyDrop<T>> {
        // SAFETY：确保正常使用时，FlagCell 存在即数据存在
        unsafe { self.0.as_ref_unchecked() }
    }
    
    /// 得到内部[`RefCell`]的指针
    pub fn as_ref_cell_ptr(&self) -> *const RefCell<ManuallyDrop<T>> {
        // SAFETY：确保正常使用时，FlagCell 存在即数据存在
        unsafe { self.0.as_ptr_unchecked() }
    }
    
    /// 生成一个 [`FlagRef`]
    ///
    pub fn flag_borrow(&self) -> FlagRef<T> {
        let ref_flag = FlagRef(InnerFlag(self.0.inner_ptr()));
        ref_flag.0.inc_ref_count();
        ref_flag
    }
    
    
    /// Replaces the wrapped value with a new one, returning the old value,
    /// without deinitializing either one.
    ///
    /// This function corresponds to [`mem::replace`].
    ///
    /// # Panics
    ///
    /// Panics if the value is currently borrowed.
    ///
    /// For non-panicking variant , see [`try_replace`](#method.try_replace).
    ///
    pub fn replace(&self, value: T) -> T {
        // SAFETY: replace返回所有权，且这个ManuallyDrop马上被丢弃
        unsafe { ManuallyDrop::take(&mut self.deref().replace(ManuallyDrop::new(value))) }
    }
    
    /// Replaces the wrapped value with a new one, returning the old value,
    /// without deinitializing either one.
    ///
    /// This function corresponds to [`mem::replace`].
    ///
    /// 如果当前存在引用，返回Err返还传入值
    ///
    /// This is the non-panicking variant of [`replace`](#method.replace).
    ///
    pub fn try_replace(&self, value: T) -> Result<T,T> {
        // SAFETY: replace返回所有权，且这个ManuallyDrop马上被丢弃
        unsafe {
            Ok(ManuallyDrop::take(
                &mut mem::replace(
                    match self.deref().try_borrow_mut() {
                        Ok(v) => {
                            v
                        }
                        Err(_) => {return Err(value)}
                    }.deref_mut(),
                    ManuallyDrop::new(value)
                )
            ))
        }
    }
    
    /// 消费自身，返回内部数据，同时禁用
    ///
    /// # Panics
    /// 若当前存在任何引用（包括FlagRef），或被异常禁用，panic。
    ///
    /// For non-panicking variant , see [`try_unwrap`](#method.try_borrow).
    ///
    pub fn unwrap(self) -> T {
        let ref_count = self.ref_count();
        if ref_count > 0 {
            panic!(
                "called `FlagCell::unwrap()` on a value with active FlagRef references (ref_count = {})",
                ref_count
            );
        }
        
        if !self.is_enabled() {
            panic!("called `FlagCell::unwrap()` on a disabled FlagCell");
        }
        
        let mut rm = self.as_ref_cell_ref().borrow_mut();
        self.disable();
        unsafe {
            ManuallyDrop::take(rm.deref_mut())
        }
        // self 将在此处被drop。
    }
    
    /// 消费自身，返回内部数据，同时禁用
    ///
    /// 若当前存在任何引用（包括FlagRef），或被异常禁用，返还Self
    ///
    /// This is the non-panicking variant of [`unwrap`](#method.unwrap).
    ///
    pub fn try_unwrap(self) -> Result<T, Self> {
        let ref_count = self.ref_count();
        if !self.is_enabled() || ref_count > 0 {
            return Err(self);
        }
        
        let r = self.as_ref_cell_ref().try_borrow_mut();
        let mut rm = match r {
            Ok(ref_mut) => {ref_mut}
            Err(_) => {
                // 如果不在此分支内drop r，编译器会认为 r 会活得更久，从而拒绝给出 self
                // 很奇葩，我都return了他还活个啥？
                drop(r);
                return Err(self);
            }
        };
        self.disable();
        unsafe {
            Ok(ManuallyDrop::take(rm.deref_mut()))
        }
        // self 将在此处被drop。
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
                // 修复：先手动析构ManuallyDrop包裹的T，再析构外层结构
                let refcell = &mut (*ptr.as_ptr()).0;
                let mut_man_drop = RefCell::get_mut(refcell);
                ManuallyDrop::drop(mut_man_drop);
                
                // 析构剩余结构 + 释放内存
                drop_in_place(ptr.as_ptr());
                dealloc(
                    ptr.as_ptr() as *mut u8,
                    Layout::new::<(RefCell<ManuallyDrop<T>>, Cell<isize>)>()
                );
            }
        }
    }
}

impl<T> Deref for FlagCell<T> {
    type Target = RefCell<ManuallyDrop<T>>;
    
    fn deref(&self) -> &Self::Target {
        // SAFETY：FlagCell存在则内存有效、指针合法
        unsafe { self.0.as_ref_unchecked() }
    }
}

// impl<T> !Send for FlagCell<T> {}
// impl<T> !Sync for FlagCell<T> {}

/// 从FlagCell产生的轻量共享引用，可Clone，单线程使用
#[repr(transparent)]
#[derive(Debug)]
pub struct FlagRef<T>(InnerFlag<T>);

/// Some: 可借用 <br>
/// Conflict: 借用冲突，不符合rust借用原则
/// Empty: 内部为空，即此FlagRef是从new函数创建的
/// Disabled: 内部数据当前已禁用
#[derive(Debug)]
pub enum FlagRefOption<T> {
    Some(T),
    Conflict,
    Empty,
    Disabled,
}

impl<T> FlagRefOption<T> {
    /// 解包 FlagRefOption
    ///
    /// # Panics
    /// 若非 `Some` ，panic
    pub fn unwrap(self) -> T {
        if let FlagRefOption::Some(val) = self {
            val
        }
        else {
            panic!("called `FlagRefOption::unwrap()` on a not `Some` value")
        }
    }
    
    /// 将自己转换为原生 `Option` 类型
    ///
    /// Some转换为Some，其余全部转换为None
    pub fn into_option(self) -> Option<T> {
        self.into()
    }
    
    /// Maps an `FlagRefOption<T>` to `FlagRefOption<U>` by applying a function to a contained value (为`Some`) or returns 原变体 (非`Some`).
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> FlagRefOption<U> {
        match self{
            FlagRefOption::Some(v) => FlagRefOption::Some(f(v)),
            FlagRefOption::Conflict => FlagRefOption::Conflict,
            FlagRefOption::Empty => FlagRefOption::Empty,
            FlagRefOption::Disabled => FlagRefOption::Disabled,
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
    // 抄的std::rc::Weak::new()方法。
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
    
    /// 尝试借用内部值。
    ///
    /// 详见 [`FlagRefOption`]
    pub fn try_borrow(&self) -> FlagRefOption<Ref<'_, T>> {
        dangling_then_return!(self.0.inner_ptr().as_ptr(), FlagRefOption::Empty);
        if !self.is_enabled() {
            return FlagRefOption::Disabled;
        }
        let borrow = unsafe { self.0.as_ref_unchecked().try_borrow().ok() };
        // 解包ManuallyDrop<T> → T
        let borrow_unwrapped = borrow.map(|r| Ref::map(r, |md| md.deref()));
        FlagRefOption::from_borrow(borrow_unwrapped)
    }
    
    /// 尝试可变借用内部值。
    ///
    /// 详见 [`FlagRefOption`]
    pub fn try_borrow_mut(&self) -> FlagRefOption<RefMut<'_, T>> {
        dangling_then_return!(self.0.inner_ptr().as_ptr(), FlagRefOption::Empty);
        if !self.is_enabled() {
            return FlagRefOption::Disabled;
        }
        let borrow = unsafe { self.0.as_ref_unchecked().try_borrow_mut().ok() };
        // 解包ManuallyDrop<T> → T
        let borrow_unwrapped = borrow.map(|r| RefMut::map(r, |md| md.deref_mut()));
        FlagRefOption::from_borrow(borrow_unwrapped)
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

impl<T> Default for FlagRef<T>{
    /// 创建一个不指向任何内容的 `FlagRef`
    ///
    /// 尝试调用任何方法都将返回 `Empty`
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for FlagRef<T> {
    // 与FlagCell的drop严格互斥
    fn drop(&mut self) {
        let ptr = self.0.inner_ptr();
        dangling_then_return!(ptr.as_ptr());
        
        let new_count = self.0.dec_ref_count();
        if new_count == 0 {
            // SAFETY: 计数0=Cell不存在=无其他引用，指针合法。
            // new_count 首次归零意味着，内存未曾释放，这是唯一释放点。
            unsafe {
                // 修复：先手动析构ManuallyDrop包裹的T，再析构外层结构
                let refcell = &mut (*ptr.as_ptr()).0;
                let mut_man_drop = RefCell::get_mut(refcell);
                ManuallyDrop::drop(mut_man_drop);
                
                // 析构剩余结构 + 释放内存
                drop_in_place(ptr.as_ptr());
                dealloc(
                    ptr.as_ptr() as *mut u8,
                    Layout::new::<(RefCell<ManuallyDrop<T>>, Cell<isize>)>()
                );
            }
        }
    }
}

impl<T> Clone for FlagRef<T> {
    /// 克隆一个 FlagRef，使引用计数加一
    fn clone(&self) -> Self {
        self.0.inc_ref_count();
        Self(InnerFlag(self.0.inner_ptr()))
    }
}

// impl<T> !Send for FlagRef<T> {}
// impl<T> !Sync for FlagRef<T> {}
