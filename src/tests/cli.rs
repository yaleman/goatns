use crate::cli::{default_config, export_zone_file};
use crate::config::test_logging;
use crate::db::DBEntity;
use crate::db::test::test_example_com_zone;

#[test]
fn test_default_config_serialization() {
    assert!(default_config() != "Failed to serialize default config file");
}

#[tokio::test]
async fn test_export_zone_file() {
    use crate::tests::test_api::start_test_server;
    let _ = test_logging().await;
    let (db_connection, servers, _config) = start_test_server().await;

    let tempdir = tempfile::tempdir().expect("failed to create temp dir");
    let output_filename = tempdir.path().join("zone_export.json");

    test_example_com_zone()
        .save(&db_connection)
        .await
        .expect("Failed to save test zone");

    export_zone_file(
        servers.datastore_tx.expect("Datastore tx not found"),
        "example.com",
        output_filename
            .to_str()
            .expect("Failed to convert path to str"),
    )
    .await
    .expect("Failed to export zone file");
}
