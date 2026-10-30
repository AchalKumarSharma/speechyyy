fn main() {
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib");
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../share/speechyyy/lib");
    if let Ok(home) = std::env::var("HOME") {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}/.local/share/speechyyy/lib",
            home
        );
    }
}
