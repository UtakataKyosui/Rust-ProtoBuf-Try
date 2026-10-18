use std::path::PathBuf;

fn main() {
    let proto_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../proto");
    let proto_files = ["protolive/v1/chat.proto"];

    println!("cargo:rerun-if-changed={}", proto_root.display());

    protobuf_codegen::CodeGen::new()
        .inputs(proto_files)
        .include(&proto_root)
        .generate_and_compile()
        .expect("protoc による Rust コード生成に失敗しました");
}
