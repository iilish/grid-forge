fn main() {
    println!("cargo:rerun-if-env-changed=GRIDFORGE_BUILD_SHA");
}
