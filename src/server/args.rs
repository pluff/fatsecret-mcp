//! Typed tool argument structs for the rmcp server (schemars + serde).
//!
//! Each struct knows how to build the FatSecret query parameters (`to_query`);
//! `format=json` is injected by the client.

use schemars::JsonSchema;
use serde::Deserialize;

pub type Query = Vec<(String, String)>;

fn push<T: ToString>(q: &mut Query, key: &str, value: &Option<T>) {
    if let Some(v) = value {
        q.push((key.to_string(), v.to_string()));
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchFoodsParams {
    /// Search query for foods
    pub search_expression: String,
    #[schemars(extend("type" = "number"))]
    pub max_results: Option<f64>,
    #[schemars(extend("type" = "number"))]
    pub page_number: Option<f64>,
    /// Premier only: add food classification hierarchy and food form.
    pub include_food_classification_data: Option<bool>,
    /// Premier only: add a nutrient content tag per nutrient.
    pub include_nutrient_content_tags: Option<bool>,
    /// Deprecated by FatSecret in v6/v3 (use include_food_classification_data); kept for compatibility.
    pub include_sub_categories: Option<bool>,
    pub flag_default_serving: Option<bool>,
    pub include_food_attributes: Option<bool>,
    pub include_food_images: Option<bool>,
    pub language: Option<String>,
    pub region: Option<String>,
    pub food_type: Option<String>,
}

impl SearchFoodsParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![(
            "search_expression".to_string(),
            self.search_expression.clone(),
        )];
        push(
            &mut q,
            "include_sub_categories",
            &self.include_sub_categories,
        );
        push(
            &mut q,
            "include_food_classification_data",
            &self.include_food_classification_data,
        );
        push(
            &mut q,
            "include_nutrient_content_tags",
            &self.include_nutrient_content_tags,
        );
        push(&mut q, "flag_default_serving", &self.flag_default_serving);
        push(
            &mut q,
            "include_food_attributes",
            &self.include_food_attributes,
        );
        push(&mut q, "include_food_images", &self.include_food_images);
        push(&mut q, "max_results", &self.max_results);
        push(&mut q, "language", &self.language);
        push(&mut q, "region", &self.region);
        push(&mut q, "page_number", &self.page_number);
        push(&mut q, "food_type", &self.food_type);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFoodParams {
    /// Food ID
    pub food_id: i64,
    /// Premier only: add food classification hierarchy and food form.
    pub include_food_classification_data: Option<bool>,
    /// Premier only: add a nutrient content tag per nutrient.
    pub include_nutrient_content_tags: Option<bool>,
    /// Deprecated by FatSecret in v6/v3 (use include_food_classification_data); kept for compatibility.
    pub include_sub_categories: Option<bool>,
    pub include_food_images: Option<bool>,
    pub include_food_attributes: Option<bool>,
    pub flag_default_serving: Option<bool>,
    pub region: Option<String>,
    pub language: Option<String>,
    #[schemars(extend("type" = "number"))]
    pub page_number: Option<f64>,
}

impl GetFoodParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![("food_id".to_string(), self.food_id.to_string())];
        push(
            &mut q,
            "include_sub_categories",
            &self.include_sub_categories,
        );
        push(
            &mut q,
            "include_food_classification_data",
            &self.include_food_classification_data,
        );
        push(
            &mut q,
            "include_nutrient_content_tags",
            &self.include_nutrient_content_tags,
        );
        push(&mut q, "include_food_images", &self.include_food_images);
        push(
            &mut q,
            "include_food_attributes",
            &self.include_food_attributes,
        );
        push(&mut q, "flag_default_serving", &self.flag_default_serving);
        push(&mut q, "region", &self.region);
        push(&mut q, "language", &self.language);
        push(&mut q, "page_number", &self.page_number);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindFoodByBarcodeParams {
    /// GTIN-13 barcode number
    pub barcode: String,
    pub region: Option<String>,
    pub language: Option<String>,
    /// Premier only: add food classification hierarchy and food form.
    pub include_food_classification_data: Option<bool>,
    /// Premier only: add a nutrient content tag per nutrient.
    pub include_nutrient_content_tags: Option<bool>,
    /// Deprecated by FatSecret in v6/v3 (use include_food_classification_data); kept for compatibility.
    pub include_sub_categories: Option<bool>,
    pub include_food_images: Option<bool>,
    pub include_food_attributes: Option<bool>,
    pub flag_default_serving: Option<bool>,
}

impl FindFoodByBarcodeParams {
    /// Mirrors JS `Number(barcode)`: the barcode is sent as a number (leading zeros dropped).
    pub fn to_query(&self) -> Result<Query, String> {
        let barcode: u64 =
            self.barcode.trim().parse().map_err(|_| {
                format!("Invalid barcode: {:?} (expected digits only)", self.barcode)
            })?;
        let mut q = vec![("barcode".to_string(), barcode.to_string())];
        push(&mut q, "region", &self.region);
        push(&mut q, "language", &self.language);
        push(
            &mut q,
            "include_sub_categories",
            &self.include_sub_categories,
        );
        push(
            &mut q,
            "include_food_classification_data",
            &self.include_food_classification_data,
        );
        push(
            &mut q,
            "include_nutrient_content_tags",
            &self.include_nutrient_content_tags,
        );
        push(&mut q, "include_food_images", &self.include_food_images);
        push(
            &mut q,
            "include_food_attributes",
            &self.include_food_attributes,
        );
        push(&mut q, "flag_default_serving", &self.flag_default_serving);
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AutocompleteFoodsParams {
    /// Partial search expression
    pub expression: String,
    #[schemars(extend("type" = "number"))]
    pub max_results: Option<f64>,
    pub region: Option<String>,
    pub language: Option<String>,
}

impl AutocompleteFoodsParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![("expression".to_string(), self.expression.clone())];
        push(&mut q, "max_results", &self.max_results);
        push(&mut q, "region", &self.region);
        push(&mut q, "language", &self.language);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRecipesParams {
    /// Search query for recipes
    pub search_expression: Option<String>,
    #[schemars(extend("type" = "number"))]
    pub page_number: Option<f64>,
    #[schemars(extend("type" = "number"))]
    pub max_results: Option<f64>,
    pub recipe_types: Option<String>,
    pub recipe_types_matchall: Option<bool>,
    pub must_have_images: Option<bool>,
    #[serde(rename = "calories.from")]
    #[schemars(extend("type" = "number"))]
    pub calories_from: Option<f64>,
    #[serde(rename = "calories.to")]
    #[schemars(extend("type" = "number"))]
    pub calories_to: Option<f64>,
    #[serde(rename = "carb_percentage.from")]
    #[schemars(extend("type" = "number"))]
    pub carb_percentage_from: Option<f64>,
    #[serde(rename = "carb_percentage.to")]
    #[schemars(extend("type" = "number"))]
    pub carb_percentage_to: Option<f64>,
    #[serde(rename = "protein_percentage.from")]
    #[schemars(extend("type" = "number"))]
    pub protein_percentage_from: Option<f64>,
    #[serde(rename = "protein_percentage.to")]
    #[schemars(extend("type" = "number"))]
    pub protein_percentage_to: Option<f64>,
    #[serde(rename = "fat_percentage.from")]
    #[schemars(extend("type" = "number"))]
    pub fat_percentage_from: Option<f64>,
    #[serde(rename = "fat_percentage.to")]
    #[schemars(extend("type" = "number"))]
    pub fat_percentage_to: Option<f64>,
    #[serde(rename = "prep_time.from")]
    #[schemars(extend("type" = "number"))]
    pub prep_time_from: Option<f64>,
    #[serde(rename = "prep_time.to")]
    #[schemars(extend("type" = "number"))]
    pub prep_time_to: Option<f64>,
    pub sort_by: Option<String>,
}

impl SearchRecipesParams {
    pub fn to_query(&self) -> Query {
        let mut q = Vec::new();
        push(&mut q, "page_number", &self.page_number);
        push(&mut q, "max_results", &self.max_results);
        push(&mut q, "search_expression", &self.search_expression);
        push(&mut q, "recipe_types", &self.recipe_types);
        push(&mut q, "recipe_types_matchall", &self.recipe_types_matchall);
        push(&mut q, "must_have_images", &self.must_have_images);
        push(&mut q, "calories.from", &self.calories_from);
        push(&mut q, "calories.to", &self.calories_to);
        push(&mut q, "carb_percentage.from", &self.carb_percentage_from);
        push(&mut q, "carb_percentage.to", &self.carb_percentage_to);
        push(
            &mut q,
            "protein_percentage.from",
            &self.protein_percentage_from,
        );
        push(&mut q, "protein_percentage.to", &self.protein_percentage_to);
        push(&mut q, "fat_percentage.from", &self.fat_percentage_from);
        push(&mut q, "fat_percentage.to", &self.fat_percentage_to);
        push(&mut q, "prep_time.from", &self.prep_time_from);
        push(&mut q, "prep_time.to", &self.prep_time_to);
        push(&mut q, "sort_by", &self.sort_by);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetRecipeParams {
    /// Recipe ID
    pub recipe_id: i64,
    #[schemars(extend("type" = "number"))]
    pub page_number: Option<f64>,
    #[schemars(extend("type" = "number"))]
    pub max_results: Option<f64>,
}

impl GetRecipeParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![("recipe_id".to_string(), self.recipe_id.to_string())];
        push(&mut q, "page_number", &self.page_number);
        push(&mut q, "max_results", &self.max_results);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFoodCategoriesParams {
    pub language: Option<String>,
    pub region: Option<String>,
}

impl GetFoodCategoriesParams {
    pub fn to_query(&self) -> Query {
        let mut q = Vec::new();
        push(&mut q, "language", &self.language);
        push(&mut q, "region", &self.region);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFoodSubCategoriesParams {
    /// Food category ID
    pub food_category_id: i64,
    pub language: Option<String>,
    pub region: Option<String>,
}

impl GetFoodSubCategoriesParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![(
            "food_category_id".to_string(),
            self.food_category_id.to_string(),
        )];
        push(&mut q, "language", &self.language);
        push(&mut q, "region", &self.region);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetBrandsParams {
    pub starts_with: Option<String>,
    pub brand_type: Option<String>,
    pub language: Option<String>,
    pub region: Option<String>,
}

impl GetBrandsParams {
    pub fn to_query(&self) -> Query {
        let mut q = Vec::new();
        push(&mut q, "starts_with", &self.starts_with);
        push(&mut q, "brand_type", &self.brand_type);
        push(&mut q, "language", &self.language);
        push(&mut q, "region", &self.region);
        q
    }
}

/// Arguments for tools that take no input.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct EmptyParams {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn has(q: &Query, k: &str, v: &str) -> bool {
        q.iter().any(|(a, b)| a == k && b == v)
    }

    #[test]
    fn search_foods_query() {
        let p: SearchFoodsParams = serde_json::from_value(json!({
            "search_expression": "apple", "max_results": 5, "include_food_images": true
        }))
        .unwrap();
        let q = p.to_query();
        assert!(has(&q, "search_expression", "apple"));
        assert!(has(&q, "max_results", "5"));
        assert!(has(&q, "include_food_images", "true"));
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn required_fields_enforced() {
        assert!(serde_json::from_value::<SearchFoodsParams>(json!({})).is_err());
        assert!(serde_json::from_value::<GetFoodParams>(json!({"food_id": "x"})).is_err());
        assert!(serde_json::from_value::<GetRecipeParams>(json!({})).is_err());
    }

    #[test]
    fn barcode_string_to_number() {
        let p: FindFoodByBarcodeParams =
            serde_json::from_value(json!({"barcode": "0012345678905", "region": "US"})).unwrap();
        let q = p.to_query().unwrap();
        assert!(has(&q, "barcode", "12345678905"));
        assert!(has(&q, "region", "US"));
        let bad = FindFoodByBarcodeParams {
            barcode: "abc".into(),
            region: None,
            language: None,
            include_sub_categories: None,
            include_food_classification_data: None,
            include_nutrient_content_tags: None,
            include_food_images: None,
            include_food_attributes: None,
            flag_default_serving: None,
        };
        assert!(bad.to_query().is_err());
    }

    #[test]
    fn recipe_dotted_keys() {
        let p: SearchRecipesParams = serde_json::from_value(json!({
            "calories.from": 100, "calories.to": 500.5, "prep_time.to": 30, "must_have_images": false
        }))
        .unwrap();
        let q = p.to_query();
        assert!(has(&q, "calories.from", "100"));
        assert!(has(&q, "calories.to", "500.5"));
        assert!(has(&q, "prep_time.to", "30"));
        assert!(has(&q, "must_have_images", "false"));
    }

    #[test]
    fn get_ids_query() {
        let q = GetFoodSubCategoriesParams {
            food_category_id: 7,
            language: None,
            region: Some("US".into()),
        }
        .to_query();
        assert!(has(&q, "food_category_id", "7"));
        assert!(has(&q, "region", "US"));
        assert!(
            GetBrandsParams {
                starts_with: None,
                brand_type: None,
                language: None,
                region: None
            }
            .to_query()
            .is_empty()
        );
    }
}

// ── Profile API (per-user OAuth1) ──

/// Converts `YYYY-MM-DD` to days since the Unix epoch (JS `dateToDays`).
pub fn date_to_days(date: &str) -> Result<i64, String> {
    let d = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| format!("Invalid date: {date:?} (expected YYYY-MM-DD)"))?;
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid epoch");
    Ok((d - epoch).num_days())
}

/// Optional date; omitted when absent (FatSecret then defaults to today), like JS.
fn push_date(q: &mut Query, key: &str, date: &Option<String>) -> Result<(), String> {
    if let Some(d) = date.as_deref().filter(|d| !d.is_empty()) {
        q.push((key.to_string(), date_to_days(d)?.to_string()));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(inline)]
pub enum Meal {
    Breakfast,
    Lunch,
    Dinner,
    Other,
}

impl Meal {
    pub fn parse(s: &str) -> Option<Meal> {
        match s.trim().to_lowercase().as_str() {
            "breakfast" => Some(Meal::Breakfast),
            "lunch" => Some(Meal::Lunch),
            "dinner" => Some(Meal::Dinner),
            "other" => Some(Meal::Other),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Meal::Breakfast => "breakfast",
            Meal::Lunch => "lunch",
            Meal::Dinner => "dinner",
            Meal::Other => "other",
        }
    }
}

/// `Option<Meal>` schema without `null` in the enum (matches the JS zod output).
fn optional_meal_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": ["breakfast", "lunch", "dinner", "other"]
    })
}

fn push_meal(q: &mut Query, meal: &Option<Meal>) {
    if let Some(m) = meal {
        q.push(("meal".to_string(), m.as_str().to_string()));
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFoodEntriesParams {
    /// Date in YYYY-MM-DD (required if food_entry_id not specified)
    pub date: Option<String>,
    #[schemars(extend("type" = "number"))]
    pub food_entry_id: Option<f64>,
}

impl GetFoodEntriesParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = Query::new();
        push(&mut q, "food_entry_id", &self.food_entry_id);
        push_date(&mut q, "date", &self.date)?;
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFoodEntriesMonthParams {
    /// Any date within the target month (YYYY-MM-DD)
    pub date: Option<String>,
}

impl GetFoodEntriesMonthParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = Query::new();
        push_date(&mut q, "date", &self.date)?;
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateFoodEntryParams {
    /// Food ID
    pub food_id: i64,
    /// Name for the food entry
    pub food_entry_name: String,
    /// Serving size ID
    pub serving_id: i64,
    /// Number of serving units
    #[schemars(extend("type" = "number"))]
    pub number_of_units: f64,
    /// Meal type
    pub meal: Meal,
    /// Date in YYYY-MM-DD format (default today)
    pub date: Option<String>,
}

impl CreateFoodEntryParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = vec![
            ("food_id".to_string(), self.food_id.to_string()),
            ("food_entry_name".to_string(), self.food_entry_name.clone()),
            ("serving_id".to_string(), self.serving_id.to_string()),
            (
                "number_of_units".to_string(),
                self.number_of_units.to_string(),
            ),
        ];
        push_meal(&mut q, &Some(self.meal));
        push_date(&mut q, "date", &self.date)?;
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditFoodEntryParams {
    /// Food entry ID to edit
    pub food_entry_id: i64,
    pub food_entry_name: Option<String>,
    #[schemars(extend("type" = "number"))]
    pub serving_id: Option<f64>,
    #[schemars(extend("type" = "number"))]
    pub number_of_units: Option<f64>,
    /// Meal type
    #[serde(default)]
    #[schemars(schema_with = "optional_meal_schema")]
    pub meal: Option<Meal>,
}

impl EditFoodEntryParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![("food_entry_id".to_string(), self.food_entry_id.to_string())];
        push(&mut q, "food_entry_name", &self.food_entry_name);
        push(&mut q, "serving_id", &self.serving_id);
        push(&mut q, "number_of_units", &self.number_of_units);
        push_meal(&mut q, &self.meal);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteFoodEntryParams {
    /// Food entry ID to delete
    pub food_entry_id: i64,
}

impl DeleteFoodEntryParams {
    pub fn to_query(&self) -> Query {
        vec![("food_entry_id".to_string(), self.food_entry_id.to_string())]
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CopyFoodEntriesParams {
    /// Source date YYYY-MM-DD
    pub from_date: String,
    /// Target date YYYY-MM-DD
    pub to_date: String,
    /// Meal type
    #[serde(default)]
    #[schemars(schema_with = "optional_meal_schema")]
    pub meal: Option<Meal>,
}

impl CopyFoodEntriesParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = vec![
            (
                "from_date".to_string(),
                date_to_days(&self.from_date)?.to_string(),
            ),
            (
                "to_date".to_string(),
                date_to_days(&self.to_date)?.to_string(),
            ),
        ];
        push_meal(&mut q, &self.meal);
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CopySavedMealEntriesParams {
    /// Saved meal ID to copy
    pub saved_meal_id: i64,
    /// Meal type
    pub meal: Meal,
    /// Date in YYYY-MM-DD format (default today)
    pub date: Option<String>,
}

impl CopySavedMealEntriesParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = vec![("saved_meal_id".to_string(), self.saved_meal_id.to_string())];
        push_meal(&mut q, &Some(self.meal));
        push_date(&mut q, "date", &self.date)?;
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteFavoriteFoodParams {
    /// Food ID to remove from favorites
    pub food_id: i64,
    pub serving_id: Option<String>,
    pub number_of_units: Option<String>,
}

impl DeleteFavoriteFoodParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![("food_id".to_string(), self.food_id.to_string())];
        push(&mut q, "serving_id", &self.serving_id);
        push(&mut q, "number_of_units", &self.number_of_units);
        q
    }
}

/// Optional meal filter (most eaten / recently eaten foods).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MealFilterParams {
    /// Meal type
    #[serde(default)]
    #[schemars(schema_with = "optional_meal_schema")]
    pub meal: Option<Meal>,
}

impl MealFilterParams {
    pub fn to_query(&self) -> Query {
        let mut q = Query::new();
        push_meal(&mut q, &self.meal);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSavedMealsParams {
    /// Filter by meal type
    #[serde(default)]
    #[schemars(schema_with = "optional_meal_schema")]
    pub meal: Option<Meal>,
}

impl GetSavedMealsParams {
    pub fn to_query(&self) -> Query {
        let mut q = Query::new();
        push_meal(&mut q, &self.meal);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecipeIdParams {
    /// Recipe ID
    pub recipe_id: i64,
}

impl RecipeIdParams {
    pub fn to_query(&self) -> Query {
        vec![("recipe_id".to_string(), self.recipe_id.to_string())]
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateSavedMealParams {
    /// Meal name
    pub saved_meal_name: String,
    pub saved_meal_description: Option<String>,
    pub meals: Option<String>,
}

impl CreateSavedMealParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![("saved_meal_name".to_string(), self.saved_meal_name.clone())];
        push(
            &mut q,
            "saved_meal_description",
            &self.saved_meal_description,
        );
        push(&mut q, "meals", &self.meals);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditSavedMealParams {
    /// Saved meal ID
    pub saved_meal_id: i64,
    pub saved_meal_name: Option<String>,
    pub saved_meal_description: Option<String>,
    pub meals: Option<String>,
}

impl EditSavedMealParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![("saved_meal_id".to_string(), self.saved_meal_id.to_string())];
        push(&mut q, "saved_meal_name", &self.saved_meal_name);
        push(
            &mut q,
            "saved_meal_description",
            &self.saved_meal_description,
        );
        push(&mut q, "meals", &self.meals);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SavedMealIdParams {
    /// Saved meal ID
    pub saved_meal_id: i64,
}

impl SavedMealIdParams {
    pub fn to_query(&self) -> Query {
        vec![("saved_meal_id".to_string(), self.saved_meal_id.to_string())]
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddSavedMealItemParams {
    /// Saved meal ID
    pub saved_meal_id: i64,
    /// Food ID to add
    pub food_id: i64,
    /// Item name
    pub saved_meal_item_name: String,
    /// Serving ID
    pub serving_id: i64,
    /// Number of serving units
    #[schemars(extend("type" = "number"))]
    pub number_of_units: f64,
}

impl AddSavedMealItemParams {
    pub fn to_query(&self) -> Query {
        vec![
            ("saved_meal_id".to_string(), self.saved_meal_id.to_string()),
            ("food_id".to_string(), self.food_id.to_string()),
            (
                "saved_meal_item_name".to_string(),
                self.saved_meal_item_name.clone(),
            ),
            ("serving_id".to_string(), self.serving_id.to_string()),
            (
                "number_of_units".to_string(),
                self.number_of_units.to_string(),
            ),
        ]
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditSavedMealItemParams {
    /// Saved meal item ID
    pub saved_meal_item_id: i64,
    pub saved_meal_item_name: Option<String>,
    #[schemars(extend("type" = "number"))]
    pub number_of_units: Option<f64>,
}

impl EditSavedMealItemParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![(
            "saved_meal_item_id".to_string(),
            self.saved_meal_item_id.to_string(),
        )];
        push(&mut q, "saved_meal_item_name", &self.saved_meal_item_name);
        push(&mut q, "number_of_units", &self.number_of_units);
        q
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteSavedMealItemParams {
    /// Saved meal item ID to delete
    pub saved_meal_item_id: i64,
}

impl DeleteSavedMealItemParams {
    pub fn to_query(&self) -> Query {
        vec![(
            "saved_meal_item_id".to_string(),
            self.saved_meal_item_id.to_string(),
        )]
    }
}

// ── Profile API: weight, exercise, custom food ──

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateWeightParams {
    /// Current weight in kg
    #[schemars(extend("type" = "number"))]
    pub current_weight_kg: f64,
    /// Date in YYYY-MM-DD format (default today)
    pub date: Option<String>,
    pub weight_type: Option<String>,
    pub height_type: Option<String>,
    #[schemars(extend("type" = "number"))]
    pub goal_weight_kg: Option<f64>,
    pub comment: Option<String>,
    #[schemars(extend("type" = "number"))]
    pub current_height_cm: Option<f64>,
}

impl UpdateWeightParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = vec![(
            "current_weight_kg".to_string(),
            self.current_weight_kg.to_string(),
        )];
        push_date(&mut q, "date", &self.date)?;
        push(&mut q, "weight_type", &self.weight_type);
        push(&mut q, "height_type", &self.height_type);
        push(&mut q, "goal_weight_kg", &self.goal_weight_kg);
        push(&mut q, "comment", &self.comment);
        push(&mut q, "current_height_cm", &self.current_height_cm);
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MonthDateParams {
    /// Any date within the target month (YYYY-MM-DD)
    pub date: Option<String>,
}

impl MonthDateParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = Query::new();
        push_date(&mut q, "date", &self.date)?;
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditExerciseEntriesParams {
    /// Exercise ID to shift time TO
    pub shift_to_id: i64,
    /// Exercise ID to shift time FROM
    pub shift_from_id: i64,
    /// Minutes to shift
    pub minutes: i64,
    /// Date in YYYY-MM-DD format (default today)
    pub date: Option<String>,
    pub shift_to_name: Option<String>,
    pub shift_from_name: Option<String>,
    #[schemars(extend("type" = "number"))]
    pub kcal: Option<f64>,
}

impl EditExerciseEntriesParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = vec![
            ("shift_to_id".to_string(), self.shift_to_id.to_string()),
            ("shift_from_id".to_string(), self.shift_from_id.to_string()),
            ("minutes".to_string(), self.minutes.to_string()),
        ];
        push(&mut q, "shift_to_name", &self.shift_to_name);
        push(&mut q, "shift_from_name", &self.shift_from_name);
        push(&mut q, "kcal", &self.kcal);
        push_date(&mut q, "date", &self.date)?;
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveExerciseTemplateParams {
    /// Days of week as bit flags (Sun=bit 1, Sat=bit 7), converted to int
    pub days: i64,
    /// Date in YYYY-MM-DD format (default today)
    pub date: Option<String>,
}

impl SaveExerciseTemplateParams {
    pub fn to_query(&self) -> Result<Query, String> {
        let mut q = vec![("days".to_string(), self.days.to_string())];
        push_date(&mut q, "date", &self.date)?;
        Ok(q)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateFoodParams {
    /// Brand name
    pub brand_name: String,
    /// Food name
    pub food_name: String,
    /// Serving size description (e.g. "1 slice")
    pub serving_size: String,
    /// Calories (kcal)
    #[schemars(extend("type" = "number"))]
    pub calories: f64,
    /// Total fat (g)
    #[schemars(extend("type" = "number"))]
    pub fat: f64,
    /// Total carbohydrate (g)
    #[schemars(extend("type" = "number"))]
    pub carbohydrate: f64,
    /// Protein (g)
    #[schemars(extend("type" = "number"))]
    pub protein: f64,
    pub serving_amount: Option<String>,
    pub serving_amount_unit: Option<String>,
    pub calories_from_fat: Option<String>,
    pub saturated_fat: Option<String>,
    pub polyunsaturated_fat: Option<String>,
    pub monounsaturated_fat: Option<String>,
    pub trans_fat: Option<String>,
    pub cholesterol: Option<String>,
    pub sodium: Option<String>,
    pub potassium: Option<String>,
    pub fiber: Option<String>,
    pub sugar: Option<String>,
    pub added_sugars: Option<String>,
    pub vitamin_d: Option<String>,
    pub vitamin_a: Option<String>,
    pub vitamin_c: Option<String>,
    pub calcium: Option<String>,
    pub iron: Option<String>,
}

impl CreateFoodParams {
    pub fn to_query(&self) -> Query {
        let mut q = vec![
            ("brand_name".to_string(), self.brand_name.clone()),
            ("food_name".to_string(), self.food_name.clone()),
            ("serving_size".to_string(), self.serving_size.clone()),
            ("calories".to_string(), self.calories.to_string()),
            ("fat".to_string(), self.fat.to_string()),
            ("carbohydrate".to_string(), self.carbohydrate.to_string()),
            ("protein".to_string(), self.protein.to_string()),
        ];
        push(&mut q, "serving_amount", &self.serving_amount);
        push(&mut q, "serving_amount_unit", &self.serving_amount_unit);
        push(&mut q, "calories_from_fat", &self.calories_from_fat);
        push(&mut q, "saturated_fat", &self.saturated_fat);
        push(&mut q, "polyunsaturated_fat", &self.polyunsaturated_fat);
        push(&mut q, "monounsaturated_fat", &self.monounsaturated_fat);
        push(&mut q, "trans_fat", &self.trans_fat);
        push(&mut q, "cholesterol", &self.cholesterol);
        push(&mut q, "sodium", &self.sodium);
        push(&mut q, "potassium", &self.potassium);
        push(&mut q, "fiber", &self.fiber);
        push(&mut q, "sugar", &self.sugar);
        push(&mut q, "added_sugars", &self.added_sugars);
        push(&mut q, "vitamin_d", &self.vitamin_d);
        push(&mut q, "vitamin_a", &self.vitamin_a);
        push(&mut q, "vitamin_c", &self.vitamin_c);
        push(&mut q, "calcium", &self.calcium);
        push(&mut q, "iron", &self.iron);
        q
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn dates_to_days() {
        assert_eq!(date_to_days("1970-01-01").unwrap(), 0);
        assert_eq!(date_to_days("2025-01-01").unwrap(), 20089);
        assert!(date_to_days("nope").is_err());
    }
}
