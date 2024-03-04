// use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    paths(
        super::auth::login,
    ),
    components(
        schemas(
            super::auth::AuthPayload,
            super::auth::AuthResponse,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Authentication", description = "Authentication-related tasks"),
        (name = "Records", description = "DNS Record operations"),
        (name = "Zones", description = "DNS Zone operations"),
    )
)]
pub(crate) struct ApiDoc;

pub(crate) struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, _openapi: &mut utoipa::openapi::OpenApi) {
        // if let Some(components) = openapi.components.as_mut() {
        //     components.add_security_scheme(
        //         "api_key",
        //         SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("todo_apikey"))),
        //     )
        // }
    }
}
