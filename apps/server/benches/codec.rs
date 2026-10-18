//! JSON と Protobuf(Google 公式 Rust / upb)のエンコード・デコード性能比較。
//!
//! 同一の論理メッセージ(`ServerEnvelope::Chat` と `ClientEnvelope::SendChat`)について
//! バイト数・encode/decode 時間・スループットを測る。WebSocket を含む E2E 遅延は含まない
//! (シリアライズ単体の測定)。手順と結果は `docs/benchmark.md`。
//!
//! 実行: `cargo bench -p protolive-server --bench codec`
//! 起動時にペイロードのバイト数表を標準エラーへ出力する。

use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use protobuf::{Parse, Serialize};
use protolive_server::proto::{ChatMessage, ClientEnvelope, SendChatRequest, ServerEnvelope};
use serde::{Deserialize, Serialize as SerdeSerialize};

/// JSON 側の同等表現。oneof は serde の外部タグ付き enum で表す(`{"chat":{...}}`)。
#[derive(SerdeSerialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum JsonServerEnvelope {
    Chat(JsonChat),
}

#[derive(SerdeSerialize, Deserialize)]
struct JsonChat {
    id: String,
    sender_id: String,
    nickname: String,
    content: String,
    timestamp_ms: u64,
}

#[derive(SerdeSerialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum JsonClientEnvelope {
    SendChat { content: String },
}

/// ペイロード種別。本文の長さと文字種を変えて比較する。
struct Case {
    name: &'static str,
    content: String,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "short_ascii",
            content: "hello".to_owned(),
        },
        Case {
            name: "short_japanese",
            content: "こんにちは、世界!".to_owned(),
        },
        Case {
            name: "medium_ascii_200",
            content: "a".repeat(200),
        },
        Case {
            name: "long_japanese_1000",
            content: "あ".repeat(1000),
        },
    ]
}

fn server_pb(content: &str) -> ServerEnvelope {
    let mut msg = ChatMessage::new();
    msg.set_id("0b6f2d1c-8f3a-4c1e-9a52-6f1f0c7d3e11");
    msg.set_sender_id("7badfc1b-5a0a-4de5-afa3-49eb835cb668");
    msg.set_nickname("ゆうき");
    msg.set_content(content);
    msg.set_timestamp_ms(1_760_000_000_123);
    let mut env = ServerEnvelope::new();
    env.set_chat(msg);
    env
}

fn server_json(content: &str) -> JsonServerEnvelope {
    JsonServerEnvelope::Chat(JsonChat {
        id: "0b6f2d1c-8f3a-4c1e-9a52-6f1f0c7d3e11".to_owned(),
        sender_id: "7badfc1b-5a0a-4de5-afa3-49eb835cb668".to_owned(),
        nickname: "ゆうき".to_owned(),
        content: content.to_owned(),
        timestamp_ms: 1_760_000_000_123,
    })
}

fn client_pb(content: &str) -> ClientEnvelope {
    let mut req = SendChatRequest::new();
    req.set_content(content);
    let mut env = ClientEnvelope::new();
    env.set_send_chat(req);
    env
}

/// 条件の再現に必要な情報と、ペイロードのバイト数表を標準エラーに出す。
fn print_sizes(cases: &[Case]) {
    eprintln!(
        "# payload bytes (JSON = serde_json compact, Protobuf = proto3 wire)\n\
         # {:<20} {:>10} {:>10} {:>8}   {:>10} {:>10} {:>8}",
        "case", "srv json", "srv pb", "pb/json", "cli json", "cli pb", "pb/json"
    );
    for c in cases {
        let sj = serde_json::to_vec(&server_json(&c.content)).unwrap().len();
        let sp = server_pb(&c.content).serialize().unwrap().len();
        let cj = serde_json::to_vec(&JsonClientEnvelope::SendChat {
            content: c.content.clone(),
        })
        .unwrap()
        .len();
        let cp = client_pb(&c.content).serialize().unwrap().len();
        eprintln!(
            "# {:<20} {:>10} {:>10} {:>7.2}%   {:>10} {:>10} {:>7.2}%",
            c.name,
            sj,
            sp,
            sp as f64 / sj as f64 * 100.0,
            cj,
            cp,
            cp as f64 / cj as f64 * 100.0
        );
    }
}

fn bench_codec(c: &mut Criterion) {
    let cases = cases();
    print_sizes(&cases);

    for case in &cases {
        let mut group = c.benchmark_group(format!("server_chat/{}", case.name));
        let pb_msg = server_pb(&case.content);
        let json_msg = server_json(&case.content);
        let pb_bytes = pb_msg.serialize().unwrap();
        let json_bytes = serde_json::to_vec(&json_msg).unwrap();

        // スループットは「そのフォーマットで実際に運ぶバイト数」基準。
        group.throughput(Throughput::Bytes(pb_bytes.len() as u64));
        group.bench_function("protobuf/encode", |b| {
            b.iter(|| black_box(&pb_msg).serialize().unwrap())
        });
        group.bench_function("protobuf/decode", |b| {
            b.iter(|| ServerEnvelope::parse(black_box(&pb_bytes)).unwrap())
        });
        // Protobuf は encode 済みの値から作る場合の構築コストも含めた「構築+encode」も測る。
        group.bench_function("protobuf/build_encode", |b| {
            b.iter_batched(
                || case.content.as_str(),
                |content| server_pb(black_box(content)).serialize().unwrap(),
                BatchSize::SmallInput,
            )
        });

        group.throughput(Throughput::Bytes(json_bytes.len() as u64));
        group.bench_function("json/encode", |b| {
            b.iter(|| serde_json::to_vec(black_box(&json_msg)).unwrap())
        });
        group.bench_function("json/decode", |b| {
            b.iter(|| serde_json::from_slice::<JsonServerEnvelope>(black_box(&json_bytes)).unwrap())
        });
        group.bench_function("json/build_encode", |b| {
            b.iter_batched(
                || case.content.as_str(),
                |content| serde_json::to_vec(&server_json(black_box(content))).unwrap(),
                BatchSize::SmallInput,
            )
        });
        group.finish();
    }
}

criterion_group!(benches, bench_codec);
criterion_main!(benches);
