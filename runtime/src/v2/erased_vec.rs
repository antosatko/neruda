use std::alloc::Layout;

pub struct AnyVec {
    layout: Layout,
    data: Vec<u8>,
}

impl AnyVec {
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            data: Vec::new(),
        }
    }

    pub fn push(&mut self) {}
}
use std::{
    alloc::{alloc, dealloc, handle_alloc_error, realloc},
    ptr::{self, NonNull},
};

#[derive(Debug, Clone, Copy)]
pub struct TypeOps {
    pub layout: Layout,

    // destroy one element in place
    pub drop_fn: unsafe fn(*mut u8),

    // move one initialized element src -> dst
    // must leave src uninitialized
    pub move_fn: unsafe fn(*mut u8, *mut u8),

    pub clone_fn: Option<unsafe fn(*const u8, *mut u8)>,
}

#[derive(Debug)]
pub struct ErasedVec {
    ptr: NonNull<u8>,
    len: usize,
    cap: usize,
    ops: &'static TypeOps,
}

impl ErasedVec {
    pub fn new(ops: &'static TypeOps) -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: 0,
            cap: 0,
            ops,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    fn elem_size(&self) -> usize {
        self.ops.layout.size()
    }

    fn array_layout(&self, cap: usize) -> Layout {
        Layout::from_size_align(self.elem_size() * cap, self.ops.layout.align()).unwrap()
    }

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };

        if self.elem_size() == 0 {
            self.cap = new_cap;
            return;
        }

        unsafe {
            let new_ptr = if self.cap == 0 {
                let layout = self.array_layout(new_cap);
                let p = alloc(layout);
                if p.is_null() {
                    handle_alloc_error(layout);
                }
                p
            } else {
                let old_layout = self.array_layout(self.cap);
                let new_layout = self.array_layout(new_cap);

                let p = realloc(self.ptr.as_ptr(), old_layout, new_layout.size());

                if p.is_null() {
                    handle_alloc_error(new_layout);
                }

                p
            };

            self.ptr = NonNull::new_unchecked(new_ptr);
        }

        self.cap = new_cap;
    }

    fn slot_ptr(&self, index: usize) -> *mut u8 {
        assert!(index < self.cap);
        unsafe { self.ptr.as_ptr().add(index * self.elem_size()) }
    }

    pub unsafe fn push_raw_move(&mut self, src: *mut u8) {
        if self.len == self.cap {
            self.grow();
        }

        let dst = self.slot_ptr(self.len);

        unsafe { (self.ops.move_fn)(src, dst) };

        self.len += 1;
    }

    pub unsafe fn push_raw_copy_bytes(&mut self, src: *const u8) {
        if self.len == self.cap {
            self.grow();
        }

        let dst = self.slot_ptr(self.len);

        unsafe { ptr::copy_nonoverlapping(src, dst, self.elem_size()) };

        self.len += 1;
    }

    pub fn get_raw(&self, index: usize) -> *const u8 {
        assert!(index < self.len);
        self.slot_ptr(index)
    }

    pub fn get_raw_mut(&mut self, index: usize) -> *mut u8 {
        assert!(index < self.len);
        self.slot_ptr(index)
    }

    pub unsafe fn swap_remove_raw(&mut self, index: usize) {
        assert!(index < self.len);

        let last = self.len - 1;

        let victim = self.slot_ptr(index);

        unsafe { (self.ops.drop_fn)(victim) };

        if index != last {
            let src = self.slot_ptr(last);
            unsafe { (self.ops.move_fn)(src, victim) };
        }

        self.len -= 1;
    }

    fn push_val<T>(&mut self, val: T) {
        let mut x = std::mem::ManuallyDrop::new(val);
        unsafe {
            self.push_raw_move((&mut *x as *mut T).cast());
        }
    }

    /// Creates a new, empty ErasedVec with the same type operations and layout
    /// as the current one, but with no elements and no allocated memory.
    pub fn clear_copy(&self) -> Self {
        Self::new(self.ops)
    }
}

impl Drop for ErasedVec {
    fn drop(&mut self) {
        unsafe {
            for i in 0..self.len {
                let p = self.slot_ptr(i);
                (self.ops.drop_fn)(p);
            }

            if self.cap > 0 && self.elem_size() > 0 {
                dealloc(self.ptr.as_ptr(), self.array_layout(self.cap));
            }
        }
    }
}
impl Clone for ErasedVec {
    fn clone(&self) -> Self {
        // Ensure the underlying type was registered with a clone function
        let clone_fn = self
            .ops
            .clone_fn
            .expect("Attempted to clone an ErasedVec containing a non-cloneable type");

        let mut new_vec = ErasedVec::new(self.ops);

        if self.len == 0 {
            return new_vec;
        }

        // We allocate exactly the length needed to save memory
        new_vec.cap = self.len;

        // ZST Check: Only allocate memory if the element has a size
        if new_vec.elem_size() > 0 {
            unsafe {
                let layout = new_vec.array_layout(new_vec.cap);
                let p = alloc(layout);
                if p.is_null() {
                    handle_alloc_error(layout);
                }
                new_vec.ptr = NonNull::new_unchecked(p);
            }
        }

        // Clone elements one by one
        for i in 0..self.len {
            unsafe {
                let src = self.get_raw(i);
                let dst = new_vec.slot_ptr(i);
                clone_fn(src, dst);
            }
            // Increment length AFTER a successful clone.
            // This ensures panic-safety: if T::clone() panics, ErasedVec's Drop impl
            // will correctly clean up only the items that were successfully written.
            new_vec.len += 1;
        }

        new_vec
    }
}

impl TypeOps {
    pub fn new<T>() -> Self {
        unsafe fn drop_impl<T>(p: *mut u8) {
            unsafe { ptr::drop_in_place(p as *mut T) };
        }

        unsafe fn move_impl<T>(src: *mut u8, dst: *mut u8) {
            // move value out of src into dst
            unsafe { ptr::write(dst as *mut T, ptr::read(src as *mut T)) };
        }

        Self {
            layout: Layout::new::<T>(),
            drop_fn: drop_impl::<T>,
            move_fn: move_impl::<T>,
            clone_fn: None,
        }
    }

    pub unsafe fn from_layout(layout: Layout) -> Self {
        unsafe fn drop_impl(p: *mut u8) {}
        unsafe fn move_impl(src: *mut u8, dst: *mut u8) {}

        Self {
            layout,
            drop_fn: drop_impl,
            move_fn: move_impl,
            clone_fn: None,
        }
    }

    pub fn new_cloneable<T: Clone>() -> Self {
        unsafe fn drop_impl<T>(p: *mut u8) {
            unsafe { ptr::drop_in_place(p as *mut T) };
        }

        unsafe fn move_impl<T>(src: *mut u8, dst: *mut u8) {
            unsafe { ptr::write(dst as *mut T, ptr::read(src as *mut T)) };
        }

        // NEW: Cast the raw pointer to a reference, safely clone it via the trait,
        // and write the new instance into the destination pointer.
        unsafe fn clone_impl<T: Clone>(src: *const u8, dst: *mut u8) {
            let cloned_val = unsafe { &*(src as *const T) }.clone();
            unsafe { ptr::write(dst as *mut T, cloned_val) };
        }

        Self {
            layout: Layout::new::<T>(),
            drop_fn: drop_impl::<T>,
            move_fn: move_impl::<T>,
            clone_fn: Some(clone_impl::<T>),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn push_and_read_i32() {
        let ops = Box::leak(Box::new(TypeOps::new::<i32>()));
        let mut v = ErasedVec::new(ops);

        for i in 0..10 {
            v.push_val(i);
        }

        assert_eq!(v.len(), 10);

        for i in 0..10 {
            unsafe {
                let p = v.get_raw(i) as *const i32;
                assert_eq!(*p, i as i32);
            }
        }
    }

    #[test]
    fn grows_correctly() {
        let ops = Box::leak(Box::new(TypeOps::new::<u64>()));
        let mut v = ErasedVec::new(ops);

        for i in 0..1000 {
            v.push_val(i as u64);
        }

        assert_eq!(v.len(), 1000);

        for i in 0..1000 {
            unsafe {
                let p = v.get_raw(i) as *const u64;
                assert_eq!(*p, i as u64);
            }
        }
    }

    #[test]
    fn swap_remove_works() {
        let ops = Box::leak(Box::new(TypeOps::new::<i32>()));
        let mut v = ErasedVec::new(ops);

        for i in 0..5 {
            v.push_val(i);
        }

        unsafe {
            v.swap_remove_raw(1);
        }

        assert_eq!(v.len(), 4);

        let mut vals = vec![];
        for i in 0..v.len() {
            unsafe {
                vals.push(*(v.get_raw(i) as *const i32));
            }
        }
        vals.sort();
        assert_eq!(vals, vec![0, 2, 3, 4]);
    }

    #[test]
    fn drop_called_once_per_element() {
        struct DropCounter {
            count: Arc<AtomicUsize>,
        }

        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.count.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));

        {
            let ops = Box::leak(Box::new(TypeOps::new::<DropCounter>()));
            let mut v = ErasedVec::new(ops);

            for _ in 0..20 {
                v.push_val(DropCounter {
                    count: counter.clone(),
                });
            }

            assert_eq!(counter.load(Ordering::SeqCst), 0);
        }

        assert_eq!(counter.load(Ordering::SeqCst), 20);
    }

    #[test]
    fn swap_remove_drops_removed_element() {
        struct DropCounter {
            count: Arc<AtomicUsize>,
        }

        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.count.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));

        {
            let ops = Box::leak(Box::new(TypeOps::new::<DropCounter>()));
            let mut v = ErasedVec::new(ops);

            for _ in 0..5 {
                v.push_val(DropCounter {
                    count: counter.clone(),
                });
            }

            unsafe {
                v.swap_remove_raw(2);
            }

            assert_eq!(counter.load(Ordering::SeqCst), 1);
        }

        // remaining 4 dropped
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn works_with_string() {
        let ops = Box::leak(Box::new(TypeOps::new::<String>()));
        let mut v = ErasedVec::new(ops);

        for s in ["a", "b", "c"] {
            v.push_val(s.to_string());
        }

        unsafe {
            let a = &*(v.get_raw(0) as *const String);
            let b = &*(v.get_raw(1) as *const String);

            assert_eq!(a, "a");
            assert_eq!(b, "b");
        }
    }
}
