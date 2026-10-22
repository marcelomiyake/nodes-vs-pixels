//! The sign-up form painted on a `<canvas>`. Everything visible is drawn by
//! this crate; transparent native controls laid over the canvas provide
//! focus, typing, autofill and the accessibility tree.

pub mod app;
pub mod backspace;
pub mod editing;
pub mod form;
pub mod grpc_web;
pub mod input;
pub mod layout;
pub mod overlay;
pub mod scene;
pub mod style;
pub mod text;
#[cfg(target_arch = "wasm32")]
mod web;

/// Messages generated from `proto/signup/v1/signup.proto`.
#[allow(clippy::all, clippy::pedantic)]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/signup.v1.rs"));
}

/// Constants generated from `design/tokens.json`.
pub mod tokens {
    include!(concat!(env!("OUT_DIR"), "/tokens.rs"));
}

/// Strings generated from `spec/copy.json`.
pub mod copy {
    include!(concat!(env!("OUT_DIR"), "/copy.rs"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use form_core::{Field, ViolationCode};

    #[test]
    fn tokens_are_generated() {
        assert_eq!(tokens::color::BRAND, "#4f46e5");
        assert_eq!(tokens::spacing::CONTROL, 44.0);
        assert_eq!(tokens::container::CARD, 448.0);
        assert_eq!(tokens::shadow::CARD.blur, 24.0);
        assert_eq!(
            tokens::text::TITLE.font(),
            "600 24px Inter, ui-sans-serif, system-ui, sans-serif"
        );
    }

    #[test]
    fn copy_is_generated() {
        assert_eq!(copy::TITLE, "Create your account");
        assert_eq!(
            copy::NETWORK_ERROR,
            "We couldn't reach the server. Please try again."
        );
        let password = copy::field("password").expect("password copy");
        assert_eq!(password.input_type, "password");
        assert!(password.hint.is_some());
        assert_eq!(copy::field("acceptTerms"), None);
        assert_eq!(
            copy::violation(Field::ConfirmPassword.key(), ViolationCode::Mismatch.key()),
            Some("Passwords don't match.")
        );
        assert_eq!(copy::violation("acceptTerms", "WEAK"), None);
    }

    #[test]
    fn static_page_matches_the_tokens() {
        let page = include_str!("../static/index.html");
        assert!(page.contains(&format!("background:{}", tokens::color::PAGE)));
        assert!(page.contains(&format!("/fonts/{}", tokens::FONT_FILE)));
        assert!(page.contains("<title>"));
    }
}
