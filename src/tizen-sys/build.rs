fn main() {
    // Only Tizen-specific libraries
    println!("cargo:rustc-link-lib=dlog");
    println!("cargo:rustc-link-lib=tizen-core");
    println!("cargo:rustc-link-lib=capi-base-common");

    // libsoup for web dashboard server
    println!("cargo:rustc-link-lib=soup-2.4");
    println!("cargo:rustc-link-lib=glib-2.0");
    println!("cargo:rustc-link-lib=gobject-2.0");
    println!("cargo:rustc-link-lib=gio-2.0");

    // Tizen app framework
    println!("cargo:rustc-link-lib=pkgmgr-client");
    println!("cargo:rustc-link-lib=vconf");
    println!("cargo:rustc-link-lib=capi-appfw-event");
}
