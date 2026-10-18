//! Axum の `/ws` エンドポイントと WebSocket セッション管理。
//!
//! 1 接続 = 1 セッション。接続ごとに一時的な接続 ID (UUID v4) を採番し、
//! 認証・永続化はしない。Binary フレームのみ受け付け、Text は拒否する。

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::sync::{broadcast, watch};
use tokio::time::{Instant, MissedTickBehavior, interval};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::adapter::{
    ClientMessage, ErrorCode, ProtocolError, ServerEvent, decode_client, encode_server,
};
use crate::room::{Frame, Room, not_joined};

/// 切断時に相手の Close 応答を待つ最大時間。
const CLOSE_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// 1 メッセージ(フレーム)の最大バイト数。超えた接続は切断される。
    pub max_message_bytes: usize,
    /// ルームの broadcast バッファ件数。超過した遅い受信者は切断される。
    pub broadcast_capacity: usize,
    /// サーバーから Ping を送る間隔。
    pub ping_interval: Duration,
    /// 何も受信しない場合に切断するまでの時間。
    pub idle_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_message_bytes: 16 * 1024,
            broadcast_capacity: 256,
            ping_interval: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(90),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    room: Arc<Room>,
    config: Arc<ServerConfig>,
    shutdown: watch::Receiver<bool>,
}

impl AppState {
    /// `shutdown` が `true` になると全セッションが `Going Away` で閉じる。
    pub fn new(config: ServerConfig, shutdown: watch::Receiver<bool>) -> Self {
        Self {
            room: Arc::new(Room::new(config.broadcast_capacity)),
            config: Arc::new(config),
            shutdown,
        }
    }

    pub fn room(&self) -> &Arc<Room> {
        &self.room
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state)
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    let max = state.config.max_message_bytes;
    ws.max_message_size(max)
        .max_frame_size(max)
        .on_upgrade(move |socket| run_session(socket, state))
}

/// セッションの終了理由。`Close` フレームの内容になる。
enum Exit {
    /// クライアントが閉じた、または接続が切れた。こちらからは何も送らない。
    Disconnected,
    Close(u16, &'static str),
}

async fn run_session(mut socket: WebSocket, state: AppState) {
    let conn_id = Uuid::new_v4().to_string();
    info!(%conn_id, "接続");

    let exit = session_loop(&mut socket, &state, &conn_id).await;

    // 切断・異常終了のいずれでも必ず退出処理を通す。
    state.room.leave(&conn_id);
    if let Exit::Close(code, reason) = exit {
        let sent = socket
            .send(Message::Close(Some(CloseFrame {
                code,
                reason: reason.into(),
            })))
            .await;
        if sent.is_ok() {
            // 相手の Close 応答まで少し待つ。すぐ破棄すると TCP RST で Close フレームが届かないことがある。
            let _ = tokio::time::timeout(CLOSE_HANDSHAKE_TIMEOUT, async {
                while let Some(Ok(_)) = socket.recv().await {}
            })
            .await;
        }
    }
    info!(%conn_id, "切断");
}

async fn session_loop(socket: &mut WebSocket, state: &AppState, conn_id: &str) -> Exit {
    let mut shutdown = state.shutdown.clone();
    let mut room_rx: Option<broadcast::Receiver<Frame>> = None;
    let mut ping = interval(state.config.ping_interval);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(incoming) = incoming else { return Exit::Disconnected };
                let msg = match incoming {
                    Ok(msg) => msg,
                    Err(err) => {
                        // 上限超過・プロトコル違反など。tungstenite 側で切断済みのことが多い。
                        debug!(%conn_id, %err, "受信エラー");
                        return Exit::Disconnected;
                    }
                };
                last_seen = Instant::now();
                match msg {
                    Message::Binary(bytes) => {
                        if let Err(exit) = handle_binary(socket, state, conn_id, &mut room_rx, &bytes).await {
                            return exit;
                        }
                    }
                    Message::Text(_) => {
                        warn!(%conn_id, "Text フレームを拒否");
                        let _ = send_error(socket, ProtocolError::new(
                            ErrorCode::MalformedMessage,
                            "Binary フレームのみ受け付けます",
                        )).await;
                        return Exit::Close(close_code::UNSUPPORTED, "binary only");
                    }
                    // Pong は axum/tungstenite が自動で返す。ここでは生存確認として扱うだけ。
                    Message::Ping(_) | Message::Pong(_) => {}
                    Message::Close(_) => return Exit::Disconnected,
                }
            }
            frame = recv_room(&mut room_rx) => {
                match frame {
                    Ok(frame) => {
                        if socket.send(Message::Binary(frame.to_vec().into())).await.is_err() {
                            return Exit::Disconnected;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(%conn_id, skipped, "受信が遅く配信が欠落したため切断");
                        let _ = send_error(socket, ProtocolError::new(
                            ErrorCode::Lagged,
                            "受信が追いつかないため切断します。再接続してください",
                        )).await;
                        return Exit::Close(close_code::AGAIN, "lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => return Exit::Close(close_code::AWAY, "room closed"),
                }
            }
            _ = ping.tick() => {
                if last_seen.elapsed() > state.config.idle_timeout {
                    info!(%conn_id, "アイドルタイムアウト");
                    return Exit::Close(close_code::POLICY, "idle timeout");
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return Exit::Disconnected;
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return Exit::Close(close_code::AWAY, "server shutting down");
                }
            }
        }
    }
}

/// 未参加の間は pending のままにして `select!` の対象から実質外す。
async fn recv_room(
    rx: &mut Option<broadcast::Receiver<Frame>>,
) -> Result<Frame, broadcast::error::RecvError> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

/// Binary フレーム 1 つを処理する。`Err` は接続を終了すべきことを表す。
async fn handle_binary(
    socket: &mut WebSocket,
    state: &AppState,
    conn_id: &str,
    room_rx: &mut Option<broadcast::Receiver<Frame>>,
    bytes: &[u8],
) -> Result<(), Exit> {
    let message = match decode_client(bytes) {
        Ok(message) => message,
        Err(err) => {
            warn!(%conn_id, code = err.code.as_str(), "不正な入力");
            let fatal = err.code == ErrorCode::MalformedMessage;
            send_error(socket, err).await?;
            // 構造が壊れたフレームを送るクライアントは信頼せず切断する。
            // 値の検証エラーは入力し直せるので接続を維持する。
            return if fatal {
                Err(Exit::Close(close_code::INVALID, "malformed message"))
            } else {
                Ok(())
            };
        }
    };

    let result = match message {
        ClientMessage::Join { nickname } => match state.room.join(conn_id, &nickname) {
            Ok(rx) => {
                *room_rx = Some(rx);
                info!(%conn_id, %nickname, "参加");
                Ok(())
            }
            Err(err) => Err(err),
        },
        ClientMessage::SendChat { content } => {
            if state.room.is_member(conn_id) {
                state.room.chat(conn_id, &content)
            } else {
                Err(not_joined())
            }
        }
    };
    if let Err(err) = result {
        send_error(socket, err).await?;
    }
    Ok(())
}

async fn send_error(socket: &mut WebSocket, err: ProtocolError) -> Result<(), Exit> {
    socket
        .send(Message::Binary(
            encode_server(&ServerEvent::Error(err)).into(),
        ))
        .await
        .map_err(|_| Exit::Disconnected)
}
