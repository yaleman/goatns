use crate::{RecordClass, RecordType};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::auth::api_token_login,
        super::records::api_record_create,
        super::records::api_record_update,
        super::records::api_record_get,
        super::records::api_record_delete,
        super::zones::api_zone_create,
        super::zones::api_zone_update,
        super::zones::api_get,
        super::zones::api_zone_delete,
    ),
    components(
        schemas(
            super::auth::AuthPayload,
            super::auth::AuthResponse,
            super::records::ApiRecordUpdate,
            super::records::RecordForm,
            super::zones::ApiZoneResponse,
            super::zones::ZoneForm,
            super::zones::ZoneUpdate,
            crate::db::entities::records::Model,
            crate::db::entities::zones::Model,
            RecordClass,
            RecordType,
        )
    ),
    tags(
        (name = "Authentication", description = "Authentication-related tasks"),
        (name = "Records", description = "DNS Record operations"),
        (name = "Zones", description = "DNS Zone operations"),
    )
)]
#[allow(dead_code)]
pub(crate) struct ApiDoc;
