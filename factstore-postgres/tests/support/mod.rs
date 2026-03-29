use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use factstore_postgres::PostgresStore;
use sqlx::{Connection, Executor, PgConnection};
use url::Url;

static NEXT_SCHEMA_ID: AtomicU64 = AtomicU64::new(1);

#[allow(dead_code)]
pub(crate) fn run_store_test<TestFn>(test: TestFn)
where
    TestFn: FnOnce(Box<dyn Fn() -> PostgresStore>),
{
    let store_database_url = create_store_database_url();
    test(Box::new(move || {
        PostgresStore::connect(&store_database_url).expect("postgres store should connect")
    }));
}

#[allow(dead_code)]
pub(crate) fn create_store() -> PostgresStore {
    let store_database_url = create_store_database_url();
    PostgresStore::connect(&store_database_url).expect("postgres store should connect")
}

fn database_url() -> String {
    env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set to run factstore-postgres integration tests")
}

fn unique_schema_name() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let next_id = NEXT_SCHEMA_ID.fetch_add(1, Ordering::Relaxed);

    format!("factstore_test_{timestamp}_{next_id}")
}

fn create_store_database_url() -> String {
    let base_database_url = database_url();
    let schema_name = unique_schema_name();
    let admin_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    admin_runtime.block_on(async {
        let mut connection = PgConnection::connect(&base_database_url)
            .await
            .expect("postgres test connection should succeed");

        connection
            .execute(format!("CREATE SCHEMA \"{schema_name}\"").as_str())
            .await
            .expect("test schema should be created");
    });

    schema_database_url(&base_database_url, &schema_name)
}

fn schema_database_url(base_database_url: &str, schema_name: &str) -> String {
    let mut url = Url::parse(base_database_url).expect("DATABASE_URL should be a valid URL");
    url.query_pairs_mut()
        .append_pair("options", &format!("--search_path={schema_name}"));
    url.into()
}
