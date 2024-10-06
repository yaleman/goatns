// /// this takes patterns <template> [<http_status>]
// /// and makes a tide result with a HTML mime type
// macro_rules! tide_result_html {
//     ($template:tt) => {
//         tide_result_html!($template, 200, (mime::HTML))
//     };
//     ($template:tt, $status:tt) => {
//         tide_result_html!($template, $status, (mime::HTML))
//     };
//     ($template:tt, $status:tt, $mimetype:tt) => {
//         Response::builder($status)
//             .body($template.render().unwrap())
//             .content_type($mimetype)
//             .build()
//     };
// }

macro_rules! check_logged_in {
    ( $state:tt, $session:tt, $path:tt ) => {
        if let Err(e) = crate::web::ui::check_logged_in(&mut $session, $path).await {
            return e.into_response();
        }
    };
}
