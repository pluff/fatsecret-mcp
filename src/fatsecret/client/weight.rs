//! Profile API: weight.

use reqwest::Method;
use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn update_weight(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/weight/v1", query)
            .await
    }

    pub async fn get_weight_month(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/weight/month/v2", query)
            .await
    }
}
