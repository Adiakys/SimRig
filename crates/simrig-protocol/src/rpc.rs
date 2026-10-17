use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// A JSON-RPC 2.0 message. One type covers requests, notifications and responses; use
/// [`Message::is_request`] and friends to tell them apart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Json>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: impl Serialize) -> Self {
        self.data = serde_json::to_value(data).ok();
        self
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (code {})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

const VERSION: &str = "2.0";

impl Message {
    fn base() -> Self {
        Self {
            jsonrpc: VERSION.into(),
            id: None,
            method: None,
            params: None,
            result: None,
            error: None,
        }
    }

    pub fn request(id: u64, method: &str, params: impl Serialize) -> Self {
        Self {
            id: Some(id),
            method: Some(method.into()),
            params: Some(to_json(params)),
            ..Self::base()
        }
    }

    pub fn notification(method: &str, params: impl Serialize) -> Self {
        Self {
            method: Some(method.into()),
            params: Some(to_json(params)),
            ..Self::base()
        }
    }

    pub fn response(id: u64, result: impl Serialize) -> Self {
        Self {
            id: Some(id),
            result: Some(to_json(result)),
            ..Self::base()
        }
    }

    pub fn error_response(id: Option<u64>, error: RpcError) -> Self {
        Self {
            id,
            error: Some(error),
            ..Self::base()
        }
    }

    pub fn is_request(&self) -> bool {
        self.method.is_some() && self.id.is_some()
    }

    pub fn is_notification(&self) -> bool {
        self.method.is_some() && self.id.is_none()
    }

    pub fn is_response(&self) -> bool {
        self.method.is_none() && self.id.is_some()
    }
}

fn to_json(value: impl Serialize) -> Json {
    serde_json::to_value(value).unwrap_or(Json::Null)
}

/// Reads the next message. Returns `Ok(None)` at end of stream; blank lines are skipped.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Message>> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if !line.trim().is_empty() {
            break;
        }
    }
    serde_json::from_str(&line)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Writes one message as a single line and flushes.
pub fn write_message(writer: &mut impl Write, message: &Message) -> io::Result<()> {
    let mut line = serde_json::to_string(message).map_err(io::Error::other)?;
    line.push('\n');
    writer.write_all(line.as_bytes())?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_roundtrip() {
        let messages = vec![
            Message::request(
                1,
                "std/get",
                serde_json::json!({"component": "a", "item": "rotation"}),
            ),
            Message::notification("input/state", serde_json::json!({})),
            Message::response(1, 42),
            Message::error_response(Some(2), RpcError::new(-32001, "unsupported")),
        ];
        let mut buf = Vec::new();
        for m in &messages {
            write_message(&mut buf, m).unwrap();
        }
        let mut reader = io::Cursor::new(buf);
        for m in &messages {
            assert_eq!(read_message(&mut reader).unwrap().as_ref(), Some(m));
        }
        assert_eq!(read_message(&mut reader).unwrap(), None);
        assert!(messages[0].is_request());
        assert!(messages[1].is_notification());
        assert!(messages[2].is_response());
    }
}
