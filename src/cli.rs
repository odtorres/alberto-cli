//! Definición de la CLI (clap). Sin lógica: solo tipos.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Formato de salida para respuestas monádicas.
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Output {
    /// JSON indentado (default)
    Pretty,
    /// JSON compacto en una línea (para pipes)
    Json,
    /// result_json tal cual llegó del servidor
    Raw,
    /// Tabla (solo para resultados que son listas)
    Table,
}

#[derive(Parser)]
#[command(
    name = "alberto",
    version,
    about = "CLI para NodeService: upload por gRPC streaming + operaciones de nodos"
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Sube un archivo por gRPC streaming y crea el nodo (variantes plain/assoc/signed)
    Upload(UploadArgs),
    /// Operaciones de nodos vía gRPC NodeManagerService (respuesta monádica)
    Node {
        #[command(subcommand)]
        cmd: NodeCmd,
    },
    /// Operaciones de tenant vía gRPC NodeManagerService
    Tenant {
        #[command(subcommand)]
        cmd: TenantCmd,
    },
    /// Operaciones administrativas (folders, grupos, índices)
    Admin {
        #[command(subcommand)]
        cmd: AdminCmd,
    },
    /// Manejo del archivo de configuración (~/.config/alberto/config.toml)
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Navegador interactivo (TUI) con preview de PDFs en la terminal
    Tui {
        /// Tenant cuyo document library se navega
        #[arg(long)]
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Descarga el contenido binario de un nodo (gRPC NodeContent)
    Download {
        /// unique_id del nodo (único en todo el repositorio)
        id: String,
        /// Ruta de destino (default: <unique_id>.bin)
        dest: Option<PathBuf>,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Genera autocompletado para tu shell (agrega a tu rc)
    Completions {
        /// Shell destino
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

/// Argumentos de `alberto upload` (variantes plain/assoc/signed).
#[derive(clap::Args)]
pub struct UploadArgs {
    /// Archivo a subir
    pub file: PathBuf,
    /// Tipo documental del nodo (ej: factura)
    #[arg(long = "type")]
    pub node_type: String,
    /// Título del nodo (default: nombre del archivo)
    #[arg(long)]
    pub title: Option<String>,
    /// Descripción
    #[arg(long, default_value = "")]
    pub description: String,
    /// unique_id del nodo padre (debe existir)
    #[arg(long)]
    pub parent: String,
    /// Username que sube (debe existir)
    #[arg(long)]
    pub user: String,
    /// Tenant (informativo; el efectivo se hereda del parent)
    #[arg(long, default_value = "")]
    pub tenant: String,
    /// Metadata JSON del nodo, ej: '{"rut":"1-9"}'
    #[arg(long, default_value = "{}")]
    pub data: String,
    /// unique_id a asociar como secondary_parent (activa variante assoc)
    #[arg(long)]
    pub assoc: Option<String>,
    /// unique_id del contenido firmado a referenciar (activa variante signed)
    #[arg(long)]
    pub signed_ref: Option<String>,
    #[command(flatten)]
    pub grpc: GrpcOpts,
    /// Intentos totales ante fallas de red/timeout (la idempotencia evita duplicados)
    #[arg(long, default_value_t = 3)]
    pub retries: u32,
}

/// Conexión ya resuelta (flags/env/perfil combinados).
pub struct Conn {
    pub endpoint: String,
    pub api_key: String,
}

#[derive(clap::Args, Clone)]
pub struct GrpcOpts {
    /// Endpoint gRPC (default: el del perfil, o http://127.0.0.1:9090)
    #[arg(long, env = "ALBERTO_GRPC_ENDPOINT")]
    pub endpoint: Option<String>,
    /// API key (metadata x-api-key)
    #[arg(long, env = "ALBERTO_API_KEY")]
    pub api_key: Option<String>,
    /// Perfil de ~/.config/alberto/config.toml
    #[arg(long, env = "ALBERTO_PROFILE")]
    pub profile: Option<String>,
    /// Formato de salida
    #[arg(long, value_enum, default_value_t = Output::Pretty)]
    pub output: Output,
}

impl GrpcOpts {
    pub fn resolve(&self) -> anyhow::Result<Conn> {
        let cfg = crate::config::load()?;
        let (endpoint, api_key) = crate::config::resolve(
            &cfg,
            self.profile.as_deref(),
            self.endpoint.clone(),
            self.api_key.clone(),
        )?;
        Ok(Conn { endpoint, api_key })
    }
}

#[derive(Subcommand)]
pub enum NodeCmd {
    /// Obtiene un nodo por unique_id (NodeGet)
    Get {
        id: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Obtiene varios nodos por sus unique_ids (Ids); --type filtra por tipo
    Ids {
        /// Lista de unique_ids separados por espacio
        #[arg(required = true, num_args = 1..)]
        ids: Vec<String>,
        #[arg(long = "type", default_value = "")]
        node_type: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Lista nodos por tipo dentro de un tenant (ByType)
    ByType {
        #[arg(long = "type")]
        node_type: String,
        #[arg(long)]
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Obtiene un nodo por path (ByPath); --tenant opcional para path relativo
    ByPath {
        path: String,
        #[arg(long, default_value = "")]
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Lista los hijos de un nodo (NodeChild); --secondary para secondary_parent
    Children {
        id: String,
        #[arg(long, default_value_t = false)]
        secondary: bool,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Busca un usuario por username (User; password siempre enmascarada)
    User {
        username: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Mezcla data JSON en el data del nodo (Datamerge, :datamerge_m)
    Datamerge {
        id: String,
        /// JSON a mezclar, ej: '{"estado":"procesado"}'
        #[arg(long)]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Merge estricto de data (DataUpdate, :dataupdate)
    DataUpdate {
        id: String,
        #[arg(long)]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Merge masivo de data en varios nodos (BulkDatamerge, :bulk_datamerge_m)
    BulkDatamerge {
        /// JSON con la colección de cambios
        #[arg(long)]
        changes: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Actualiza un valor dentro del data de un nodo ubicado por path (Patch, :patch_m)
    Patch {
        /// Path del nodo, ej /tenants/t/documentlibrary/x
        envelope_path: String,
        /// Ruta dentro del data (claves separadas)
        #[arg(long, required = true, num_args = 1..)]
        path: Vec<String>,
        /// Valor JSON a colocar
        #[arg(long)]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Lee un nodo por path, opcionalmente un valor interno del data (Get, :get_m)
    GetIn {
        /// Path del nodo
        node_path: String,
        /// Ruta interna del data (vacío = nodo completo)
        #[arg(long, num_args = 0..)]
        path: Vec<String>,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Busca nodos por nombre (NodeByName, :nodebyname)
    ByName {
        name: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Crea un nodo hijo sin contenido (NodeCreate, :node_m)
    Create {
        #[arg(long)]
        parent: String,
        #[arg(long = "type")]
        node_type: String,
        /// JSON del data; DEBE incluir "name"
        #[arg(long)]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Agrega un secondary parent (AddSecondaryParent, :addsecundaryparent)
    AddSecondary {
        child_id: String,
        parent_id: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Quita un secondary parent (RemoveSecondaryParent, :removesecundaryparent)
    RemoveSecondary {
        child_id: String,
        parent_id: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
}

#[derive(Subcommand)]
pub enum TenantCmd {
    /// Obtiene el nodo del tenant (TenantGet, :tenant)
    Get {
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Crea un tenant (TenantCreate, :tenant)
    Create {
        tenant: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        dni: String,
        #[arg(long)]
        company: String,
        #[arg(long)]
        email: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Document library del tenant (DocLib, :doc_lib)
    Doclib {
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Home de un tipo documental (Home, :home)
    Home {
        tenant: String,
        #[arg(long = "type")]
        node_type: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Package/home clasificado de un tipo (Package, :package_m)
    Package {
        tenant: String,
        #[arg(long = "type")]
        node_type: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
}

#[derive(Subcommand)]
pub enum AdminCmd {
    /// Crea un folder bajo un nodo (Folder, :folder)
    Folder {
        #[arg(long)]
        parent: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "{}")]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Crea el grupo default con nombre (DefaultGroup, :default_group)
    DefaultGroup {
        #[arg(long)]
        name: String,
        #[arg(long)]
        parent: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Grupo colaborador default del tenant (:default_colaborator_group)
    ColaboratorGroup {
        #[arg(long)]
        parent: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Grupo consumidor default del tenant (:default_consumer_group)
    ConsumerGroup {
        #[arg(long)]
        parent: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Grupo administrador default del tenant (:default_administrator_group)
    AdministratorGroup {
        #[arg(long)]
        parent: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Índices configurados en etcd (Indexs, :indexs)
    Indexs {
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Tipos documentales del document library (DocLibsTypes, :doc_libs_types)
    DoclibTypes {
        #[command(flatten)]
        grpc: GrpcOpts,
    },
}

#[derive(Subcommand)]
pub enum ConfigCmd {
    /// Crea un config de ejemplo si no existe
    Init,
    /// Lista los perfiles configurados
    List,
    /// Muestra los valores efectivos de un perfil (api_key enmascarada)
    Show {
        /// Perfil a mostrar (default: default_profile)
        profile: Option<String>,
    },
}
