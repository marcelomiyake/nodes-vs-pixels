//! Exercises the running server over native gRPC and over gRPC-web on HTTP/1.1,
//! the transport both frontends use through the nginx proxy.

use std::net::SocketAddr;

use backend::proto::signup_service_client::SignupServiceClient;
use backend::proto::{Field, SignUpRequest, SignUpResponse, ViolationCode};
use bytes::{Buf, Bytes};
use http_body_util::{BodyExt, Full};
use hyper::{HeaderMap, Request};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use prost::Message;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const SIGN_UP_PATH: &str = "/signup.v1.SignupService/SignUp";
const TRAILER_FLAG: u8 = 0x80;

struct TestServer {
    addr: SocketAddr,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<(), tonic::transport::Error>>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("address");
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(backend::serve(listener, async {
            stopped.await.ok();
        }));
        Self { addr, stop, task }
    }

    async fn shutdown(self) {
        self.stop.send(()).expect("server running");
        self.task.await.expect("join").expect("clean shutdown");
    }
}

fn valid_request() -> SignUpRequest {
    SignUpRequest {
        full_name: "Ada Lovelace".into(),
        email: "ada@example.com".into(),
        password: "Analytical#Engine1".into(),
        confirm_password: "Analytical#Engine1".into(),
        accept_terms: true,
    }
}

/// Wraps a message in a single gRPC-web data frame.
fn data_frame(message: &impl Message) -> Bytes {
    let payload = message.encode_to_vec();
    let mut frame = vec![0];
    frame.extend_from_slice(&u32::try_from(payload.len()).expect("small").to_be_bytes());
    frame.extend_from_slice(&payload);
    frame.into()
}

/// Splits a gRPC-web response body into its message and its trailer block.
fn split_frames(mut body: Bytes) -> (Option<SignUpResponse>, String) {
    let mut message = None;
    let mut trailers = String::new();
    while body.has_remaining() {
        let flag = body.get_u8();
        let length = usize::try_from(body.get_u32()).expect("length");
        let payload = body.split_to(length);
        if flag & TRAILER_FLAG == 0 {
            message = Some(SignUpResponse::decode(payload).expect("message"));
        } else {
            trailers = String::from_utf8(payload.to_vec()).expect("utf-8 trailers");
        }
    }
    (message, trailers)
}

async fn grpc_web_call(addr: SocketAddr, body: Bytes) -> (u16, HeaderMap, Bytes) {
    let client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();
    let request = Request::post(format!("http://{addr}{SIGN_UP_PATH}"))
        .header("content-type", "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .body(Full::new(body))
        .expect("request");
    let response = client.request(request).await.expect("response");
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, headers, body)
}

#[tokio::test]
async fn native_grpc_accepts_a_valid_form() {
    let server = TestServer::start().await;
    let mut client = SignupServiceClient::connect(format!("http://{}", server.addr))
        .await
        .expect("connect");

    let response = client
        .sign_up(valid_request())
        .await
        .expect("call")
        .into_inner();

    assert_eq!(response.violations, []);
    assert_eq!(response.account_id.len(), 36);
    server.shutdown().await;
}

#[tokio::test]
async fn native_grpc_reports_violations() {
    let server = TestServer::start().await;
    let mut client = SignupServiceClient::connect(format!("http://{}", server.addr))
        .await
        .expect("connect");
    let request = SignUpRequest {
        email: "ada@example".into(),
        confirm_password: "different".into(),
        ..valid_request()
    };

    let response = client.sign_up(request).await.expect("call").into_inner();

    let violations: Vec<_> = response
        .violations
        .iter()
        .map(|v| (v.field(), v.code()))
        .collect();
    assert_eq!(
        violations,
        [
            (Field::Email, ViolationCode::InvalidFormat),
            (Field::ConfirmPassword, ViolationCode::Mismatch),
        ]
    );
    assert_eq!(response.account_id, "");
    server.shutdown().await;
}

#[tokio::test]
async fn grpc_web_over_http1_round_trips() {
    let server = TestServer::start().await;

    let (status, headers, body) = grpc_web_call(server.addr, data_frame(&valid_request())).await;

    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/grpc-web+proto");
    let (message, trailers) = split_frames(body);
    assert_eq!(message.expect("message").account_id.len(), 36);
    assert!(trailers.contains("grpc-status:0"), "trailers: {trailers}");
    server.shutdown().await;
}

#[tokio::test]
async fn grpc_web_rejects_a_malformed_message() {
    let server = TestServer::start().await;

    let malformed = Bytes::from_static(&[0, 0, 0, 0, 2, 0xff, 0xff]);
    let (status, headers, body) = grpc_web_call(server.addr, malformed).await;

    // Errors come back as a trailers-only response: the status is in the headers.
    assert_eq!(status, 200);
    assert_eq!(headers["grpc-status"], "13");
    assert_eq!(body, Bytes::new());
    server.shutdown().await;
}
