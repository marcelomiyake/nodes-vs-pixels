//! Sign-up form fields and the validation rules defined in `spec/README.md`.
//!
//! The backend uses these rules as the authority and the WebAssembly frontend
//! uses them for instant feedback, so both always agree.

use std::fmt;

/// A field of the sign-up form, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    FullName,
    Email,
    Password,
    ConfirmPassword,
    AcceptTerms,
}

impl Field {
    /// Every field, in form order.
    pub const ALL: [Self; 5] = [
        Self::FullName,
        Self::Email,
        Self::Password,
        Self::ConfirmPassword,
        Self::AcceptTerms,
    ];

    /// The key used for this field in `spec/copy.json` and `spec/validation-cases.json`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::FullName => "fullName",
            Self::Email => "email",
            Self::Password => "password",
            Self::ConfirmPassword => "confirmPassword",
            Self::AcceptTerms => "acceptTerms",
        }
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Why a field was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViolationCode {
    Required,
    TooShort,
    TooLong,
    InvalidCharacters,
    InvalidFormat,
    Weak,
    Mismatch,
}

impl ViolationCode {
    /// The key used for this code in `spec/copy.json` and `spec/validation-cases.json`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Required => "REQUIRED",
            Self::TooShort => "TOO_SHORT",
            Self::TooLong => "TOO_LONG",
            Self::InvalidCharacters => "INVALID_CHARACTERS",
            Self::InvalidFormat => "INVALID_FORMAT",
            Self::Weak => "WEAK",
            Self::Mismatch => "MISMATCH",
        }
    }
}

impl fmt::Display for ViolationCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// A rejected field and the first rule it failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Violation {
    pub field: Field,
    pub code: ViolationCode,
}

/// The values submitted through the sign-up form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignUpForm {
    pub full_name: String,
    pub email: String,
    pub password: String,
    pub confirm_password: String,
    pub accept_terms: bool,
}

const NAME_MIN: usize = 2;
const NAME_MAX: usize = 80;
const EMAIL_MAX: usize = 254;
const PASSWORD_MIN: usize = 12;
const PASSWORD_MAX: usize = 128;

impl SignUpForm {
    /// Validates every field and returns the violations in form order.
    #[must_use]
    pub fn validate(&self) -> Vec<Violation> {
        Field::ALL
            .into_iter()
            .filter_map(|field| self.check(field).map(|code| Violation { field, code }))
            .collect()
    }

    /// Returns the first rule `field` fails, if any.
    #[must_use]
    pub fn check(&self, field: Field) -> Option<ViolationCode> {
        match field {
            Field::FullName => check_full_name(&self.full_name),
            Field::Email => check_email(&self.email),
            Field::Password => check_password(&self.password),
            Field::ConfirmPassword => check_confirmation(&self.password, &self.confirm_password),
            Field::AcceptTerms => (!self.accept_terms).then_some(ViolationCode::Required),
        }
    }
}

fn check_length(value: &str, min: usize, max: usize) -> Option<ViolationCode> {
    let length = value.chars().count();
    if length == 0 {
        Some(ViolationCode::Required)
    } else if length < min {
        Some(ViolationCode::TooShort)
    } else if length > max {
        Some(ViolationCode::TooLong)
    } else {
        None
    }
}

fn check_full_name(value: &str) -> Option<ViolationCode> {
    let name = value.trim();
    check_length(name, NAME_MIN, NAME_MAX).or_else(|| {
        let allowed = |c: char| c.is_alphabetic() || matches!(c, ' ' | '\'' | '\u{2019}' | '-');
        (!name.chars().all(allowed)).then_some(ViolationCode::InvalidCharacters)
    })
}

fn check_email(value: &str) -> Option<ViolationCode> {
    let email = value.trim();
    check_length(email, 1, EMAIL_MAX)
        .or_else(|| (!is_email_shaped(email)).then_some(ViolationCode::InvalidFormat))
}

fn is_email_shaped(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !email.chars().any(char::is_whitespace)
        && !local.is_empty()
        && !domain.contains('@')
        && domain.split('.').count() >= 2
        && domain.split('.').all(|label| !label.is_empty())
}

fn check_password(value: &str) -> Option<ViolationCode> {
    check_length(value, PASSWORD_MIN, PASSWORD_MAX).or_else(|| {
        let is_symbol = |c: char| !c.is_alphabetic() && !c.is_numeric() && !c.is_whitespace();
        let strong = value.chars().any(char::is_uppercase)
            && value.chars().any(char::is_lowercase)
            && value.chars().any(|c| c.is_ascii_digit())
            && value.chars().any(is_symbol);
        (!strong).then_some(ViolationCode::Weak)
    })
}

fn check_confirmation(password: &str, confirmation: &str) -> Option<ViolationCode> {
    if confirmation.is_empty() {
        Some(ViolationCode::Required)
    } else if confirmation != password {
        Some(ViolationCode::Mismatch)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_match_the_specification() {
        let fields: Vec<_> = Field::ALL.iter().map(ToString::to_string).collect();
        assert_eq!(
            fields,
            [
                "fullName",
                "email",
                "password",
                "confirmPassword",
                "acceptTerms"
            ]
        );
        assert_eq!(
            ViolationCode::InvalidCharacters.to_string(),
            "INVALID_CHARACTERS"
        );
        assert_eq!(ViolationCode::Mismatch.key(), "MISMATCH");
    }

    #[test]
    fn default_form_reports_every_field_as_required() {
        let violations = SignUpForm::default().validate();
        assert_eq!(violations.len(), Field::ALL.len());
        assert!(violations.iter().all(|v| v.code == ViolationCode::Required));
    }

    #[test]
    fn check_reports_a_single_field() {
        let form = SignUpForm {
            email: "ada@example".into(),
            ..SignUpForm::default()
        };
        assert_eq!(form.check(Field::Email), Some(ViolationCode::InvalidFormat));
    }
}
