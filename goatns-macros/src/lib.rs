extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro]
pub fn check_api_auth(_item: TokenStream) -> TokenStream {
    r#"
    let user: User = match session.get("user").await.unwrap() {
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
    .expect("Failed to parse code")
}

// TODO: go back and revisit this weirdness.
// #[proc_macro]
// /// Add a strict-transport-security header via a middleware ref: [Mozilla: Strict-Transport-Security headers](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Strict-Transport-Security)
// ///
// /// max_age: Number of seconds before this header should expire in the client's cache
// ///
// /// preload: Ref [preloading STS](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Strict-Transport-Security#preloading_strict_transport_security) When using preload, the max-age directive must be at least 31536000 (1 year), and the includeSubDomains directive must be present. Not part of the specification.
// pub fn generate_sts_middleware(
//     input: TokenStream,
//     // max_age: u64,
//     // preload: Option<bool>,
//     // include_subdomains: Option<bool>,
// ) -> TokenStream {
//     let signature = syn::parse_macro_input!(input as Signature);
//     println!("// signature {:?}", signature);

//     let max_age = 12345;
//     let preload = Some(false);
//     let include_subdomains = None;

//     let mut header_string = format!("max-age={max_age}");
//     if let Some(include_subdomains) = include_subdomains {
//         if include_subdomains {
//             header_string.push_str("; includeSubDomains");
//         }
//     }
//     if let Some(preload) = preload {
//         if max_age < 31536000 {
//             panic!("max_age must be at least 31536000 (1 year) when preload is set!");
//         }
//         if let Some(subdomains) = include_subdomains {
//             if !subdomains {
//                 panic!("include_subdomainsm must be set when preload is enabled");
//             }
//         }
//         if preload {
//             header_string.push_str("; preload");
//         }
//     }

//     format!(
//         "
// // Middleware that adds a strict-transport-security header
// pub async fn add_sts_headers<B>(mut req: Request<B>, next: Next<B>) -> impl IntoResponse {{
//     let mut response = next.run(req).await;
//     let headers = response.headers_mut();
//     headers.insert(\"Strict-Transport-Security\", \"{}\".parse().unwrap());
//     response
// }}",
//         header_string
//     )
//     .parse()
//     .unwrap()
// }
