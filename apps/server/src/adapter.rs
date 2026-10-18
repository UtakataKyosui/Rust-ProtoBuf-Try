//! Protocol Adapter。Google Protobuf 生成型とドメイン型を分離し、受信入力の検証を担う。
//!
//! 生成型 (`crate::proto`) はこのモジュールの外へ出さない。クライアントから届いた
//! バイト列は [`decode_client`] でドメイン型 [`ClientMessage`] に変換され、その時点で
//! 文字数・空文字が検証済みになる。送信は [`ServerEvent`] を [`encode_server`] で
//! バイト列にする。

use protobuf::{Parse, Serialize};

use crate::proto::{
    ChatMessage, ClientEnvelope, ErrorEvent, MemberEvent, ServerEnvelope, client_envelope,
};

/// ニックネームの最大文字数 (Unicode scalar value 数)。
pub const MAX_NICKNAME_CHARS: usize = 32;
/// 発言本文の最大文字数 (Unicode scalar value 数)。
pub const MAX_CONTENT_CHARS: usize = 1000;

/// 検証済みのクライアント要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    Join { nickname: String },
    SendChat { content: String },
}

/// サーバーが確定させた発言。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub id: String,
    pub sender_id: String,
    pub nickname: String,
    pub content: String,
    pub timestamp_ms: u64,
}

/// クライアントへ送るイベント。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    Joined { sender_id: String, nickname: String },
    Left { sender_id: String, nickname: String },
    Chat(Chat),
    Error(ProtocolError),
}

/// `ErrorEvent` の `code` に対応する機械可読な識別子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Protobuf として解釈できない、または必須の payload がない。
    MalformedMessage,
    InvalidNickname,
    InvalidContent,
    NotJoined,
    AlreadyJoined,
    /// 受信が遅く、配信が間に合わなかった。
    Lagged,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MalformedMessage => "malformed_message",
            Self::InvalidNickname => "invalid_nickname",
            Self::InvalidContent => "invalid_content",
            Self::NotJoined => "not_joined",
            Self::AlreadyJoined => "already_joined",
            Self::Lagged => "lagged",
        }
    }
}

/// クライアントへ返すエラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// 1 フレーム分のバイト列を検証済みの [`ClientMessage`] にする。
pub fn decode_client(bytes: &[u8]) -> Result<ClientMessage, ProtocolError> {
    let envelope = ClientEnvelope::parse(bytes).map_err(|_| {
        ProtocolError::new(
            ErrorCode::MalformedMessage,
            "メッセージを Protobuf として解釈できません",
        )
    })?;
    match envelope.payload() {
        client_envelope::PayloadOneof::Join(join) => Ok(ClientMessage::Join {
            nickname: validate_nickname(&join.nickname().to_string())?,
        }),
        client_envelope::PayloadOneof::SendChat(chat) => Ok(ClientMessage::SendChat {
            content: validate_content(&chat.content().to_string())?,
        }),
        _ => Err(ProtocolError::new(
            ErrorCode::MalformedMessage,
            "payload がありません",
        )),
    }
}

/// ニックネームを検証する。前後の空白は除去し、空・制御文字・長すぎる値は拒否する。
pub fn validate_nickname(raw: &str) -> Result<String, ProtocolError> {
    let nickname = raw.trim();
    if nickname.is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidNickname,
            "ニックネームが空です",
        ));
    }
    if nickname.chars().count() > MAX_NICKNAME_CHARS {
        return Err(ProtocolError::new(
            ErrorCode::InvalidNickname,
            format!("ニックネームは {MAX_NICKNAME_CHARS} 文字以内にしてください"),
        ));
    }
    if nickname.chars().any(char::is_control) {
        return Err(ProtocolError::new(
            ErrorCode::InvalidNickname,
            "ニックネームに制御文字は使えません",
        ));
    }
    Ok(nickname.to_owned())
}

/// 発言本文を検証する。空白のみ・長すぎる値・改行/タブ以外の制御文字は拒否する。
pub fn validate_content(raw: &str) -> Result<String, ProtocolError> {
    if raw.trim().is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidContent,
            "メッセージが空です",
        ));
    }
    if raw.chars().count() > MAX_CONTENT_CHARS {
        return Err(ProtocolError::new(
            ErrorCode::InvalidContent,
            format!("メッセージは {MAX_CONTENT_CHARS} 文字以内にしてください"),
        ));
    }
    if raw
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ProtocolError::new(
            ErrorCode::InvalidContent,
            "メッセージに制御文字は使えません",
        ));
    }
    Ok(raw.to_owned())
}

/// [`ServerEvent`] を WebSocket Binary フレームの中身にする。
pub fn encode_server(event: &ServerEvent) -> Vec<u8> {
    let mut envelope = ServerEnvelope::new();
    match event {
        ServerEvent::Joined {
            sender_id,
            nickname,
        } => envelope.set_joined(member_event(sender_id, nickname)),
        ServerEvent::Left {
            sender_id,
            nickname,
        } => envelope.set_left(member_event(sender_id, nickname)),
        ServerEvent::Chat(chat) => {
            let mut msg = ChatMessage::new();
            msg.set_id(chat.id.as_str());
            msg.set_sender_id(chat.sender_id.as_str());
            msg.set_nickname(chat.nickname.as_str());
            msg.set_content(chat.content.as_str());
            msg.set_timestamp_ms(chat.timestamp_ms);
            envelope.set_chat(msg);
        }
        ServerEvent::Error(err) => {
            let mut ev = ErrorEvent::new();
            ev.set_code(err.code.as_str());
            ev.set_message(err.message.as_str());
            envelope.set_error(ev);
        }
    }
    envelope
        .serialize()
        .expect("ServerEnvelope のシリアライズは失敗しない")
}

fn member_event(sender_id: &str, nickname: &str) -> MemberEvent {
    let mut ev = MemberEvent::new();
    ev.set_sender_id(sender_id);
    ev.set_nickname(nickname);
    ev
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{JoinRequest, SendChatRequest};

    fn join_bytes(nickname: &str) -> Vec<u8> {
        let mut j = JoinRequest::new();
        j.set_nickname(nickname);
        let mut e = ClientEnvelope::new();
        e.set_join(j);
        e.serialize().unwrap()
    }

    fn chat_bytes(content: &str) -> Vec<u8> {
        let mut c = SendChatRequest::new();
        c.set_content(content);
        let mut e = ClientEnvelope::new();
        e.set_send_chat(c);
        e.serialize().unwrap()
    }

    #[test]
    fn decodes_valid_messages() {
        assert_eq!(
            decode_client(&join_bytes("  ゆうき ")).unwrap(),
            ClientMessage::Join {
                nickname: "ゆうき".into()
            }
        );
        assert_eq!(
            decode_client(&chat_bytes("こんにちは\n世界")).unwrap(),
            ClientMessage::SendChat {
                content: "こんにちは\n世界".into()
            }
        );
    }

    #[test]
    fn rejects_malformed_and_empty_payload() {
        for bytes in [&[0x0a, 0x05, 0x41][..], &[0x0f], &[]] {
            let err = decode_client(bytes).unwrap_err();
            assert_eq!(err.code, ErrorCode::MalformedMessage);
        }
    }

    #[test]
    fn rejects_invalid_nickname() {
        for nick in [
            "",
            "   ",
            "a\u{0007}b",
            &"あ".repeat(MAX_NICKNAME_CHARS + 1),
        ] {
            let err = decode_client(&join_bytes(nick)).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidNickname, "{nick:?}");
        }
        assert!(decode_client(&join_bytes(&"あ".repeat(MAX_NICKNAME_CHARS))).is_ok());
    }

    #[test]
    fn rejects_invalid_content() {
        for content in [
            "",
            " \n ",
            "a\u{0000}b",
            &"あ".repeat(MAX_CONTENT_CHARS + 1),
        ] {
            let err = decode_client(&chat_bytes(content)).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidContent, "{content:?}");
        }
        assert!(decode_client(&chat_bytes(&"あ".repeat(MAX_CONTENT_CHARS))).is_ok());
    }

    #[test]
    fn encodes_server_events() {
        let bytes = encode_server(&ServerEvent::Error(ProtocolError::new(
            ErrorCode::NotJoined,
            "先に参加してください",
        )));
        let env = ServerEnvelope::parse(&bytes).unwrap();
        assert_eq!(env.error().code(), "not_joined");

        let bytes = encode_server(&ServerEvent::Chat(Chat {
            id: "m1".into(),
            sender_id: "c1".into(),
            nickname: "alice".into(),
            content: "hi".into(),
            timestamp_ms: 42,
        }));
        let env = ServerEnvelope::parse(&bytes).unwrap();
        assert_eq!(env.chat().content(), "hi");
        assert_eq!(env.chat().timestamp_ms(), 42);
    }
}
