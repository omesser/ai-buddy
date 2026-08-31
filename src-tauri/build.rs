fn main() {
    // The shipped Characters are a bundle resource that tauri-build copies
    // next to the binary, and cargo reruns build scripts only for changes
    // it is told to watch. Without this line a freshly imported package
    // sits in ../characters while the app keeps searching a stale copy in
    // target/debug/characters. Cargo scans a named directory recursively.
    println!("cargo:rerun-if-changed=../characters");
    tauri_build::build()
}
