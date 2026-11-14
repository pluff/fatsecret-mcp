//! Public API: recipes.

use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn search_recipes(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/recipes/search/v3", query).await
    }

    pub async fn get_recipe(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/recipe/v2", query).await
    }
}
