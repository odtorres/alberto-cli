fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Espejos de apps/nodeservice/priv/protos/*.proto (repo umbrella).
    // Si el contrato cambia en el servidor, copiar aquí los .proto actualizados.
    tonic_build::compile_protos("proto/binary_transfer.proto")?;
    tonic_build::compile_protos("proto/node_manager.proto")?;
    Ok(())
}
