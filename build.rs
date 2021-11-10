fn main() {
    #[cfg(unix)]
    {
        println!("cargo:rustc-link-lib=vulkan");
        println!("cargo:rustc-link-lib=EGL");
    }
}
