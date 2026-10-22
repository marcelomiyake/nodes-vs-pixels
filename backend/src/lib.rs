//! Sign-up service. It validates forms with `form-core` and serves both gRPC
//! and gRPC-web (HTTP/1.1) on a single port.

use std::future::Future;

use form_core::{Field, SignUpForm, ViolationCode};
use tokio::net::TcpListener;
use tonic::transport::Server;
use tonic::transport::server::TcpIncoming;
use tonic::{Request, Response, Status};
use tonic_web::GrpcWebLayer;
use uuid::Uuid;

/// Code generated from `proto/signup/v1/signup.proto`.
#[allow(clippy::all, clippy::pedantic)]
pub mod proto {
    tonic::include_proto!("signup.v1");
}

use proto::signup_service_server::{SignupService, SignupServiceServer};
use proto::{FieldViolation, SignUpRequest, SignUpResponse};

/// The `signup.v1.SignupService` implementation.
#[derive(Debug, Default)]
pub struct Signup;

#[tonic::async_trait]
impl SignupService for Signup {
    async fn sign_up(
        &self,
        request: Request<SignUpRequest>,
    ) -> Result<Response<SignUpResponse>, Status> {
        Ok(Response::new(sign_up(request.into_inner())))
    }
}

/// Validates the request and, when it is valid, issues a new account id.
#[must_use]
pub fn sign_up(request: SignUpRequest) -> SignUpResponse {
    let form = SignUpForm {
        full_name: request.full_name,
        email: request.email,
        password: request.password,
        confirm_password: request.confirm_password,
        accept_terms: request.accept_terms,
    };
    let violations: Vec<FieldViolation> = form
        .validate()
        .into_iter()
        .map(|violation| FieldViolation {
            field: proto_field(violation.field).into(),
            code: proto_code(violation.code).into(),
        })
        .collect();
    let account_id = if violations.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        String::new()
    };
    SignUpResponse {
        account_id,
        violations,
    }
}

const fn proto_field(field: Field) -> proto::Field {
    match field {
        Field::FullName => proto::Field::FullName,
        Field::Email => proto::Field::Email,
        Field::Password => proto::Field::Password,
        Field::ConfirmPassword => proto::Field::ConfirmPassword,
        Field::AcceptTerms => proto::Field::AcceptTerms,
    }
}

const fn proto_code(code: ViolationCode) -> proto::ViolationCode {
    match code {
        ViolationCode::Required => proto::ViolationCode::Required,
        ViolationCode::TooShort => proto::ViolationCode::TooShort,
        ViolationCode::TooLong => proto::ViolationCode::TooLong,
        ViolationCode::InvalidCharacters => proto::ViolationCode::InvalidCharacters,
        ViolationCode::InvalidFormat => proto::ViolationCode::InvalidFormat,
        ViolationCode::Weak => proto::ViolationCode::Weak,
        ViolationCode::Mismatch => proto::ViolationCode::Mismatch,
    }
}

/// Serves gRPC and gRPC-web on `listener` until `shutdown` resolves.
///
/// # Errors
///
/// Returns an error when the server fails to accept or serve connections.
pub async fn serve(
    listener: TcpListener,
    shutdown: impl Future<Output = ()>,
) -> Result<(), tonic::transport::Error> {
    Server::builder()
        .accept_http1(true)
        .layer(GrpcWebLayer::new())
        .add_service(SignupServiceServer::new(Signup))
        .serve_with_incoming_shutdown(TcpIncoming::from(listener), shutdown)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screaming_snake(key: &str) -> String {
        key.chars().fold(String::new(), |mut out, c| {
            if c.is_ascii_uppercase() {
                out.push('_');
            }
            out.push(c.to_ascii_uppercase());
            out
        })
    }

    #[test]
    fn fields_map_to_their_proto_names() {
        for field in Field::ALL {
            let expected = format!("FIELD_{}", screaming_snake(field.key()));
            assert_eq!(proto_field(field).as_str_name(), expected);
        }
    }

    #[test]
    fn codes_map_to_their_proto_names() {
        let codes = [
            ViolationCode::Required,
            ViolationCode::TooShort,
            ViolationCode::TooLong,
            ViolationCode::InvalidCharacters,
            ViolationCode::InvalidFormat,
            ViolationCode::Weak,
            ViolationCode::Mismatch,
        ];
        for code in codes {
            let expected = format!("VIOLATION_CODE_{}", code.key());
            assert_eq!(proto_code(code).as_str_name(), expected);
        }
    }

    #[test]
    fn valid_request_gets_an_account_id() {
        let response = sign_up(SignUpRequest {
            full_name: "Ada Lovelace".into(),
            email: "ada@example.com".into(),
            password: "Analytical#Engine1".into(),
            confirm_password: "Analytical#Engine1".into(),
            accept_terms: true,
        });
        assert_eq!(response.violations, []);
        assert!(Uuid::parse_str(&response.account_id).is_ok());
    }

    #[test]
    fn invalid_request_gets_violations_and_no_account() {
        let response = sign_up(SignUpRequest::default());
        assert_eq!(response.account_id, "");
        assert_eq!(response.violations.len(), Field::ALL.len());
        assert!(
            response
                .violations
                .iter()
                .all(|v| v.code() == proto::ViolationCode::Required)
        );
    }
}
