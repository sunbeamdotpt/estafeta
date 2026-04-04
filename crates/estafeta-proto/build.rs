fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = "../../proto";
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
        println!("cargo:rerun-if-changed={proto_root}/{proto}");
    }

    Ok(())
}
