use async_trait::async_trait;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{BehaviorVersion, Region};
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{Delete, ObjectIdentifier};
use aws_sdk_s3::Client;
use bytes::Bytes;
use chrono::DateTime;

use super::{CasResult, ListObjectsResult, ObjectInfo, S3Client};
use crate::error::{Result, S3QueueError};

/// S3Client implementation using the AWS SDK, targeting MinIO.
pub struct MinioS3Client {
    client: Client,
    bucket: String,
}

impl MinioS3Client {
    pub async fn new(
        endpoint: &str,
        bucket: &str,
        access_key: Option<&str>,
        secret_key: Option<&str>,
    ) -> Self {
        let creds = Credentials::new(
            access_key.unwrap_or("minioadmin"),
            secret_key.unwrap_or("minioadmin"),
            None,
            None,
            "s3q",
        );

        let config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .region(Region::new("us-east-1"))
            .credentials_provider(creds)
            .force_path_style(true)
            .build();

        let client = Client::from_conf(config);

        MinioS3Client {
            client,
            bucket: bucket.to_string(),
        }
    }

    /// Ensure the bucket exists (for convenience in tests/CLI).
    pub async fn ensure_bucket(&self) -> Result<()> {
        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => Ok(()),
            Err(_) => {
                self.client
                    .create_bucket()
                    .bucket(&self.bucket)
                    .send()
                    .await
                    .map_err(|e| S3QueueError::S3Error(e.to_string()))?;
                Ok(())
            }
        }
    }
}

fn is_precondition_failed<E>(err: &SdkError<E>) -> bool {
    match err {
        SdkError::ServiceError(e) => {
            let raw = e.raw();
            raw.status().as_u16() == 412
        }
        _ => false,
    }
}

fn is_not_found<E>(err: &SdkError<E>) -> bool {
    match err {
        SdkError::ServiceError(e) => {
            let raw = e.raw();
            raw.status().as_u16() == 404
        }
        _ => false,
    }
}

#[async_trait]
impl S3Client for MinioS3Client {
    async fn get(&self, key: &str) -> Result<Option<(Bytes, String)>> {
        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(output) => {
                let etag = output
                    .e_tag()
                    .unwrap_or("")
                    .trim_matches('"')
                    .to_string();
                let body = output
                    .body
                    .collect()
                    .await
                    .map_err(|e| S3QueueError::S3Error(e.to_string()))?;
                Ok(Some((body.into_bytes(), etag)))
            }
            Err(err) => {
                if is_not_found(&err) {
                    Ok(None)
                } else {
                    Err(S3QueueError::S3Error(err.to_string()))
                }
            }
        }
    }

    async fn put_conditional_match(
        &self,
        key: &str,
        value: Bytes,
        etag: &str,
    ) -> Result<CasResult> {
        let quoted_etag = if etag.starts_with('"') {
            etag.to_string()
        } else {
            format!("\"{}\"", etag)
        };

        match self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(value))
            .if_match(&quoted_etag)
            .send()
            .await
        {
            Ok(output) => {
                let new_etag = output
                    .e_tag()
                    .unwrap_or("")
                    .trim_matches('"')
                    .to_string();
                Ok(CasResult::Ok { etag: new_etag })
            }
            Err(err) => {
                if is_precondition_failed(&err) {
                    Ok(CasResult::PreconditionFailed)
                } else {
                    Err(S3QueueError::S3Error(err.to_string()))
                }
            }
        }
    }

    async fn put_conditional_none_match(&self, key: &str, value: Bytes) -> Result<CasResult> {
        match self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(value))
            .if_none_match("*")
            .send()
            .await
        {
            Ok(output) => {
                let new_etag = output
                    .e_tag()
                    .unwrap_or("")
                    .trim_matches('"')
                    .to_string();
                Ok(CasResult::Ok { etag: new_etag })
            }
            Err(err) => {
                if is_precondition_failed(&err) {
                    Ok(CasResult::PreconditionFailed)
                } else {
                    Err(S3QueueError::S3Error(err.to_string()))
                }
            }
        }
    }

    async fn put(&self, key: &str, value: Bytes) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(value))
            .send()
            .await
            .map_err(|e| S3QueueError::S3Error(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| S3QueueError::S3Error(e.to_string()))?;
        Ok(())
    }

    async fn batch_delete(&self, keys: &[String]) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }

        let objects: Vec<ObjectIdentifier> = keys
            .iter()
            .map(|k| ObjectIdentifier::builder().key(k).build().unwrap())
            .collect();

        let delete = Delete::builder()
            .set_objects(Some(objects))
            .quiet(true)
            .build()
            .map_err(|e| S3QueueError::S3Error(e.to_string()))?;

        self.client
            .delete_objects()
            .bucket(&self.bucket)
            .delete(delete)
            .send()
            .await
            .map_err(|e| S3QueueError::S3Error(e.to_string()))?;

        Ok(())
    }

    async fn list_objects(
        &self,
        prefix: &str,
        start_after: &str,
        max_keys: i32,
    ) -> Result<ListObjectsResult> {
        let output = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .start_after(start_after)
            .max_keys(max_keys)
            .send()
            .await
            .map_err(|e| S3QueueError::S3Error(e.to_string()))?;

        let contents = output
            .contents()
            .iter()
            .map(|obj| {
                let last_modified = obj
                    .last_modified()
                    .map(|t| {
                        DateTime::from_timestamp(t.secs(), t.subsec_nanos() as u32)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();

                ObjectInfo {
                    key: obj.key().unwrap_or("").to_string(),
                    last_modified,
                }
            })
            .collect();

        Ok(ListObjectsResult {
            contents,
            is_truncated: output.is_truncated().unwrap_or(false),
        })
    }
}
