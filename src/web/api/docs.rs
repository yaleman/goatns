use utoipa::{Modify, OpenApi};

use crate::zones::FileZoneRecord;
use crate::RecordClass;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::auth::login,
        super::filezonerecord::api_create,
    ),
    components(
        schemas(
            super::auth::AuthPayload,
            super::auth::AuthResponse,
            FileZoneRecord,
            RecordClass,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Authentication", description = "Authentication-related tasks"),
        (name = "Records", description = "DNS Record operations"),
        (name = "Zones", description = "DNS Zone operations"),
    )
)]
#[allow(dead_code)]
pub(crate) struct ApiDoc;

#[allow(dead_code)]
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
