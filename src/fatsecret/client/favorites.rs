//! Profile API: favorites.

use reqwest::Method;
use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn get_favorite_foods(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/food/favorites/v2", query)
            .await
    }

    pub async fn delete_favorite_food(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/food/favorite/v1", query)
            .await
    }

    pub async fn get_most_eaten_foods(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/food/most-eaten/v2", query)
            .await
    }

    pub async fn get_recently_eaten_foods(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/food/recently-eaten/v2", query)
            .await
    }

    pub async fn get_favorite_recipes(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::GET, "/recipe/favorites/v2", query)
            .await
    }

    pub async fn add_favorite_recipe(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::POST, "/recipe/favorites/v1", query)
            .await
    }

    pub async fn delete_favorite_recipe(&self, query: &[(String, String)]) -> Result<Value> {
        self.profile_request(Method::DELETE, "/recipe/favorites/v1", query)
            .await
    }
}
