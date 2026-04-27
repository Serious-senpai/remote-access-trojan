#[macro_export]
macro_rules! log {
    () => {
        wdk::println!("[RAT Driver]");
    };
    ($($arg:tt)*) => {
        wdk::println!("[RAT Driver] {}", format_args!($($arg)*));
    };
}
