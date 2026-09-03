fn main() {
    // The shipped Characters are a bundle resource that tauri-build copies
    // next to the binary, and cargo reruns build scripts only for changes
    // it is told to watch. Without this line a freshly imported package
    // sits in ../characters while the app keeps searching a stale copy in
    // target/debug/characters. Cargo scans a named directory recursively.
    println!("cargo:rerun-if-changed=../characters");
    // icons/icon.ico is a build input for this call, not a bundle asset:
    // targeting Windows, tauri-build always compiles a Windows Resource file
    // and errors out when no .ico is there to compile. Without it the Windows
    // arm cannot even be type-checked, on a platform `docs/SPEC.md` still puts
    // out of v1.
    //
    // It is generated from icons/icon.png, the icon tauri.conf.json actually
    // bundles, at sizes 16/24/32/48/64/256. Nothing regenerates it, so redraw
    // the .png and this goes stale until someone redraws this too. #247.
    tauri_build::build()
}
