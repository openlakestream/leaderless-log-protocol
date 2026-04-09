use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

use s3_queue::config::S3QueueConfig;
use s3_queue::error::AckResult;
use s3_queue::model::Message;
use s3_queue::queue::{compaction, consumer, fence, init, producer, status, vacuum};
use s3_queue::s3::minio::MinioS3Client;
use s3_queue::s3::S3Client;

#[derive(Parser)]
#[command(name = "s3q", about = "S3-Queue CLI — distributed message queue on S3")]
struct Cli {
    /// S3-compatible endpoint URL
    #[arg(long, env = "S3Q_ENDPOINT", default_value = "http://localhost:9000")]
    endpoint: String,

    /// S3 bucket name
    #[arg(long, env = "S3Q_BUCKET", default_value = "s3-queue")]
    bucket: String,

    /// S3 access key
    #[arg(long, env = "S3Q_ACCESS_KEY")]
    access_key: Option<String>,

    /// S3 secret key
    #[arg(long, env = "S3Q_SECRET_KEY")]
    secret_key: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a topic (creates meta objects)
    Init {
        topic: String,
    },

    /// Produce a single message
    Produce {
        topic: String,
        /// Message payload (JSON string)
        payload: String,
    },

    /// Produce a batch of messages from file or stdin (one JSON per line)
    ProduceBatch {
        topic: String,
        /// File containing messages (one JSON per line). Reads from stdin if omitted.
        #[arg(long)]
        file: Option<PathBuf>,
    },

    /// Consume the next message for a consumer
    Consume {
        topic: String,
        consumer_id: String,
    },

    /// Acknowledge a consumed message
    Ack {
        topic: String,
        consumer_id: String,
        offset: u64,
    },

    /// Compact a range of offsets
    Compact {
        topic: String,
        #[arg(long)]
        start: u64,
        #[arg(long)]
        end: u64,
    },

    /// Fence a topic (halt writes)
    Fence {
        topic: String,
    },

    /// Unfence a topic (resume writes)
    Unfence {
        topic: String,
    },

    /// Clean up orphan data objects
    Vacuum {
        topic: String,
    },

    /// Show topic status
    Status {
        topic: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let s3_client = MinioS3Client::new(
        &cli.endpoint,
        &cli.bucket,
        cli.access_key.as_deref(),
        cli.secret_key.as_deref(),
    )
    .await;

    // Ensure bucket exists
    if let Err(e) = s3_client.ensure_bucket().await {
        eprintln!("Warning: could not ensure bucket exists: {}", e);
    }

    let client: Arc<dyn S3Client> = Arc::new(s3_client);
    let config = S3QueueConfig::new(cli.endpoint, cli.bucket);

    let result = run_command(client.as_ref(), &config, cli.command).await;

    match result {
        Ok(()) => {}
        Err(e) => {
            let error_json = serde_json::json!({ "error": e.to_string() });
            println!("{}", serde_json::to_string_pretty(&error_json).unwrap());
            std::process::exit(1);
        }
    }
}

async fn run_command(
    client: &dyn S3Client,
    config: &S3QueueConfig,
    command: Command,
) -> s3_queue::error::Result<()> {
    match command {
        Command::Init { topic } => {
            init::initialize_topic(client, &topic).await?;
            let out = serde_json::json!({ "status": "initialized", "topic": topic });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }

        Command::Produce { topic, payload } => {
            let message = Message { payload };
            let offset = producer::produce(client, &topic, &message, config).await?;
            let out = serde_json::json!({ "status": "produced", "offset": offset });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }

        Command::ProduceBatch { topic, file } => {
            let input = if let Some(path) = file {
                std::fs::read_to_string(&path)
                    .map_err(|e| s3_queue::error::S3QueueError::S3Error(format!("read file: {}", e)))?
            } else {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(|e| s3_queue::error::S3QueueError::S3Error(format!("read stdin: {}", e)))?;
                buf
            };

            let messages: Vec<Message> = input
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|line| serde_json::from_str(line))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| s3_queue::error::S3QueueError::SerializationError(e.to_string()))?;

            let end_offset =
                producer::produce_batch(client, &topic, &messages, config).await?;
            let out = serde_json::json!({
                "status": "produced",
                "count": messages.len(),
                "endOffset": end_offset
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }

        Command::Consume { topic, consumer_id } => {
            match consumer::consume(client, &topic, &consumer_id, config).await? {
                Some(result) => {
                    let out = serde_json::json!({
                        "offset": result.offset,
                        "message": result.message
                    });
                    println!("{}", serde_json::to_string_pretty(&out).unwrap());
                }
                None => {
                    let out = serde_json::json!({ "status": "none", "message": null });
                    println!("{}", serde_json::to_string_pretty(&out).unwrap());
                }
            }
        }

        Command::Ack {
            topic,
            consumer_id,
            offset,
        } => {
            let result =
                consumer::acknowledge(client, &topic, &consumer_id, offset, config).await?;
            let out = match result {
                AckResult::AckAdvanced => {
                    serde_json::json!({ "status": "advanced", "newOffset": offset + 1 })
                }
                AckResult::AckAlreadyProcessed => {
                    serde_json::json!({ "status": "already_processed" })
                }
                AckResult::AckOffsetMismatch { expected, actual } => {
                    serde_json::json!({
                        "status": "offset_mismatch",
                        "expected": expected,
                        "actual": actual
                    })
                }
            };
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }

        Command::Compact { topic, start, end } => {
            compaction::compact(client, &topic, start, end, config).await?;
            let out = serde_json::json!({
                "status": "compacted",
                "start": start,
                "end": end
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }

        Command::Fence { topic } => {
            fence::fence_log(client, &topic, config).await?;
            let out = serde_json::json!({ "status": "fenced", "topic": topic });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }

        Command::Unfence { topic } => {
            fence::unfence_log(client, &topic, config).await?;
            let out = serde_json::json!({ "status": "unfenced", "topic": topic });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }

        Command::Vacuum { topic } => {
            let deleted = vacuum::vacuum(client, &topic, config.vacuum_retention_period_s).await?;
            let out = serde_json::json!({
                "status": "vacuumed",
                "deletedOrphans": deleted
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }

        Command::Status { topic } => {
            let s = status::get_status(client, &topic).await?;
            println!("{}", serde_json::to_string_pretty(&s).unwrap());
        }
    }

    Ok(())
}
