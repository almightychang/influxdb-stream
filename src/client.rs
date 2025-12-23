//! InfluxDB streaming client.
//!
//! This module provides the main `Client` type for executing streaming queries
//! against an InfluxDB 2.x server.

use std::pin::Pin;

use async_stream::stream;
use futures::{Stream, StreamExt, TryStreamExt};
use reqwest::{Method, Url};
use serde::Serialize;
use tokio_util::io::StreamReader;

use crate::error::Result;
use crate::parser::AnnotatedCsvParser;
use crate::types::FluxRecord;

/// InfluxDB 2.x streaming client.
///
/// This client executes Flux queries and returns results as an async stream,
/// allowing you to process millions of rows without loading them all into memory.
///
/// # Example
///
/// ```ignore
/// use influxdb_stream::Client;
/// use futures::StreamExt;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = Client::new("http://localhost:8086", "my-org", "my-token");
///
///     let mut stream = client.query_stream(r#"
///         from(bucket: "sensors")
///         |> range(start: -1h)
///         |> filter(fn: (r) => r._measurement == "temperature")
///     "#).await?;
///
///     while let Some(record) = stream.next().await {
///         let record = record?;
///         println!("Got: {:?}", record);
///     }
///
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: Url,
    org: String,
    token: String,
}

/// Query payload for the InfluxDB API.
#[derive(Debug, Serialize)]
struct QueryPayload {
    query: String,
    #[serde(rename = "type")]
    query_type: String,
    dialect: QueryDialect,
}

/// CSV dialect settings for query responses.
#[derive(Debug, Serialize)]
struct QueryDialect {
    annotations: Vec<String>,
    #[serde(rename = "commentPrefix")]
    comment_prefix: String,
    #[serde(rename = "dateTimeFormat")]
    date_time_format: String,
    delimiter: String,
    header: bool,
}

impl Default for QueryDialect {
    fn default() -> Self {
        Self {
            annotations: vec![
                "datatype".to_string(),
                "group".to_string(),
                "default".to_string(),
            ],
            comment_prefix: "#".to_string(),
            date_time_format: "RFC3339".to_string(),
            delimiter: ",".to_string(),
            header: true,
        }
    }
}

impl QueryPayload {
    fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            query_type: "flux".to_string(),
            dialect: QueryDialect::default(),
        }
    }
}

impl Client {
    /// Create a new InfluxDB client.
    ///
    /// # Arguments
    ///
    /// * `url` - Base URL of the InfluxDB server (e.g., "http://localhost:8086")
    /// * `org` - Organization name
    /// * `token` - Authentication token
    ///
    /// # Panics
    ///
    /// Panics if the provided URL is invalid.
    pub fn new(url: impl Into<String>, org: impl Into<String>, token: impl Into<String>) -> Self {
        let url_str = url.into();
        let base_url = Url::parse(&url_str)
            .unwrap_or_else(|e| panic!("Invalid InfluxDB URL '{}': {}", url_str, e));

        Self {
            http: reqwest::Client::new(),
            base_url,
            org: org.into(),
            token: token.into(),
        }
    }

    /// Create a new client with a custom reqwest client.
    ///
    /// This allows you to configure timeouts, proxies, TLS settings, etc.
    pub fn with_http_client(
        http: reqwest::Client,
        url: impl Into<String>,
        org: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        let url_str = url.into();
        let base_url = Url::parse(&url_str)
            .unwrap_or_else(|e| panic!("Invalid InfluxDB URL '{}': {}", url_str, e));

        Self {
            http,
            base_url,
            org: org.into(),
            token: token.into(),
        }
    }

    /// Get the base URL.
    pub fn url(&self) -> &Url {
        &self.base_url
    }

    /// Get the organization name.
    pub fn org(&self) -> &str {
        &self.org
    }

    /// Build the full URL for an API endpoint.
    fn endpoint(&self, path: &str) -> String {
        let mut url = self.base_url.clone();
        url.set_path(path);
        url.to_string()
    }

    /// Execute a Flux query and return results as an async stream.
    ///
    /// This is the primary method for querying InfluxDB. Results are streamed
    /// one record at a time, so you can process arbitrarily large result sets
    /// without running out of memory.
    ///
    /// # Arguments
    ///
    /// * `query` - Flux query string
    ///
    /// # Returns
    ///
    /// A stream of `Result<FluxRecord>`. Each item is either a successfully
    /// parsed record or an error.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use futures::StreamExt;
    ///
    /// let mut stream = client.query_stream("from(bucket: \"test\") |> range(start: -1h)").await?;
    ///
    /// let mut count = 0;
    /// while let Some(result) = stream.next().await {
    ///     let record = result?;
    ///     count += 1;
    /// }
    /// println!("Processed {} records", count);
    /// ```
    pub async fn query_stream(
        &self,
        query: impl Into<String>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<FluxRecord>> + Send>>> {
        let endpoint = self.endpoint("/api/v2/query");
        let payload = QueryPayload::new(query);
        let body = serde_json::to_string(&payload)?;

        let response = self
            .http
            .request(Method::POST, &endpoint)
            .header("Authorization", format!("Token {}", self.token))
            .header("Accept", "application/csv")
            .header("Content-Type", "application/json")
            .query(&[("org", &self.org)])
            .body(body)
            .send()
            .await?
            .error_for_status()?;

        // Convert the response body to an async reader
        let reader = StreamReader::new(
            response
                .bytes_stream()
                .map_err(std::io::Error::other),
        );

        let mut parser = AnnotatedCsvParser::new(reader);

        // Create an async stream that yields records
        let s = stream! {
            loop {
                match parser.next().await {
                    Ok(Some(record)) => yield Ok(record),
                    Ok(None) => break,       // EOF
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(s))
    }

    /// Execute a Flux query and collect all results into a Vec.
    ///
    /// **Warning**: This loads all results into memory. For large result sets,
    /// use `query_stream()` instead to process records one at a time.
    ///
    /// # Arguments
    ///
    /// * `query` - Flux query string
    ///
    /// # Returns
    ///
    /// A vector of all records from the query.
    pub async fn query(&self, query: impl Into<String>) -> Result<Vec<FluxRecord>> {
        let mut stream = self.query_stream(query).await?;
        let mut results = Vec::new();

        while let Some(item) = stream.next().await {
            results.push(item?);
        }

        Ok(results)
    }
}
