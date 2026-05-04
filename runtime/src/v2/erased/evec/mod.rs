use std::alloc::Layout;

use std::{
    alloc::{alloc, dealloc, handle_alloc_error, realloc},
    ptr::{self, NonNull},
};

use crate::v2::erased::{ErasedBox, TypeOps};

#[derive(Debug)]
pub struct ErasedVec {
    pub ptr: NonNull<u8>,
    pub len: usize,
    pub cap: usize,
    pub ops: &'static TypeOps,
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

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    #[inline]
    fn elem_size(&self) -> usize {
        self.ops.layout.size()
    }

    #[inline]
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

    #[inline]
    fn slot_ptr(&self, index: usize) -> *mut u8 {
        debug_assert!(index < self.cap);
        unsafe { self.ptr.as_ptr().add(index * self.elem_size()) }
    }

    #[inline]
    pub unsafe fn push_raw_move(&mut self, src: *mut u8) {
        if self.len == self.cap {
            self.grow();
        }

        let dst = self.slot_ptr(self.len);

        unsafe { (self.ops.move_fn)(src, dst, self.ops.layout) };

        self.len += 1;
    }

    #[inline]
    pub unsafe fn push_raw_copy_bytes(&mut self, src: *const u8) {
        if self.len == self.cap {
            self.grow();
        }

        let dst = self.slot_ptr(self.len);

        unsafe { ptr::copy_nonoverlapping(src, dst, self.elem_size()) };

        self.len += 1;
    }

    #[inline]
    pub fn get_raw(&self, index: usize) -> *const u8 {
        debug_assert!(index < self.len);
        self.slot_ptr(index)
    }

    #[inline]
    pub fn get_raw_mut(&mut self, index: usize) -> *mut u8 {
        debug_assert!(index < self.len);
        self.slot_ptr(index)
    }

    pub unsafe fn swap_remove_raw(&mut self, index: usize) {
        debug_assert!(index < self.len);

        let last = self.len - 1;

        let victim = self.slot_ptr(index);

        unsafe { (self.ops.drop_fn)(victim) };

        if index != last {
            let src = self.slot_ptr(last);
            unsafe { (self.ops.move_fn)(src, victim, self.ops.layout) };
        }

        self.len -= 1;
    }

    #[inline]
    fn push_val<T>(&mut self, val: T) {
        let mut x = std::mem::ManuallyDrop::new(val);
        unsafe {
            self.push_raw_move((&mut *x as *mut T).cast());
        }
    }

    /// Creates a new, empty ErasedVec with the same type operations and layout
    /// as the current one, but with no elements and no allocated memory.
    #[inline]
    pub fn clear_copy(&self) -> Self {
        Self::new(self.ops)
    }

    #[inline]
    pub fn ops(&self) -> &'static TypeOps {
        self.ops
    }

    /// Move an element out of the vector into a new ErasedBox.
    /// Uses swap_remove semantics.
    pub fn swap_remove_box(&mut self, index: usize) -> ErasedBox {
        debug_assert!(index < self.len);

        let mut out = ErasedBox::new_uninit(self.ops);

        unsafe {
            let last = self.len - 1;
            let victim = self.slot_ptr(index);

            // Move removed element into output box.
            out.write_move(victim);

            // Fill hole using swap-remove.
            if index != last {
                let src = self.slot_ptr(last);
                (self.ops.move_fn)(src, victim, self.ops.layout);
            }

            self.len -= 1;
        }

        out
    }

    /// Push an ErasedBox into the vector.
    #[inline]
    pub fn push_box(&mut self, b: &mut ErasedBox) {
        debug_assert!(std::ptr::eq(self.ops, b.ops()));
        b.move_into_vec(self);
    }

    /// Pop last element into an ErasedBox.
    #[inline]
    pub fn pop_box(&mut self) -> Option<ErasedBox> {
        if self.len == 0 {
            None
        } else {
            Some(self.swap_remove_box(self.len - 1))
        }
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

#[cfg(test)]
mod tests;
