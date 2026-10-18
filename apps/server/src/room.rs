//! インメモリの単一公開ルーム。Tokio の有界 broadcast で全参加者へ配信する。
//!
//! 参加者の登録と配信は同じ `Mutex` の下で行い、参加直後の自分への `Joined` 通知と
//! 他参加者への通知の順序が入れ替わらないようにする。ロック中に `await` はしない。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::adapter::{Chat, ErrorCode, ProtocolError, ServerEvent, encode_server};

/// 配信されるフレーム。エンコードは 1 回で、全受信者が共有する。
pub type Frame = Arc<[u8]>;

pub struct Room {
    tx: broadcast::Sender<Frame>,
    /// 接続 ID -> ニックネーム。参加済みの接続だけが入る。
    members: Mutex<HashMap<String, String>>,
}

impl Room {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            members: Mutex::new(HashMap::new()),
        }
    }

    fn members(&self) -> MutexGuard<'_, HashMap<String, String>> {
        // 毒化しても整合性を壊す操作はないので続行する。
        self.members.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn member_count(&self) -> usize {
        self.members().len()
    }

    /// 参加する。購読を開始してから `Joined` を配信するので、参加者本人にも届く。
    pub fn join(
        &self,
        conn_id: &str,
        nickname: &str,
    ) -> Result<broadcast::Receiver<Frame>, ProtocolError> {
        let mut members = self.members();
        if members.contains_key(conn_id) {
            return Err(ProtocolError::new(
                ErrorCode::AlreadyJoined,
                "すでに参加しています",
            ));
        }
        let rx = self.tx.subscribe();
        members.insert(conn_id.to_owned(), nickname.to_owned());
        self.publish(&ServerEvent::Joined {
            sender_id: conn_id.to_owned(),
            nickname: nickname.to_owned(),
        });
        Ok(rx)
    }

    /// 発言を配信する。送信者・ID・時刻はサーバーが付与する。
    pub fn chat(&self, conn_id: &str, content: &str) -> Result<(), ProtocolError> {
        let members = self.members();
        let Some(nickname) = members.get(conn_id) else {
            return Err(not_joined());
        };
        self.publish(&ServerEvent::Chat(Chat {
            id: Uuid::new_v4().to_string(),
            sender_id: conn_id.to_owned(),
            nickname: nickname.clone(),
            content: content.to_owned(),
            timestamp_ms: now_ms(),
        }));
        Ok(())
    }

    /// 退出する。未参加の接続なら何もしない(切断時に無条件で呼べる)。
    pub fn leave(&self, conn_id: &str) {
        let mut members = self.members();
        if let Some(nickname) = members.remove(conn_id) {
            self.publish(&ServerEvent::Left {
                sender_id: conn_id.to_owned(),
                nickname,
            });
        }
    }

    pub fn is_member(&self, conn_id: &str) -> bool {
        self.members().contains_key(conn_id)
    }

    fn publish(&self, event: &ServerEvent) {
        let frame: Frame = encode_server(event).into();
        // 受信者がいなければ Err になるが、無視してよい。
        let _ = self.tx.send(frame);
    }
}

pub fn not_joined() -> ProtocolError {
    ProtocolError::new(ErrorCode::NotJoined, "先に参加してください")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use protobuf::Parse;

    use super::*;
    use crate::proto::{ServerEnvelope, server_envelope};

    fn kind(frame: &Frame) -> &'static str {
        let env = ServerEnvelope::parse(frame).unwrap();
        match env.payload() {
            server_envelope::PayloadOneof::Joined(_) => "joined",
            server_envelope::PayloadOneof::Left(_) => "left",
            server_envelope::PayloadOneof::Chat(_) => "chat",
            server_envelope::PayloadOneof::Error(_) => "error",
            _ => "none",
        }
    }

    #[tokio::test]
    async fn join_chat_leave_are_broadcast_to_members() {
        let room = Room::new(16);
        let mut a = room.join("a", "alice").unwrap();
        let mut b = room.join("b", "bob").unwrap();
        room.chat("a", "hi").unwrap();
        room.leave("b");

        // alice: 自分の join, bob の join, chat, bob の left
        for expected in ["joined", "joined", "chat", "left"] {
            assert_eq!(kind(&a.recv().await.unwrap()), expected);
        }
        // bob: 自分の join より前のイベントは受け取らない
        for expected in ["joined", "chat", "left"] {
            assert_eq!(kind(&b.recv().await.unwrap()), expected);
        }
        assert_eq!(room.member_count(), 1);
    }

    #[tokio::test]
    async fn non_members_cannot_chat_and_leave_is_idempotent() {
        let room = Room::new(16);
        assert_eq!(room.chat("x", "hi").unwrap_err().code, ErrorCode::NotJoined);
        room.leave("x");
        let _rx = room.join("a", "alice").unwrap();
        assert_eq!(
            room.join("a", "again").unwrap_err().code,
            ErrorCode::AlreadyJoined
        );
        room.leave("a");
        room.leave("a");
        assert_eq!(room.member_count(), 0);
    }

    #[tokio::test]
    async fn slow_receiver_lags() {
        let room = Room::new(2);
        let mut slow = room.join("a", "alice").unwrap();
        for _ in 0..5 {
            room.chat("a", "x").unwrap();
        }
        assert!(matches!(
            slow.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
    }
}
