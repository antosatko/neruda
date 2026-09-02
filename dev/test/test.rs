extern "C" {
    fn fun_0(a: i32) -> i32;
}

fn main() {
    println!("Starting test program...");

    unsafe {
        println!("result: {}", fun_0(42));
    }

    println!("Test completed successfully!");
}
