//! Profile API: food diary.

use reqwest::Method;
use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn get_food_entries(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/food-entries/v2", query)
            .await
    }

    pub async fn get_food_entries_month(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/food-entries/month/v2", query)
            .await
    }

    pub async fn create_food_entry(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/food-entries/v1", query)
            .await
    }

    pub async fn edit_food_entry(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::PUT, "/food-entries/v1", query)
            .await
    }

    pub async fn delete_food_entry(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::DELETE, "/food-entries/v1", query)
            .await
    }

    pub async fn copy_food_entries(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/food-entries/copy/v1", query)
            .await
    }

    pub async fn copy_saved_meal_entries(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/food-entries/copy/saved-meal/v1", query)
            .await
    }
}
