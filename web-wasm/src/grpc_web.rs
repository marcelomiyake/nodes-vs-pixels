//! Unary gRPC-web framing over plain HTTP: a 5-byte header (flags and length)
//! precedes each message, and the status arrives in a trailer frame or, for
//! trailers-only responses, in the HTTP headers.

use std::fmt;

use prost::Message;

pub const CONTENT_TYPE: &str = "application/grpc-web+proto";

const HEADER_LEN: usize = 5;
const COMPRESSED_FLAG: u8 = 0x01;
const TRAILER_FLAG: u8 = 0x80;

/// Why a call did not produce a response message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
    /// The HTTP request failed or returned a non-200 status.
    Http(u16),
    /// The server answered with a non-OK gRPC status.
    Status { code: u32, message: String },
    /// The response did not follow the gRPC-web protocol.
    Malformed(&'static str),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(status) => write!(f, "HTTP status {status}"),
            Self::Status { code, message } => write!(f, "gRPC status {code}: {message}"),
            Self::Malformed(reason) => write!(f, "malformed gRPC-web response: {reason}"),
        }
    }
}

impl std::error::Error for CallError {}

/// Frames `message` as the body of a gRPC-web request.
///
/// # Panics
///
/// Panics if the encoded message exceeds 4 GiB, the frame length limit.
#[must_use]
pub fn encode(message: &impl Message) -> Vec<u8> {
    let payload = message.encode_to_vec();
    let length = u32::try_from(payload.len()).expect("messages are far below 4 GiB");
    let mut body = Vec::with_capacity(HEADER_LEN + payload.len());
    body.push(0);
    body.extend_from_slice(&length.to_be_bytes());
    body.extend_from_slice(&payload);
    body
}

/// The parts of a gRPC-web HTTP response that decoding needs.
#[derive(Clone, Copy, Debug)]
pub struct RawResponse<'a> {
    pub http_status: u16,
    /// The `grpc-status` header, present on trailers-only responses.
    pub grpc_status: Option<&'a str>,
    /// The `grpc-message` header, present on trailers-only responses.
    pub grpc_message: Option<&'a str>,
    pub body: &'a [u8],
}

/// Decodes the single message of a unary gRPC-web response.
///
/// # Errors
///
/// Returns a [`CallError`] for HTTP failures, non-OK gRPC statuses and
/// responses that break the gRPC-web framing.
pub fn decode<M: Message + Default>(response: RawResponse<'_>) -> Result<M, CallError> {
    if response.http_status != 200 {
        return Err(CallError::Http(response.http_status));
    }
    let mut payload = None;
    let mut status = response.grpc_status.map(str::to_owned);
    let mut message = response.grpc_message.unwrap_or_default().to_owned();
    let mut rest = response.body;
    while !rest.is_empty() {
        let (flags, frame, remaining) = split_frame(rest)?;
        rest = remaining;
        if flags & COMPRESSED_FLAG != 0 {
            return Err(CallError::Malformed("compressed frames are not supported"));
        }
        if flags & TRAILER_FLAG == 0 {
            if payload.replace(frame).is_some() {
                return Err(CallError::Malformed("unary response with several messages"));
            }
            continue;
        }
        if !rest.is_empty() {
            return Err(CallError::Malformed("frames after the trailers"));
        }
        for (name, value) in trailers(frame) {
            match name.as_str() {
                "grpc-status" => status = Some(value),
                "grpc-message" => message = value,
                _ => {}
            }
        }
    }
    let code = status
        .ok_or(CallError::Malformed("missing grpc-status"))?
        .parse::<u32>()
        .map_err(|_| CallError::Malformed("invalid grpc-status"))?;
    if code != 0 {
        let message = percent_decode(&message);
        return Err(CallError::Status { code, message });
    }
    let payload = payload.ok_or(CallError::Malformed("missing response message"))?;
    M::decode(payload).map_err(|_| CallError::Malformed("invalid response message"))
}

fn split_frame(bytes: &[u8]) -> Result<(u8, &[u8], &[u8]), CallError> {
    let truncated = CallError::Malformed("truncated frame");
    let (header, rest) = bytes
        .split_at_checked(HEADER_LEN)
        .ok_or(truncated.clone())?;
    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]);
    let length = usize::try_from(length).map_err(|_| truncated.clone())?;
    let (frame, rest) = rest.split_at_checked(length).ok_or(truncated)?;
    Ok((header[0], frame, rest))
}

/// Decodes the percent-encoding gRPC applies to `grpc-message`.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = bytes
            .get(index + 1..index + 3)
            .filter(|hex| bytes[index] == b'%' && hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        if let Some(byte) = escaped {
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Parses `name: value` lines separated by CRLF; names are case-insensitive.
fn trailers(frame: &[u8]) -> impl Iterator<Item = (String, String)> + '_ {
    String::from_utf8_lossy(frame)
        .split("\r\n")
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{FieldViolation, SignUpRequest, SignUpResponse};

    fn frame(flags: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![flags];
        bytes.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn ok_response(body: &[u8]) -> RawResponse<'_> {
        RawResponse {
            http_status: 200,
            grpc_status: None,
            grpc_message: None,
            body,
        }
    }

    fn response_message() -> SignUpResponse {
        SignUpResponse {
            account_id: String::new(),
            violations: vec![FieldViolation { field: 2, code: 5 }],
        }
    }

    #[test]
    fn encodes_a_single_uncompressed_frame() {
        let request = SignUpRequest {
            full_name: "Ada".into(),
            ..SignUpRequest::default()
        };
        let body = encode(&request);
        assert_eq!(body[..5], [0, 0, 0, 0, 5]);
        assert_eq!(SignUpRequest::decode(&body[5..]).unwrap(), request);
    }

    #[test]
    fn decodes_a_message_followed_by_trailers() {
        let mut body = frame(0, &response_message().encode_to_vec());
        body.extend(frame(TRAILER_FLAG, b"grpc-status:0\r\ngrpc-message:\r\n"));
        assert_eq!(
            decode::<SignUpResponse>(ok_response(&body)),
            Ok(response_message())
        );
    }

    #[test]
    fn decodes_an_empty_message() {
        let mut body = frame(0, &[]);
        body.extend(frame(TRAILER_FLAG, b"Grpc-Status: 0\r\n"));
        assert_eq!(
            decode::<SignUpResponse>(ok_response(&body)),
            Ok(SignUpResponse::default())
        );
    }

    #[test]
    fn reports_http_failures() {
        let response = RawResponse {
            http_status: 502,
            ..ok_response(&[])
        };
        assert_eq!(
            decode::<SignUpResponse>(response),
            Err(CallError::Http(502))
        );
    }

    #[test]
    fn reports_trailers_only_errors_from_headers() {
        let response = RawResponse {
            grpc_status: Some("13"),
            grpc_message: Some("boom"),
            ..ok_response(&[])
        };
        let expected = CallError::Status {
            code: 13,
            message: "boom".into(),
        };
        assert_eq!(decode::<SignUpResponse>(response), Err(expected));
    }

    #[test]
    fn reports_errors_from_trailer_frames() {
        let body = frame(
            TRAILER_FLAG,
            b"grpc-status: 3\r\ngrpc-message: bad\r\nother: x",
        );
        let expected = CallError::Status {
            code: 3,
            message: "bad".into(),
        };
        assert_eq!(decode::<SignUpResponse>(ok_response(&body)), Err(expected));
    }

    #[test]
    fn percent_decodes_error_messages() {
        let response = RawResponse {
            grpc_status: Some("3"),
            grpc_message: Some("bad%20request%3A%20caf%C3%A9 100%"),
            ..ok_response(&[])
        };
        let expected = CallError::Status {
            code: 3,
            message: "bad request: café 100%".into(),
        };
        assert_eq!(decode::<SignUpResponse>(response), Err(expected));
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode("bad%+Arequest"), "bad%+Arequest");
    }

    #[test]
    fn rejects_protocol_violations() {
        let malformed = |body: &[u8]| match decode::<SignUpResponse>(ok_response(body)) {
            Err(CallError::Malformed(reason)) => reason,
            other => panic!("expected a malformed response, got {other:?}"),
        };
        let trailers = frame(TRAILER_FLAG, b"grpc-status:0");
        assert_eq!(malformed(&[0, 0, 0]), "truncated frame");
        assert_eq!(malformed(&[0, 0, 0, 0, 9, 1]), "truncated frame");
        assert_eq!(
            malformed(&frame(COMPRESSED_FLAG, b"x")),
            "compressed frames are not supported"
        );
        assert_eq!(malformed(&frame(0, &[])), "missing grpc-status");
        assert_eq!(
            malformed(&frame(TRAILER_FLAG, b"grpc-status: x")),
            "invalid grpc-status"
        );
        assert_eq!(malformed(&trailers), "missing response message");
        let mut garbage = frame(0, &[0xff, 0xff]);
        garbage.extend(&trailers);
        assert_eq!(malformed(&garbage), "invalid response message");
        let mut twice = frame(0, &response_message().encode_to_vec());
        twice.extend(frame(0, &SignUpResponse::default().encode_to_vec()));
        twice.extend(&trailers);
        assert_eq!(malformed(&twice), "unary response with several messages");
        let mut masked = frame(TRAILER_FLAG, b"grpc-status:13");
        masked.extend(&trailers);
        assert_eq!(malformed(&masked), "frames after the trailers");
    }

    #[test]
    fn errors_describe_themselves() {
        assert_eq!(CallError::Http(500).to_string(), "HTTP status 500");
        assert_eq!(
            CallError::Status {
                code: 2,
                message: "x".into()
            }
            .to_string(),
            "gRPC status 2: x"
        );
        assert_eq!(
            CallError::Malformed("truncated frame").to_string(),
            "malformed gRPC-web response: truncated frame"
        );
    }
}
