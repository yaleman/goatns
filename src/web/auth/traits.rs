use axum::response::Redirect;
use openidconnect::EndUserUsername;

use crate::web::utils::Urls;

use super::CustomClaimType;

pub trait CustomClaimTypeThings {
    fn get_displayname(&self) -> String;
    fn get_email(&self) -> Result<String, Redirect>;
    fn get_username(&self) -> String;
}

impl CustomClaimTypeThings for CustomClaimType {
    fn get_email(&self) -> Result<String, Redirect> {
        let email: String;
        if let Some(user_email) = self.email() {
            email = user_email.to_string();
        } else if let Some(user_email) = self.preferred_username() {
            email = user_email.to_string();
        } else {
            tracing::error!("Couldn't extract email address from claim: {self:?}");
            return Err(Urls::Home.redirect());
        }
        Ok(email)
    }
    fn get_displayname(&self) -> String {
        let mut displayname: String = "Anonymous Kid".to_string();
        if let Some(name) = self.name() {
            if let Some(username) = name.iter().next() {
                displayname = username.1.to_string();
            }
        }
        displayname
    }
    fn get_username(&self) -> String {
        let default = EndUserUsername::new("".to_string());
        self.preferred_username().unwrap_or(&default).to_string()
    }
}

#[test]
fn custom_claim_type_things() {
    use url::Url;

    use openidconnect::{
        EmptyAdditionalClaims, EndUserEmail, IssuerUrl, StandardClaims, SubjectIdentifier,
    };
    let cct = CustomClaimType::new(
        IssuerUrl::from_url(Url::parse("https://example.com").expect("Failed to parse URL")),
        vec![],
        chrono::Utc::now(),
        chrono::Utc::now(),
        StandardClaims::new(SubjectIdentifier::new("example_identifier".to_string()))
            .set_email(Some(EndUserEmail::new("billy@goat.net".to_string()))),
        EmptyAdditionalClaims::default(),
    );

    assert_eq!(cct.get_displayname(), "Anonymous Kid".to_string());

    assert_eq!("".to_string(), cct.get_username());

    assert_eq!(
        "billy@goat.net".to_string(),
        cct.get_email().expect("Failed to get email")
    );
}
