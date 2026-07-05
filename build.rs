fn main() -> Result<(), Box<dyn std::error::Error>> {
    // protoc vendorizado: la compilación no depende de protoc del sistema
    // (necesario para `cargo install` y para CI limpia).
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    // Espejos de apps/nodeservice/priv/protos/*.proto (repo umbrella).
    // Si el contrato cambia en el servidor, copiar aquí los .proto actualizados.
    tonic_build::compile_protos("proto/binary_transfer.proto")?;
    tonic_build::compile_protos("proto/node_manager.proto")?;
    Ok(())
}
