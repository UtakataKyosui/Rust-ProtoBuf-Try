//! ProtoLive サーバーのライブラリ部。通信型は `proto/` を単一の定義元として生成される。

#[allow(clippy::all, clippy::pedantic, warnings)]
pub mod proto {
    //! `proto/protolive/v1/chat.proto` から Google 公式 Rust Protobuf (upb カーネル) で生成された型。
    include!(concat!(
        env!("OUT_DIR"),
        "/protobuf_generated/protolive/v1/generated.rs"
    ));
}
