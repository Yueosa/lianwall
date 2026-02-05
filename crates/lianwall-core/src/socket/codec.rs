//! 消息编解码器
//!
//! 使用长度前缀帧格式:
//! ```text
//! +----------------+------------------+
//! | 长度 (4 bytes) | JSON 数据        |
//! | u32 big-endian | UTF-8 字符串     |
//! +----------------+------------------+
//! ```

use std::io::{Read, Write};

use super::error::SocketError;
use super::protocol::MAX_MESSAGE_SIZE;

/// 编码消息（添加长度前缀）
pub fn encode<W: Write>(writer: &mut W, data: &[u8]) -> Result<(), SocketError> {
    let len = data.len();

    if len > MAX_MESSAGE_SIZE {
        return Err(SocketError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    // 写入长度前缀 (4 bytes, big-endian)
    let len_bytes = (len as u32).to_be_bytes();
    writer
        .write_all(&len_bytes)
        .map_err(SocketError::SendFailed)?;

    // 写入数据
    writer.write_all(data).map_err(SocketError::SendFailed)?;

    // 确保数据发送
    writer.flush().map_err(SocketError::SendFailed)?;

    Ok(())
}

/// 解码消息（读取长度前缀帧）
pub fn decode<R: Read>(reader: &mut R) -> Result<Vec<u8>, SocketError> {
    // 读取长度前缀
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(SocketError::ConnectionClosed);
        }
        Err(e) => return Err(SocketError::RecvFailed(e)),
    }

    let len = u32::from_be_bytes(len_bytes) as usize;

    // 检查消息大小
    if len > MAX_MESSAGE_SIZE {
        return Err(SocketError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    // 读取数据
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data).map_err(SocketError::RecvFailed)?;

    Ok(data)
}

/// 编码并发送 JSON 消息
pub fn send_json<W: Write, T: serde::Serialize>(
    writer: &mut W,
    value: &T,
) -> Result<(), SocketError> {
    let json = serde_json::to_vec(value).map_err(SocketError::SerializeFailed)?;
    encode(writer, &json)
}

/// 接收并解码 JSON 消息
pub fn recv_json<R: Read, T: serde::de::DeserializeOwned>(reader: &mut R) -> Result<T, SocketError> {
    let data = decode(reader)?;
    serde_json::from_slice(&data).map_err(|e| SocketError::DeserializeFailed {
        context: String::from_utf8_lossy(&data).chars().take(100).collect(),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = b"Hello, World!";
        let mut buffer = Vec::new();

        // 编码
        encode(&mut buffer, original).unwrap();

        // 检查长度前缀
        assert_eq!(buffer.len(), 4 + original.len());
        assert_eq!(&buffer[0..4], &(original.len() as u32).to_be_bytes());

        // 解码
        let mut cursor = Cursor::new(buffer);
        let decoded = decode(&mut cursor).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn test_json_roundtrip() {
        use super::super::protocol::{Request, Response};

        let request = Request::Next;
        let mut buffer = Vec::new();

        // 发送
        send_json(&mut buffer, &request).unwrap();

        // 接收
        let mut cursor = Cursor::new(buffer);
        let received: Request = recv_json(&mut cursor).unwrap();

        assert!(matches!(received, Request::Next));

        // 测试 Response
        let response = Response::ok();
        let mut buffer = Vec::new();

        send_json(&mut buffer, &response).unwrap();

        let mut cursor = Cursor::new(buffer);
        let received: Response = recv_json(&mut cursor).unwrap();

        assert!(received.is_success());
    }

    #[test]
    fn test_message_too_large() {
        let huge_data = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let mut buffer = Vec::new();

        let result = encode(&mut buffer, &huge_data);
        assert!(matches!(result, Err(SocketError::MessageTooLarge { .. })));
    }

    #[test]
    fn test_connection_closed() {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let result = decode(&mut cursor);
        assert!(matches!(result, Err(SocketError::ConnectionClosed)));
    }
}
