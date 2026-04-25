use crate::v2::erased::{ErasedVec, TypeOps};

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
