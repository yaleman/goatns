extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro]
pub fn check_api_auth(_item: TokenStream) -> TokenStream {
    r#"
    let user: User = match session.get("user") {
        Some(val) => val,
        None => {
            #[cfg(test)]
            println!("User not found in api_create call");
            #[cfg(not(test))]
            log::debug!("User not found in api_create call");
            return error_result_json!("", StatusCode::FORBIDDEN);
        }
    };
    "#
    .parse()
    .unwrap()
}
