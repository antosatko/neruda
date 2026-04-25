pub mod ebox;
pub mod evec;
use std::alloc::Layout;

pub use ebox::*;
pub use evec::*;

#[derive(Debug, Clone, Copy)]
pub struct TypeOps {
    pub layout: Layout,

    // destroy one element in place
    pub drop_fn: unsafe fn(*mut u8),

    // move one initialized element src -> dst
    // must leave src uninitialized
    pub move_fn: unsafe fn(*mut u8, *mut u8, Layout),
}

impl TypeOps {
    pub fn new<T>() -> Self {
        unsafe fn drop_impl<T>(p: *mut u8) {
            unsafe { std::ptr::drop_in_place(p as *mut T) };
        }

        unsafe fn move_impl<T>(src: *mut u8, dst: *mut u8, _: Layout) {
            // move value out of src into dst
            unsafe { std::ptr::write(dst as *mut T, std::ptr::read(src as *mut T)) };
        }

        Self {
            layout: Layout::new::<T>(),
            drop_fn: drop_impl::<T>,
            move_fn: move_impl::<T>,
        }
    }

    pub unsafe fn from_layout_pod(layout: Layout) -> Self {
        unsafe fn drop_impl(_: *mut u8) {}

        unsafe fn pod_move(src: *mut u8, dst: *mut u8, layout: Layout) {
            unsafe { std::ptr::copy_nonoverlapping(src, dst, layout.size()) };
        }

        Self {
            layout,
            drop_fn: drop_impl,
            move_fn: pod_move,
        }
    }
}
