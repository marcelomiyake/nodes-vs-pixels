//! Runs every case from `spec/validation-cases.json` against the Rust rules.

use form_core::{Field, SignUpForm, ViolationCode};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
struct Spec {
    base: Map<String, Value>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    input: Map<String, Value>,
    violations: Vec<Expected>,
}

#[derive(Deserialize, Debug, PartialEq)]
struct Expected {
    field: String,
    code: String,
}

fn form_from(values: &Map<String, Value>) -> SignUpForm {
    let text = |key: &str| values[key].as_str().expect("text field").to_owned();
    SignUpForm {
        full_name: text("fullName"),
        email: text("email"),
        password: text("password"),
        confirm_password: text("confirmPassword"),
        accept_terms: values["acceptTerms"].as_bool().expect("boolean field"),
    }
}

#[test]
fn every_specification_case_passes() {
    let spec: Spec = serde_json::from_str(include_str!("../../../spec/validation-cases.json"))
        .expect("valid specification");
    assert!(!spec.cases.is_empty());

    for case in spec.cases {
        let mut values = spec.base.clone();
        values.extend(case.input);
        let actual: Vec<Expected> = form_from(&values)
            .validate()
            .into_iter()
            .map(|v| Expected {
                field: v.field.key().to_owned(),
                code: v.code.key().to_owned(),
            })
            .collect();
        assert_eq!(actual, case.violations, "case: {}", case.name);
    }
}

#[test]
fn every_field_and_code_has_copy() {
    let copy: Value =
        serde_json::from_str(include_str!("../../../spec/copy.json")).expect("valid copy");
    let codes = [
        ViolationCode::Required,
        ViolationCode::TooShort,
        ViolationCode::TooLong,
        ViolationCode::InvalidCharacters,
        ViolationCode::InvalidFormat,
        ViolationCode::Weak,
        ViolationCode::Mismatch,
    ];
    for field in Field::ALL {
        let messages = copy["violations"][field.key()]
            .as_object()
            .expect("messages for field");
        assert!(
            messages.contains_key(ViolationCode::Required.key()),
            "{field} needs REQUIRED"
        );
        for key in messages.keys() {
            assert!(
                codes.iter().any(|c| c.key() == key),
                "{field} has unknown code {key}"
            );
        }
    }
}
