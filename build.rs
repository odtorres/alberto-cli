fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Espejo de apps/nodeservice/priv/protos/binary_transfer.proto (repo umbrella).
    // Si el contrato cambia en el servidor, copiar aquí el .proto actualizado.
    tonic_build::compile_protos("proto/binary_transfer.proto")?;
    Ok(())
}
