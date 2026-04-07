fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=native/macos/quicklook_bridge.m");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=QuickLookUI");

        cc::Build::new()
            .file("native/macos/quicklook_bridge.m")
            .flag("-fobjc-arc")
            .compile("quicklook_bridge");
    }
}
