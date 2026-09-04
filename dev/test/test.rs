use std::hint::black_box;
use std::time::Instant;

extern "C" {
    fn foo(a: i32) -> i32;
    fn fact(a: i32) -> i32;
    fn loops(a: i32);
    fn fib(n: i32) -> i32;
    fn fib_rec(n: i32) -> i32;
}

fn fib_rust(mut n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    let mut _0 = 0;
    let mut _1 = 1;
    loop {
        let current = _0 + _1;
        _0 = _1;
        _1 = current;
        if n == 2 {
            return current;
        }
        n = n - 1;
    }
}

fn fib_rust_rec(n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    return fib_rust_rec(n - 2) + fib_rust_rec(n - 1);
}

fn main() {
    unsafe {
        println!("foo(42) => {}", foo(42));
        println!("foo(67) => {}", foo(67));
        println!("loops(700) not trap anymore");
        loops(700);
        println!(
            "fib sequence recursive: {:?}",
            (1..10)
                .into_iter()
                .map(|n| (n, fact(n)))
                .collect::<Vec<_>>()
        );

        println!("perf test fib(10000)");
        println!("fact(6) => {}", fact(6));
        let start = Instant::now();
        for _ in 0..100 {
            black_box(fib_rust_rec(25));
        }
        println!("rust => {}: {:?}", fib_rust_rec(25), start.elapsed());
        let start = Instant::now();
        for _ in 0..100 {
            black_box(fib_rec(25));
        }
        println!("neruda => {}: {:?}", fib_rec(25), start.elapsed());
    }
}
