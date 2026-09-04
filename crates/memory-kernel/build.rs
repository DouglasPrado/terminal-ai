fn main() {
    println!(
        "cargo:rustc-env=TARGET_TRIPLE_HINT={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".into())
    );
}
