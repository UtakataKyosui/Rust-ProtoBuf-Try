//! 実サーバーを立てた WebSocket 統合テスト。

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protobuf::{Parse, Serialize};
use protolive_server::proto::{
    ClientEnvelope, JoinRequest, SendChatRequest, ServerEnvelope, server_envelope,
};
use protolive_server::server::{AppState, ServerConfig, router};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn start(config: ServerConfig) -> (SocketAddr, watch::Sender<bool>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = watch::channel(false);
    let app = router(AppState::new(config, rx));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (addr, tx)
}

async fn connect(addr: SocketAddr) -> Client {
    connect_async(format!("ws://{addr}/ws")).await.unwrap().0
}

fn join(nickname: &str) -> Message {
    let mut j = JoinRequest::new();
    j.set_nickname(nickname);
    let mut e = ClientEnvelope::new();
    e.set_join(j);
    Message::Binary(e.serialize().unwrap().into())
}

fn chat(content: &str) -> Message {
    let mut c = SendChatRequest::new();
    c.set_content(content);
    let mut e = ClientEnvelope::new();
    e.set_send_chat(c);
    Message::Binary(e.serialize().unwrap().into())
}

/// 次のアプリケーションメッセージ(Binary)を種別と主要値に要約して返す。
async fn next_event(c: &mut Client) -> String {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), c.next())
            .await
            .expect("タイムアウト")
            .expect("接続が閉じた")
            .unwrap();
        match msg {
            Message::Binary(b) => {
                let env = ServerEnvelope::parse(&b[..]).unwrap();
                return match env.payload() {
                    server_envelope::PayloadOneof::Joined(m) => format!("joined:{}", m.nickname()),
                    server_envelope::PayloadOneof::Left(m) => format!("left:{}", m.nickname()),
                    server_envelope::PayloadOneof::Chat(m) => {
                        format!("chat:{}:{}", m.nickname(), m.content())
                    }
                    server_envelope::PayloadOneof::Error(e) => format!("error:{}", e.code()),
                    _ => "none".into(),
                };
            }
            Message::Ping(_) | Message::Pong(_) => {}
            other => panic!("想定外のメッセージ: {other:?}"),
        }
    }
}

/// 閉じられるまで読み、Close フレームのコードを返す(Close なしなら None)。
async fn closed_code(c: &mut Client) -> Option<u16> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), c.next())
            .await
            .expect("タイムアウト")
        {
            Some(Ok(Message::Close(frame))) => return frame.map(|f| u16::from(f.code)),
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return None,
        }
    }
}

#[tokio::test]
async fn two_clients_exchange_events() {
    let (addr, _shutdown) = start(ServerConfig::default()).await;
    let mut alice = connect(addr).await;
    let mut bob = connect(addr).await;

    alice.send(join("alice")).await.unwrap();
    assert_eq!(next_event(&mut alice).await, "joined:alice");
    bob.send(join("bob")).await.unwrap();
    assert_eq!(next_event(&mut alice).await, "joined:bob");
    assert_eq!(next_event(&mut bob).await, "joined:bob");

    alice.send(chat("こんにちは")).await.unwrap();
    assert_eq!(next_event(&mut alice).await, "chat:alice:こんにちは");
    assert_eq!(next_event(&mut bob).await, "chat:alice:こんにちは");

    bob.close(None).await.unwrap();
    assert_eq!(next_event(&mut alice).await, "left:bob");
}

#[tokio::test]
async fn invalid_input_does_not_break_the_server() {
    let (addr, _shutdown) = start(ServerConfig::default()).await;
    let mut c = connect(addr).await;

    // 参加前の発言 -> エラーだが接続は維持される
    c.send(chat("hi")).await.unwrap();
    assert_eq!(next_event(&mut c).await, "error:not_joined");

    // 値の検証エラー -> 接続は維持される
    c.send(join("   ")).await.unwrap();
    assert_eq!(next_event(&mut c).await, "error:invalid_nickname");

    c.send(join("alice")).await.unwrap();
    assert_eq!(next_event(&mut c).await, "joined:alice");
    c.send(join("alice2")).await.unwrap();
    assert_eq!(next_event(&mut c).await, "error:already_joined");
    c.send(chat("")).await.unwrap();
    assert_eq!(next_event(&mut c).await, "error:invalid_content");

    // 壊れた Protobuf -> エラーを返して切断(1007)
    c.send(Message::Binary(vec![0x0f].into())).await.unwrap();
    assert_eq!(next_event(&mut c).await, "error:malformed_message");
    assert_eq!(closed_code(&mut c).await, Some(1007));

    // 他の接続は影響を受けず、サーバーは健在
    let mut d = connect(addr).await;
    d.send(join("dave")).await.unwrap();
    assert_eq!(next_event(&mut d).await, "joined:dave");
}

#[tokio::test]
async fn text_frame_is_rejected() {
    let (addr, _shutdown) = start(ServerConfig::default()).await;
    let mut c = connect(addr).await;
    c.send(Message::Text("hello".into())).await.unwrap();
    assert_eq!(next_event(&mut c).await, "error:malformed_message");
    assert_eq!(closed_code(&mut c).await, Some(1003));
}

#[tokio::test]
async fn oversized_frame_closes_only_that_connection() {
    let config = ServerConfig {
        max_message_bytes: 256,
        ..ServerConfig::default()
    };
    let (addr, _shutdown) = start(config).await;
    let mut good = connect(addr).await;
    good.send(join("good")).await.unwrap();
    assert_eq!(next_event(&mut good).await, "joined:good");

    let mut bad = connect(addr).await;
    bad.send(Message::Binary(vec![0u8; 1024].into()))
        .await
        .unwrap();
    // 上限超過の接続は閉じられる。
    let _ = closed_code(&mut bad).await;

    good.send(chat("まだ動く")).await.unwrap();
    assert_eq!(next_event(&mut good).await, "chat:good:まだ動く");
}

#[tokio::test]
async fn slow_receiver_is_disconnected_with_lag() {
    let config = ServerConfig {
        broadcast_capacity: 4,
        ..ServerConfig::default()
    };
    let (addr, _shutdown) = start(config).await;
    let mut sender = connect(addr).await;
    let mut slow = connect(addr).await;
    sender.send(join("sender")).await.unwrap();
    slow.send(join("slow")).await.unwrap();
    assert_eq!(next_event(&mut sender).await, "joined:sender");
    assert_eq!(next_event(&mut sender).await, "joined:slow");

    // sender 自身も配信を受けるので、別タスクで読み捨てて遅れないようにする。
    let (mut sender_tx, mut sender_rx) = sender.split();
    tokio::spawn(async move { while sender_rx.next().await.is_some() {} });

    // slow は読まない。サーバー側で詰まる量を超えて送り続ける。
    let payload = "x".repeat(900);
    for _ in 0..2000 {
        if sender_tx.send(chat(&payload)).await.is_err() {
            break;
        }
    }
    // 読み始めると、いずれ lagged エラー(または切断)で終わる。
    let mut saw_lag = false;
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_secs(5), slow.next()).await {
        match msg {
            Message::Binary(b) => {
                let env = ServerEnvelope::parse(&b[..]).unwrap();
                if matches!(env.payload(), server_envelope::PayloadOneof::Error(e) if e.code() == "lagged")
                {
                    saw_lag = true;
                }
            }
            Message::Close(frame) => {
                assert!(saw_lag, "lagged エラー無しで切断された");
                assert_eq!(frame.map(|f| u16::from(f.code)), Some(1013));
                return;
            }
            _ => {}
        }
    }
    panic!("遅い受信者が切断されなかった");
}

#[tokio::test]
async fn graceful_shutdown_closes_sessions() {
    let (addr, shutdown) = start(ServerConfig::default()).await;
    let mut c = connect(addr).await;
    c.send(join("alice")).await.unwrap();
    assert_eq!(next_event(&mut c).await, "joined:alice");
    shutdown.send(true).unwrap();
    assert_eq!(closed_code(&mut c).await, Some(1001));
}
