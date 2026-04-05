use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let local_proto = Path::new("proto/estafeta/v1");
    let workspace_proto = Path::new("../../proto/estafeta/v1");

    // Copy workspace proto files into the crate so they're included in `cargo publish`
    if workspace_proto.exists() && workspace_proto.is_dir() {
        std::fs::create_dir_all(local_proto)?;
        for entry in std::fs::read_dir(workspace_proto)? {
            let entry = entry?;
            let src = entry.path();
            if src.extension().is_some_and(|ext| ext == "proto") {
                let dest = local_proto.join(entry.file_name());
                std::fs::copy(&src, &dest)?;
            }
        }
    }

    let proto_root = "proto";
    let protos = &[
        "estafeta/v1/common.proto",
        "estafeta/v1/notification.proto",
        "estafeta/v1/schema_registry.proto",
        "estafeta/v1/user_config.proto",
        "estafeta/v1/admin.proto",
        "estafeta/v1/streaming.proto",
    ];

    let proto_paths: Vec<String> = protos
        .iter()
        .map(|p| format!("{proto_root}/{p}"))
        .collect();

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&proto_paths, &[proto_root.to_string()])?;

    for proto in protos {
        println!("cargo:rerun-if-changed=../../proto/{proto}");
        println!("cargo:rerun-if-changed={proto_root}/{proto}");
    }

    Ok(())
}
