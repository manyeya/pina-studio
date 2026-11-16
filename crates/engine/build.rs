//! ONNX Runtime's Core ML code (linked in on Apple silicon, for stem separation) checks the
//! macOS version with `@available`, whose other half (`__isPlatformVersionAtLeast`) is in clang's
//! runtime. rustc links without it (`-nodefaultlibs`), so it's linked here, as clang would.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("TARGET").as_deref() != Ok("aarch64-apple-darwin") {
        return;
    }
    let dir = std::process::Command::new("clang")
        .arg("--print-resource-dir")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    match dir {
        Some(dir) => {
            println!("cargo:rustc-link-search=native={dir}/lib/darwin");
            println!("cargo:rustc-link-lib=static=clang_rt.osx");
        }
        None => println!("cargo:warning=clang not found: linking ONNX Runtime may fail"),
    }
}
