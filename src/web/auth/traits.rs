use axum::response::Redirect;
use openidconnect::EndUserUsername;

use crate::web::utils::redirect_to_home;

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
            log::error!("Couldn't extract email address from claim: {self:?}");
            return Err(redirect_to_home());
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
