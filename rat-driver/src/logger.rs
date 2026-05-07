#[macro_export]
macro_rules! __rat_log_internal {
    ($level_str:expr, $log_macro:path, $($arg:tt)*) => {
        {
            // Note: We use format_args! to pass the arguments through correctly
            wdk::println!(concat!("[RAT Driver] ", $level_str, ": {}"), format_args!($($arg)*));
            $log_macro!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => { $crate::__rat_log_internal!("ERROR", log::error, $($arg)*) };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => { $crate::__rat_log_internal!("WARN", log::warn, $($arg)*) };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { $crate::__rat_log_internal!("INFO", log::info, $($arg)*) };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => { $crate::__rat_log_internal!("DEBUG", log::debug, $($arg)*) };
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => { $crate::__rat_log_internal!("TRACE", log::trace, $($arg)*) };
}
