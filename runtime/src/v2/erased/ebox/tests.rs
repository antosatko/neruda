use crate::v2::erased::{ErasedBox, TypeOps};

use std::{
    mem::ManuallyDrop,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[test]
fn stores_and_reads_i32() {
    let ops = Box::leak(Box::new(TypeOps::new::<i32>()));

    let mut boxed = ErasedBox::new_uninit(ops);

    let mut x = ManuallyDrop::new(42i32);

    unsafe {
        boxed.write_move((&mut *x as *mut i32).cast());

        let p = boxed.as_ptr() as *const i32;
        assert_eq!(*p, 42);
    }
}

#[test]
fn from_value_works() {
    let ops = Box::leak(Box::new(TypeOps::new::<u64>()));

    let boxed = ErasedBox::from_value(123u64, ops);

    unsafe {
        let p = boxed.as_ptr() as *const u64;
        assert_eq!(*p, 123);
    }
}

#[test]
fn move_into_moves_value_out() {
    let ops = Box::leak(Box::new(TypeOps::new::<i32>()));

    let mut boxed = ErasedBox::from_value(77i32, ops);

    let mut out = std::mem::MaybeUninit::<i32>::uninit();

    unsafe {
        boxed.move_into(out.as_mut_ptr().cast());

        assert_eq!(out.assume_init(), 77);
    }
}

#[test]
fn drop_called_once() {
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

        let _boxed = ErasedBox::from_value(
            DropCounter {
                count: counter.clone(),
            },
            ops,
        );

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn moved_out_value_not_dropped_twice() {
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

        let mut boxed = ErasedBox::from_value(
            DropCounter {
                count: counter.clone(),
            },
            ops,
        );

        let mut out = std::mem::MaybeUninit::<DropCounter>::uninit();

        unsafe {
            boxed.move_into(out.as_mut_ptr().cast());

            assert_eq!(counter.load(Ordering::SeqCst), 0);

            drop(out.assume_init());
        }
    }

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn works_with_string() {
    let ops = Box::leak(Box::new(TypeOps::new::<String>()));

    let boxed = ErasedBox::from_value("hello".to_string(), ops);

    unsafe {
        let s = &*(boxed.as_ptr() as *const String);
        assert_eq!(s, "hello");
    }
}

#[test]
fn zst_works() {
    #[derive(Debug, PartialEq)]
    struct Marker;

    let ops = Box::leak(Box::new(TypeOps::new::<Marker>()));

    let mut boxed = ErasedBox::from_value(Marker, ops);

    let mut out = std::mem::MaybeUninit::<Marker>::uninit();

    unsafe {
        boxed.move_into(out.as_mut_ptr().cast());

        assert_eq!(out.assume_init(), Marker);
    }
}

#[test]
fn clear_copy_produces_uninitialized_box() {
    let ops = Box::leak(Box::new(TypeOps::new::<i32>()));

    let boxed = ErasedBox::from_value(5, ops);

    let mut copy = boxed.clear_copy();

    let mut x = ManuallyDrop::new(99);

    unsafe {
        copy.write_move((&mut *x as *mut i32).cast());

        let p = copy.as_ptr() as *const i32;
        assert_eq!(*p, 99);
    }
}
