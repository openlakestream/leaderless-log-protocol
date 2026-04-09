use std::sync::Arc;

use s3_queue::config::S3QueueConfig;
use s3_queue::queue::init::initialize_topic;
use s3_queue::s3::minio::MinioS3Client;

/// Create a MinIO S3 client for testing.
pub async fn create_test_client() -> Arc<MinioS3Client> {
    let endpoint = std::env::var("S3Q_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".into());
    let bucket = std::env::var("S3Q_BUCKET").unwrap_or_else(|_| "s3q-test".into());
    let access_key = std::env::var("S3Q_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".into());
    let secret_key = std::env::var("S3Q_SECRET_KEY").unwrap_or_else(|_| "minioadmin".into());

    let client = MinioS3Client::new(&endpoint, &bucket, Some(&access_key), Some(&secret_key)).await;
    client.ensure_bucket().await.expect("Failed to ensure bucket");
    Arc::new(client)
}

/// Generate a unique topic name for test isolation.
pub fn unique_topic() -> String {
    format!("test-{}", uuid::Uuid::new_v4())
}

/// Create a test client and initialize a topic.
pub async fn setup_topic() -> (Arc<MinioS3Client>, String, S3QueueConfig) {
    let client = create_test_client().await;
    let topic = unique_topic();
    initialize_topic(client.as_ref(), &topic)
        .await
        .expect("Failed to initialize topic");
    let config = S3QueueConfig::new("http://localhost:9000".into(), "s3q-test".into());
    (client, topic, config)
}
