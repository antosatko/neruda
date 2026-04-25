use std::{
    alloc::{alloc, dealloc, handle_alloc_error},
    ptr::{self, NonNull},
};

use crate::v2::erased::{ErasedVec, TypeOps};

#[derive(Debug)]
pub struct ErasedBox {
    ptr: NonNull<u8>,
    initialized: bool,
    ops: &'static TypeOps,
}

impl ErasedBox {
    pub fn new_uninit(ops: &'static TypeOps) -> Self {
        let layout = ops.layout;

        let ptr = if layout.size() == 0 {
            NonNull::dangling()
        } else {
            unsafe {
                let p = alloc(layout);
                if p.is_null() {
                    handle_alloc_error(layout);
                }
                NonNull::new_unchecked(p)
            }
        };

        Self {
            ptr,
            initialized: false,
            ops,
        }
    }

    pub fn from_value<T>(value: T, ops: &'static TypeOps) -> Self {
        let mut b = Self::new_uninit(ops);
        let mut value = std::mem::ManuallyDrop::new(value);

        unsafe {
            b.write_move((&mut *value as *mut T).cast());
        }

        b
    }

    pub unsafe fn write_move(&mut self, src: *mut u8) {
        debug_assert!(!self.initialized);
        unsafe { (self.ops.move_fn)(src, self.ptr.as_ptr(), self.ops.layout) };
        self.initialized = true;
    }

    pub unsafe fn write_copy_bytes(&mut self, src: *const u8) {
        debug_assert!(!self.initialized);
        unsafe { ptr::copy_nonoverlapping(src, self.ptr.as_ptr(), self.ops.layout.size()) };
        self.initialized = true;
    }

    pub fn as_ptr(&self) -> *const u8 {
        debug_assert!(self.initialized);
        self.ptr.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        debug_assert!(self.initialized);
        self.ptr.as_ptr()
    }

    /// Move value out into destination, consuming initialization.
    pub unsafe fn move_into(&mut self, dst: *mut u8) {
        debug_assert!(self.initialized);
        unsafe { (self.ops.move_fn)(self.ptr.as_ptr(), dst, self.ops.layout) };
        self.initialized = false;
    }

    pub fn clear_copy(&self) -> Self {
        Self::new_uninit(self.ops)
    }

    pub fn ops(&self) -> &'static TypeOps {
        self.ops
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Move this boxed value into an ErasedVec.
    /// Consumes the box's initialization.
    pub fn move_into_vec(&mut self, vec: &mut ErasedVec) {
        assert!(std::ptr::eq(self.ops, vec.ops()));

        unsafe {
            vec.push_raw_move(self.ptr.as_ptr());
        }

        self.initialized = false;
    }

    /// Convert this box into a single-element vector.
    pub fn into_vec(mut self) -> ErasedVec {
        let mut vec = ErasedVec::new(self.ops);
        self.move_into_vec(&mut vec);
        vec
    }
}

impl Drop for ErasedBox {
    fn drop(&mut self) {
        unsafe {
            if self.initialized {
                (self.ops.drop_fn)(self.ptr.as_ptr());
            }

            if self.ops.layout.size() > 0 {
                dealloc(self.ptr.as_ptr(), self.ops.layout);
            }
        }
    }
}

#[cfg(test)]
mod tests;
