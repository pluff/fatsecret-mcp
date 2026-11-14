//! Profile API: exercises.

use reqwest::Method;
use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn get_exercises(&self) -> Result<Value> {
        self.profile_request(Method::GET, "/exercises/v2", &[])
            .await
    }

    pub async fn edit_exercise_entries(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::PUT, "/exercise-entries/v1", query)
            .await
    }

    pub async fn get_exercise_entries_month(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/exercise-entries/month/v2", query)
            .await
    }

    pub async fn save_exercise_template(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/exercise-entries/day/v1", query)
            .await
    }
}
