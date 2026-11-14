//! Profile API: saved meals.

use reqwest::Method;
use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn get_saved_meals(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/saved-meals/v2", query)
            .await
    }

    pub async fn create_saved_meal(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/saved-meals/v1", query)
            .await
    }

    pub async fn edit_saved_meal(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::PUT, "/saved-meals/v1", query)
            .await
    }

    pub async fn delete_saved_meal(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::DELETE, "/saved-meals/v1", query)
            .await
    }

    pub async fn get_saved_meal_items(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/saved-meals/item/v2", query)
            .await
    }

    pub async fn add_saved_meal_item(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/saved-meals/item/v1", query)
            .await
    }

    pub async fn edit_saved_meal_item(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::PUT, "/saved-meals/item/v1", query)
            .await
    }

    pub async fn delete_saved_meal_item(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::DELETE, "/saved-meals/item/v1", query)
            .await
    }
}
