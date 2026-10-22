//! Form state with the same behavior as the React app's `useSignUpForm`. A
//! field shows its error once it has been blurred or the form was submitted.
//! Server violations override the client check until that field changes.
//! Values are locked while a submission is pending.

use std::collections::{HashMap, HashSet};

use form_core::{Field, SignUpForm, ViolationCode};

use crate::copy;
use crate::grpc_web::CallError;
use crate::proto::{self, SignUpRequest, SignUpResponse};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Editing,
    Submitting,
    Success,
    Failed,
}

/// What to do after the user asks to submit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Submission {
    /// Client validation failed; focus this field.
    Invalid(Field),
    /// Send this request to the backend.
    Send(SignUpRequest),
}

#[derive(Debug)]
pub struct FormState {
    values: SignUpForm,
    touched: HashSet<Field>,
    server_errors: HashMap<Field, ViolationCode>,
    status: Status,
}

impl Default for FormState {
    fn default() -> Self {
        Self {
            values: SignUpForm::default(),
            touched: HashSet::new(),
            server_errors: HashMap::new(),
            status: Status::Editing,
        }
    }
}

impl FormState {
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    #[must_use]
    pub fn locked(&self) -> bool {
        self.status == Status::Submitting
    }

    #[must_use]
    pub const fn values(&self) -> &SignUpForm {
        &self.values
    }

    /// The current value of a text field; empty for the checkbox.
    #[must_use]
    pub fn text(&self, field: Field) -> &str {
        match field {
            Field::FullName => &self.values.full_name,
            Field::Email => &self.values.email,
            Field::Password => &self.values.password,
            Field::ConfirmPassword => &self.values.confirm_password,
            Field::AcceptTerms => "",
        }
    }

    #[must_use]
    pub fn error(&self, field: Field) -> Option<ViolationCode> {
        if !self.touched.contains(&field) {
            return None;
        }
        self.server_errors
            .get(&field)
            .copied()
            .or_else(|| self.values.check(field))
    }

    /// The error message for `field`, or its hint when it has no error.
    #[must_use]
    pub fn message(&self, field: Field) -> Option<&'static str> {
        match self.error(field) {
            Some(code) => Some(copy::violation(field.key(), code.key()).unwrap_or(code.key())),
            None => copy::field(field.key()).and_then(|copy| copy.hint),
        }
    }

    /// Updates a text field; returns whether the value was accepted.
    pub fn set_text(&mut self, field: Field, value: &str) -> bool {
        if self.locked() {
            return false;
        }
        let slot = match field {
            Field::FullName => &mut self.values.full_name,
            Field::Email => &mut self.values.email,
            Field::Password => &mut self.values.password,
            Field::ConfirmPassword => &mut self.values.confirm_password,
            Field::AcceptTerms => return false,
        };
        value.clone_into(slot);
        self.server_errors.remove(&field);
        true
    }

    /// Updates the terms checkbox; returns whether the value was accepted.
    pub fn set_accept_terms(&mut self, accepted: bool) -> bool {
        if self.locked() {
            return false;
        }
        self.values.accept_terms = accepted;
        self.server_errors.remove(&Field::AcceptTerms);
        true
    }

    pub fn blur(&mut self, field: Field) {
        self.touched.insert(field);
    }

    /// Validates everything and either starts sending or names the field to focus.
    pub fn begin_submit(&mut self) -> Submission {
        self.touched.extend(Field::ALL);
        if let Some(first) = self.values.validate().first() {
            return Submission::Invalid(first.field);
        }
        self.status = Status::Submitting;
        Submission::Send(SignUpRequest {
            full_name: self.values.full_name.clone(),
            email: self.values.email.clone(),
            password: self.values.password.clone(),
            confirm_password: self.values.confirm_password.clone(),
            accept_terms: self.values.accept_terms,
        })
    }

    /// Applies the backend's answer and returns the field to focus, if any.
    pub fn finish_submit(&mut self, result: Result<SignUpResponse, CallError>) -> Option<Field> {
        let Ok(response) = result else {
            self.status = Status::Failed;
            return None;
        };
        if response.violations.is_empty() {
            self.status = Status::Success;
            return None;
        }
        self.server_errors = response
            .violations
            .iter()
            .filter_map(|violation| {
                Some((field_from(violation.field)?, code_from(violation.code)?))
            })
            .collect();
        let first = Field::ALL
            .into_iter()
            .find(|field| self.server_errors.contains_key(field));
        self.status = if first.is_some() {
            Status::Editing
        } else {
            Status::Failed
        };
        first
    }

    /// The trimmed name shown on the success panel.
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.values.full_name.trim()
    }
}

fn field_from(value: i32) -> Option<Field> {
    match proto::Field::try_from(value).ok()? {
        proto::Field::Unspecified => None,
        proto::Field::FullName => Some(Field::FullName),
        proto::Field::Email => Some(Field::Email),
        proto::Field::Password => Some(Field::Password),
        proto::Field::ConfirmPassword => Some(Field::ConfirmPassword),
        proto::Field::AcceptTerms => Some(Field::AcceptTerms),
    }
}

fn code_from(value: i32) -> Option<ViolationCode> {
    match proto::ViolationCode::try_from(value).ok()? {
        proto::ViolationCode::Unspecified => None,
        proto::ViolationCode::Required => Some(ViolationCode::Required),
        proto::ViolationCode::TooShort => Some(ViolationCode::TooShort),
        proto::ViolationCode::TooLong => Some(ViolationCode::TooLong),
        proto::ViolationCode::InvalidCharacters => Some(ViolationCode::InvalidCharacters),
        proto::ViolationCode::InvalidFormat => Some(ViolationCode::InvalidFormat),
        proto::ViolationCode::Weak => Some(ViolationCode::Weak),
        proto::ViolationCode::Mismatch => Some(ViolationCode::Mismatch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::FieldViolation;

    fn filled() -> FormState {
        let mut state = FormState::default();
        state.set_text(Field::FullName, "  Ada Lovelace ");
        state.set_text(Field::Email, "ada@example.com");
        state.set_text(Field::Password, "Analytical#Engine1");
        state.set_text(Field::ConfirmPassword, "Analytical#Engine1");
        state.set_accept_terms(true);
        state
    }

    fn violation(field: proto::Field, code: proto::ViolationCode) -> FieldViolation {
        FieldViolation {
            field: field.into(),
            code: code.into(),
        }
    }

    #[test]
    fn shows_the_hint_until_a_field_is_touched() {
        let mut state = FormState::default();
        assert_eq!(
            state.message(Field::Password),
            copy::field("password").unwrap().hint
        );
        assert_eq!(state.message(Field::FullName), None);

        state.set_text(Field::FullName, "A");
        assert_eq!(state.error(Field::FullName), None);
        state.blur(Field::FullName);

        assert_eq!(state.error(Field::FullName), Some(ViolationCode::TooShort));
        assert_eq!(
            state.message(Field::FullName),
            Some("Name must be at least 2 characters.")
        );
    }

    #[test]
    fn invalid_submit_touches_everything_and_names_the_first_field() {
        let mut state = FormState::default();
        assert_eq!(state.begin_submit(), Submission::Invalid(Field::FullName));
        assert_eq!(state.status(), Status::Editing);
        assert_eq!(
            state.message(Field::AcceptTerms),
            Some("Accept the terms to continue.")
        );
    }

    #[test]
    fn valid_submit_locks_values_until_success() {
        let mut state = filled();
        let Submission::Send(request) = state.begin_submit() else {
            panic!("expected a request");
        };
        assert_eq!(request.full_name, "  Ada Lovelace ");
        assert!(request.accept_terms);
        assert!(state.locked());
        assert!(!state.set_text(Field::FullName, "x"));
        assert!(!state.set_accept_terms(false));
        assert_eq!(state.text(Field::FullName), "  Ada Lovelace ");

        assert_eq!(state.finish_submit(Ok(SignUpResponse::default())), None);
        assert_eq!(state.status(), Status::Success);
        assert_eq!(state.display_name(), "Ada Lovelace");
    }

    #[test]
    fn server_violations_show_until_the_field_changes() {
        let mut state = filled();
        state.begin_submit();
        let response = SignUpResponse {
            account_id: String::new(),
            violations: vec![
                violation(proto::Field::Unspecified, proto::ViolationCode::Required),
                violation(proto::Field::Email, proto::ViolationCode::InvalidFormat),
            ],
        };

        assert_eq!(state.finish_submit(Ok(response)), Some(Field::Email));
        assert_eq!(state.status(), Status::Editing);
        assert_eq!(
            state.error(Field::Email),
            Some(ViolationCode::InvalidFormat)
        );

        state.set_text(Field::Email, "ada@example.org");
        assert_eq!(state.error(Field::Email), None);
    }

    #[test]
    fn unusable_or_failed_responses_mark_the_submission_failed() {
        let mut state = filled();
        state.begin_submit();
        let unknown = SignUpResponse {
            account_id: String::new(),
            violations: vec![violation(
                proto::Field::Password,
                proto::ViolationCode::Unspecified,
            )],
        };
        assert_eq!(state.finish_submit(Ok(unknown)), None);
        assert_eq!(state.status(), Status::Failed);

        state.begin_submit();
        assert_eq!(state.finish_submit(Err(CallError::Http(503))), None);
        assert_eq!(state.status(), Status::Failed);
        assert!(state.set_text(Field::FullName, "Ada"));
    }

    #[test]
    fn maps_every_proto_value() {
        for field in Field::ALL {
            let proto_value =
                i32::try_from(Field::ALL.iter().position(|f| *f == field).unwrap() + 1).unwrap();
            assert_eq!(field_from(proto_value), Some(field));
        }
        assert_eq!(field_from(99), None);
        let codes = [
            ViolationCode::Required,
            ViolationCode::TooShort,
            ViolationCode::TooLong,
            ViolationCode::InvalidCharacters,
            ViolationCode::InvalidFormat,
            ViolationCode::Weak,
            ViolationCode::Mismatch,
        ];
        for (index, code) in codes.into_iter().enumerate() {
            assert_eq!(code_from(i32::try_from(index + 1).unwrap()), Some(code));
        }
        assert_eq!(code_from(0), None);
    }

    #[test]
    fn checkbox_and_server_messages_fall_back_sensibly() {
        let mut state = FormState::default();
        assert!(!state.set_text(Field::AcceptTerms, "x"));
        assert_eq!(state.text(Field::AcceptTerms), "");
        state
            .server_errors
            .insert(Field::AcceptTerms, ViolationCode::Weak);
        state.blur(Field::AcceptTerms);
        assert_eq!(state.message(Field::AcceptTerms), Some("WEAK"));
        assert!(!state.values().accept_terms);
    }
}
