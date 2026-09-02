extern "C" {
    fn foo(a: i32) -> i32;
    fn loops(a: i32);
    fn fib(n: i32) -> i32;
}

fn main() {
    unsafe {
        println!("foo(42) => {}", foo(42));
        println!("foo(67) => {}", foo(67));
        println!("loops(700) not trap anymore");
        loops(700);
        println!(
            "fib sequence: {:?}",
            (1..10).into_iter().map(|n| (n, fib(n))).collect::<Vec<_>>()
        );
    }
}
