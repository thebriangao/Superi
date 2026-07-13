fn main() {
    println!("cargo:rerun-if-changed=src/vpx_shim.c");
    println!("cargo:rerun-if-changed=src/vpx_shim.h");
    println!("cargo:rerun-if-changed=vendor/libvpx/include");

    cc::Build::new()
        .file("src/vpx_shim.c")
        .include("vendor/libvpx/include")
        .warnings(true)
        .compile("superi_vpx_shim");
}
