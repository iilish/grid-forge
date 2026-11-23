fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let include = protoc_bin_vendored::include_path()?;

    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    println!("cargo:rerun-if-changed=proto/gridforge/v1/envelope.proto");

    prost_build::Config::new().compile_protos(
        &["proto/gridforge/v1/envelope.proto"],
        &["proto", include.to_str().expect("include path is UTF-8")],
    )?;

    Ok(())
}
