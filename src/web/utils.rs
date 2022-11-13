use axum::response::Redirect;

pub fn redirect_to_home() -> Redirect {
    Redirect::to("/")
}

pub fn redirect_to_login() -> Redirect {
    Redirect::to("/auth/login")
}

pub fn redirect_to_dashboard() -> Redirect {
    Redirect::to("/ui")
}
pub fn redirect_to_zones_list() -> Redirect {
    Redirect::to("/ui/zones/list")
}
