fn main() {
    println!("cargo:rustc-link-lib=dlog");
    println!("cargo:rustc-link-lib=pkgmgr_installer");
}
