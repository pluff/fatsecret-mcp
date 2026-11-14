//! Profile API: profile status and custom foods.

use reqwest::Method;
use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn get_profile(&self) -> Result<Value> {
        self.profile_request(Method::GET, "/profile/v1", &[]).await
    }

    pub async fn create_food(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/food/v2", query).await
    }
}
