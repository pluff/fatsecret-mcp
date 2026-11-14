//! Public API: reference data (categories, brands, recipe types).

use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn get_food_categories(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/food-categories/v2", query).await
    }

    pub async fn get_food_sub_categories(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/food-sub-categories/v2", query).await
    }

    pub async fn get_brands(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/brands/v2", query).await
    }

    pub async fn get_recipe_types(&self) -> Result<Value> {
        self.public_get("/recipe-types/v2", &[]).await
    }
}
