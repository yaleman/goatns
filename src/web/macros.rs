/// this takes three patterns <template> [<http_status> [<mime type>]]
/// and makes a tide res
macro_rules! tide_result_html {
    ($template:tt) => {
        tide_result_html!($template, 200, (mime::HTML))
    };
    ($template:tt, $status:tt) => {
        tide_result_html!($template, $status, (mime::HTML))
    };
    ($template:tt, $status:tt, $mimetype:tt) => {
        tide::Result::Ok(
            Response::builder($status)
                .body($template.render().unwrap())
                .content_type($mimetype)
                .build(),
        )
    };
}
