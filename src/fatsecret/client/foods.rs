//! Public API: foods.

use serde_json::Value;

use super::FatSecretClient;
use super::error::Result;

impl FatSecretClient {
    pub async fn search_foods(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/foods/search/v6", query).await
    }

    pub async fn get_food(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/food/v6", query).await
    }

    pub async fn find_food_by_barcode(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/food/barcode/find-by-id/v3", query).await
    }

    pub async fn autocomplete_foods(&self, query: &[(String, String)]) -> Result<Value> {
        self.public_get("/food/autocomplete/v2", query).await
    }
}
