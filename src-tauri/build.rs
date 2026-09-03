fn main() {
    // The shipped Characters are a bundle resource that tauri-build copies
    // next to the binary, and cargo reruns build scripts only for changes
    // it is told to watch. Without this line a freshly imported package
    // sits in ../characters while the app keeps searching a stale copy in
    // target/debug/characters. Cargo scans a named directory recursively.
    println!("cargo:rerun-if-changed=../characters");
    // icons/icon.ico is here for this call, not for packaging. Targeting
    // Windows, tauri-build always compiles a Windows Resource file and errors
    // out when it cannot find an .ico — there is no way to decline it. So the
    // file is a build input on a platform docs/SPEC.md still puts out of v1,
    // and without it the Windows arm cannot even be type-checked. #247.
    tauri_build::build()
}
