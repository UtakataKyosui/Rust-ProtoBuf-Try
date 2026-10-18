//! Rust ↔ TypeScript 相互運用の golden バイナリ検証。
//!
//! `fixtures/golden/*.bin` が正本。Rust 側は同じメッセージを構築してバイト列が一致することと、
//! fixture をデコードして内容が一致することを確認する。TypeScript 側は
//! `packages/protocol-ts/src/golden.test.ts` で同じ fixture を検証する。
//!
//! fixture を更新する場合: `UPDATE_GOLDEN=1 cargo test -p protolive-server --test golden`

use std::path::PathBuf;

use protobuf::{Parse, Serialize};
use protolive_server::proto::{
    ChatMessage, ClientEnvelope, ErrorEvent, JoinRequest, MemberEvent, SendChatRequest,
    ServerEnvelope, client_envelope, server_envelope,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/golden")
        .join(format!("{name}.bin"))
}

/// `UPDATE_GOLDEN=1` なら fixture を書き出し、そうでなければ既存 fixture と一致を確認する。
fn assert_golden(name: &str, encoded: &[u8]) {
    let path = fixture_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, encoded).unwrap();
    }
    let expected = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("fixture {} を読めません: {e}", path.display()));
    assert_eq!(
        encoded,
        expected.as_slice(),
        "fixture {name} とエンコード結果が不一致"
    );
}

fn read_fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap()
}

fn join(nickname: &str) -> ClientEnvelope {
    let mut join = JoinRequest::new();
    join.set_nickname(nickname);
    let mut env = ClientEnvelope::new();
    env.set_join(join);
    env
}

fn send_chat(content: &str) -> ClientEnvelope {
    let mut req = SendChatRequest::new();
    req.set_content(content);
    let mut env = ClientEnvelope::new();
    env.set_send_chat(req);
    env
}

fn member_event(sender_id: &str, nickname: &str) -> MemberEvent {
    let mut ev = MemberEvent::new();
    ev.set_sender_id(sender_id);
    ev.set_nickname(nickname);
    ev
}

fn check_client(name: &str, env: &ClientEnvelope) {
    let bytes = env.serialize().unwrap();
    assert_golden(name, &bytes);
    let decoded = ClientEnvelope::parse(&read_fixture(name)).unwrap();
    assert_eq!(
        decoded.serialize().unwrap(),
        bytes,
        "{name}: 再エンコードが不一致"
    );
}

fn check_server(name: &str, env: &ServerEnvelope) {
    let bytes = env.serialize().unwrap();
    assert_golden(name, &bytes);
    let decoded = ServerEnvelope::parse(&read_fixture(name)).unwrap();
    assert_eq!(
        decoded.serialize().unwrap(),
        bytes,
        "{name}: 再エンコードが不一致"
    );
}

#[test]
fn client_join() {
    check_client("client_join", &join("alice"));
    let decoded = ClientEnvelope::parse(&read_fixture("client_join")).unwrap();
    match decoded.payload() {
        client_envelope::PayloadOneof::Join(j) => assert_eq!(j.nickname(), "alice"),
        other => panic!("join 以外: {other:?}"),
    }
}

#[test]
fn client_join_japanese() {
    check_client("client_join_japanese", &join("ゆうき🎉"));
    let decoded = ClientEnvelope::parse(&read_fixture("client_join_japanese")).unwrap();
    assert_eq!(decoded.join().nickname(), "ゆうき🎉");
}

#[test]
fn client_send_chat_japanese() {
    check_client("client_send_chat_japanese", &send_chat("こんにちは、世界"));
    let decoded = ClientEnvelope::parse(&read_fixture("client_send_chat_japanese")).unwrap();
    assert!(decoded.has_send_chat());
    assert!(!decoded.has_join());
    assert_eq!(decoded.send_chat().content(), "こんにちは、世界");
}

#[test]
fn client_send_chat_empty() {
    // proto3 のデフォルト値(空文字)はフィールドが省略され、envelope 内に空メッセージだけが残る。
    check_client("client_send_chat_empty", &send_chat(""));
    let decoded = ClientEnvelope::parse(&read_fixture("client_send_chat_empty")).unwrap();
    assert!(decoded.has_send_chat());
    assert_eq!(decoded.send_chat().content(), "");
}

#[test]
fn client_send_chat_boundary_lengths() {
    // 長さ varint が 1→2 バイト(128)、2→3 バイト(16384)に切り替わる境界。
    for len in [127usize, 128, 16384] {
        let content = "a".repeat(len);
        let name = format!("client_send_chat_len_{len}");
        check_client(&name, &send_chat(&content));
        let decoded = ClientEnvelope::parse(&read_fixture(&name)).unwrap();
        assert_eq!(decoded.send_chat().content().as_bytes().len(), len);
    }
}

#[test]
fn server_member_events() {
    let mut joined = ServerEnvelope::new();
    joined.set_joined(member_event("conn-1", "alice"));
    check_server("server_joined", &joined);

    let mut left = ServerEnvelope::new();
    left.set_left(member_event("conn-1", "alice"));
    check_server("server_left", &left);

    // joined と left は同じ MemberEvent 型でも oneof のケースで区別される。
    let decoded = ServerEnvelope::parse(&read_fixture("server_left")).unwrap();
    assert!(decoded.has_left());
    assert!(!decoded.has_joined());
}

#[test]
fn server_chat() {
    let mut chat = ChatMessage::new();
    chat.set_id("msg-0001");
    chat.set_sender_id("conn-1");
    chat.set_nickname("ゆうき");
    chat.set_content("こんにちは 👋");
    chat.set_timestamp_ms(1_760_000_000_123);
    let mut env = ServerEnvelope::new();
    env.set_chat(chat);
    check_server("server_chat", &env);

    let decoded = ServerEnvelope::parse(&read_fixture("server_chat")).unwrap();
    match decoded.payload() {
        server_envelope::PayloadOneof::Chat(c) => {
            assert_eq!(c.id(), "msg-0001");
            assert_eq!(c.nickname(), "ゆうき");
            assert_eq!(c.content(), "こんにちは 👋");
            assert_eq!(c.timestamp_ms(), 1_760_000_000_123);
        }
        other => panic!("chat 以外: {other:?}"),
    }
}

#[test]
fn server_error() {
    let mut err = ErrorEvent::new();
    err.set_code("invalid_nickname");
    err.set_message("ニックネームが不正です");
    let mut env = ServerEnvelope::new();
    env.set_error(err);
    check_server("server_error", &env);
}

#[test]
fn unknown_fields_are_ignored_and_preserved() {
    // client_join に未知フィールド(番号 99, varint 1)を末尾追加した fixture。
    let mut bytes = join("alice").serialize().unwrap();
    bytes.extend_from_slice(&[0x98, 0x06, 0x01]);
    assert_golden("client_join_unknown_field", &bytes);

    let decoded = ClientEnvelope::parse(&read_fixture("client_join_unknown_field")).unwrap();
    assert_eq!(decoded.join().nickname(), "alice");
    // 公式ランタイムは未知フィールドを保持して再エンコードする。
    assert_eq!(decoded.serialize().unwrap(), bytes);
}

#[test]
fn invalid_bytes_are_rejected() {
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(fixture_path("invalid_truncated"), [0x0a, 0x05, 0x41]).unwrap();
        std::fs::write(fixture_path("invalid_wire_type"), [0x0f]).unwrap();
        // ClientEnvelope.join(1) { nickname(1) = [0xff] }
        std::fs::write(fixture_path("invalid_utf8"), [0x0a, 0x03, 0x0a, 0x01, 0xff]).unwrap();
    }
    // 長さ 5 を宣言しているが 1 バイトしかない(切り詰め)。
    assert!(ClientEnvelope::parse(&read_fixture("invalid_truncated")).is_err());
    // 不正なワイヤタイプ (7)。
    assert!(ClientEnvelope::parse(&read_fixture("invalid_wire_type")).is_err());
    // 文字列フィールドに不正な UTF-8 (proto3 は検証必須)。
    assert!(ClientEnvelope::parse(&read_fixture("invalid_utf8")).is_err());
}
