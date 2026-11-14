//! rmcp-based MCP server for FatSecret.
//!
//! `FatSecretServer` owns one `FatSecretClient`. Per-user OAuth1 credentials are injected by
//! building the client with them (`FatSecretServer::with_credentials`), so the HTTP layer can
//! create one server per authenticated user. Further tool groups (profile tools) can be added
//! as additional `#[tool_router(router = ...)]` blocks and combined in `new`.

use std::sync::Arc;

use rmcp::{
    ErrorData, Peer, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ProgressNotificationParam, ProgressToken},
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde_json::Value;

use crate::fatsecret::client::{FatSecretClient, FatSecretError};
use crate::fatsecret::oauth1::OAuth1Credentials;

pub mod args;
pub mod diary_plan;

use diary_plan::{ProgressSink, SaveWeeklyFoodDiaryParams, Timing};

use args::{
    AddSavedMealItemParams, AutocompleteFoodsParams, CopyFoodEntriesParams,
    CopySavedMealEntriesParams, CreateFoodEntryParams, CreateFoodParams, CreateSavedMealParams,
    DeleteFavoriteFoodParams, DeleteFoodEntryParams, DeleteSavedMealItemParams,
    EditExerciseEntriesParams, EditFoodEntryParams, EditSavedMealItemParams, EditSavedMealParams,
    EmptyParams, FindFoodByBarcodeParams, GetBrandsParams, GetFoodCategoriesParams,
    GetFoodEntriesMonthParams, GetFoodEntriesParams, GetFoodParams, GetFoodSubCategoriesParams,
    GetRecipeParams, GetSavedMealsParams, MealFilterParams, MonthDateParams, RecipeIdParams,
    SaveExerciseTemplateParams, SavedMealIdParams, SearchFoodsParams, SearchRecipesParams,
    UpdateWeightParams,
};

pub const SERVER_NAME: &str = "fatsecret-mcp";

pub const SERVER_INSTRUCTIONS: &str = "FatSecret MCP server. Each user authorizes their own FatSecret account.\n\nPUBLIC API: Food search, recipes, brands, categories (OAuth 2.0, app-level).\nPROFILE API: Food diary, saved meals, favorites, weight, exercises, profile (per-user OAuth 1.0).\n\nIf a tool fails with an auth error, the user should disconnect and reconnect via Claude.ai.";

/// Pretty JSON text result (equivalent of JS `text()`).
pub(crate) fn json_text(value: &Value) -> Result<CallToolResult, ErrorData> {
    let text = match value {
        Value::Null => "undefined".to_string(),
        v => serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()),
    };
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

/// Tool-level error result (`isError: true`).
pub(crate) fn tool_error(err: impl std::fmt::Display) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::error(vec![ContentBlock::text(
        err.to_string(),
    )]))
}

/// Converts a client result into a tool result.
pub(crate) fn respond(res: Result<Value, FatSecretError>) -> Result<CallToolResult, ErrorData> {
    match res {
        Ok(v) => json_text(&v),
        Err(e) => tool_error(e),
    }
}

/// Sends MCP `notifications/progress` for the request's progress token (no-op without one).
struct PeerProgress {
    peer: Peer<RoleServer>,
    token: Option<ProgressToken>,
}

impl ProgressSink for PeerProgress {
    async fn report(&self, progress: f64, total: usize, message: String) {
        let Some(token) = self.token.clone() else {
            return;
        };
        let param = ProgressNotificationParam::new(token, progress)
            .with_total(total as f64)
            .with_message(message);
        if let Err(e) = self.peer.notify_progress(param).await {
            tracing::debug!("progress notification failed: {e}");
        }
    }
}

#[derive(Clone)]
pub struct FatSecretServer {
    pub(crate) client: Arc<FatSecretClient>,
    tool_router: ToolRouter<Self>,
}

impl FatSecretServer {
    pub fn new(client: Arc<FatSecretClient>) -> Self {
        Self {
            client,
            tool_router: Self::public_tool_router()
                + Self::profile_tool_router()
                + Self::profile_b_tool_router(),
        }
    }

    /// Builds a server (and client) from app credentials plus OAuth1 consumer/user tokens.
    pub fn with_credentials(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        oauth1: OAuth1Credentials,
    ) -> Self {
        Self::new(Arc::new(FatSecretClient::new(
            client_id,
            client_secret,
            oauth1,
        )))
    }
}

#[tool_router(router = public_tool_router, vis = "pub")]
impl FatSecretServer {
    #[tool(
        description = "Search the FatSecret food database. Returns food names, descriptions, and basic nutrition info.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn search_foods(
        &self,
        Parameters(p): Parameters<SearchFoodsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.search_foods(&p.to_query()).await)
    }

    #[tool(
        description = "Get detailed nutritional information for a specific food by ID. Returns servings, calories, macros, and micronutrients.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn get_food(
        &self,
        Parameters(p): Parameters<GetFoodParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_food(&p.to_query()).await)
    }

    #[tool(
        description = "Find food by barcode (GTIN-13). UPC-A, EAN-13 and EAN-8 supported. Premier exclusive.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn find_food_by_barcode(
        &self,
        Parameters(p): Parameters<FindFoodByBarcodeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.find_food_by_barcode(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Get autocomplete suggestions for a partial food search expression. Premier exclusive.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn autocomplete_foods(
        &self,
        Parameters(p): Parameters<AutocompleteFoodsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.autocomplete_foods(&p.to_query()).await)
    }

    #[tool(
        description = "Search recipes with optional filters for calories, macros, prep time, and recipe types.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn search_recipes(
        &self,
        Parameters(p): Parameters<SearchRecipesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.search_recipes(&p.to_query()).await)
    }

    #[tool(
        description = "Get detailed recipe information by ID including ingredients, directions, and nutrition.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn get_recipe(
        &self,
        Parameters(p): Parameters<GetRecipeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_recipe(&p.to_query()).await)
    }

    #[tool(
        description = "Get the full list of food categories. Premier exclusive.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_food_categories(
        &self,
        Parameters(p): Parameters<GetFoodCategoriesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_food_categories(&p.to_query()).await)
    }

    #[tool(
        description = "Get food sub categories for a given food category. Premier exclusive.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_food_sub_categories(
        &self,
        Parameters(p): Parameters<GetFoodSubCategoriesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_food_sub_categories(&p.to_query()).await)
    }

    #[tool(
        description = "Get the list of food brands, optionally filtered by starting letter and type. Premier exclusive.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_brands(
        &self,
        Parameters(p): Parameters<GetBrandsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_brands(&p.to_query()).await)
    }

    #[tool(
        description = "Get the full list of supported recipe type names.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_recipe_types(
        &self,
        Parameters(_): Parameters<EmptyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_recipe_types().await)
    }
}

#[tool_router(router = profile_tool_router, vis = "pub")]
impl FatSecretServer {
    #[tool(
        description = "Get food diary entries for a date or a specific entry by ID. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_food_entries(
        &self,
        Parameters(p): Parameters<GetFoodEntriesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.get_food_entries(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Get daily nutrition summary for a month. Returns calories and macros per day. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_food_entries_month(
        &self,
        Parameters(p): Parameters<GetFoodEntriesMonthParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.get_food_entries_month(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Add a food diary entry. Requires food_id, serving_id, and meal type. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = false)
    )]
    async fn create_food_entry(
        &self,
        Parameters(p): Parameters<CreateFoodEntryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.create_food_entry(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Edit an existing food diary entry. Cannot change the date. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = true)
    )]
    async fn edit_food_entry(
        &self,
        Parameters(p): Parameters<EditFoodEntryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.edit_food_entry(&p.to_query()).await)
    }

    #[tool(
        description = "Delete a food diary entry by ID. Requires profile auth (check_auth_status first).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn delete_food_entry(
        &self,
        Parameters(p): Parameters<DeleteFoodEntryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.delete_food_entry(&p.to_query()).await)
    }

    #[tool(
        description = "Copy food entries from one date to another, optionally filtered by meal. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = false)
    )]
    async fn copy_food_entries(
        &self,
        Parameters(p): Parameters<CopyFoodEntriesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.copy_food_entries(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Copy entries from a saved meal to a meal on a specific date. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = false)
    )]
    async fn copy_saved_meal_entries(
        &self,
        Parameters(p): Parameters<CopySavedMealEntriesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.copy_saved_meal_entries(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Get the user's favorite foods. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_favorite_foods(
        &self,
        Parameters(_): Parameters<EmptyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_favorite_foods(&[]).await)
    }

    #[tool(
        description = "Remove a food from the user's favorites. Requires profile auth (check_auth_status first).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn delete_favorite_food(
        &self,
        Parameters(p): Parameters<DeleteFavoriteFoodParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.delete_favorite_food(&p.to_query()).await)
    }

    #[tool(
        description = "Get the user's most eaten foods, optionally filtered by meal. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_most_eaten_foods(
        &self,
        Parameters(p): Parameters<MealFilterParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_most_eaten_foods(&p.to_query()).await)
    }

    #[tool(
        description = "Get the user's recently eaten foods, optionally filtered by meal. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_recently_eaten_foods(
        &self,
        Parameters(p): Parameters<MealFilterParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_recently_eaten_foods(&p.to_query()).await)
    }

    #[tool(
        description = "Get the user's favorite recipes. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_favorite_recipes(
        &self,
        Parameters(_): Parameters<EmptyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_favorite_recipes(&[]).await)
    }

    #[tool(
        description = "Add a recipe to the user's favorites. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = true)
    )]
    async fn add_favorite_recipe(
        &self,
        Parameters(p): Parameters<RecipeIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.add_favorite_recipe(&p.to_query()).await)
    }

    #[tool(
        description = "Remove a recipe from the user's favorites. Requires profile auth (check_auth_status first).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn delete_favorite_recipe(
        &self,
        Parameters(p): Parameters<RecipeIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.delete_favorite_recipe(&p.to_query()).await)
    }

    #[tool(
        description = "Get the user's saved meals, optionally filtered by meal type. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_saved_meals(
        &self,
        Parameters(p): Parameters<GetSavedMealsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_saved_meals(&p.to_query()).await)
    }

    #[tool(
        description = "Create a new saved meal. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = false)
    )]
    async fn create_saved_meal(
        &self,
        Parameters(p): Parameters<CreateSavedMealParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.create_saved_meal(&p.to_query()).await)
    }

    #[tool(
        description = "Edit a saved meal name, description, or associated meals. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = true)
    )]
    async fn edit_saved_meal(
        &self,
        Parameters(p): Parameters<EditSavedMealParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.edit_saved_meal(&p.to_query()).await)
    }

    #[tool(
        description = "Delete a saved meal. Requires profile auth (check_auth_status first).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn delete_saved_meal(
        &self,
        Parameters(p): Parameters<SavedMealIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.delete_saved_meal(&p.to_query()).await)
    }

    #[tool(
        description = "Get all food items in a saved meal. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_saved_meal_items(
        &self,
        Parameters(p): Parameters<SavedMealIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_saved_meal_items(&p.to_query()).await)
    }

    #[tool(
        description = "Add a food item to a saved meal. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = false)
    )]
    async fn add_saved_meal_item(
        &self,
        Parameters(p): Parameters<AddSavedMealItemParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.add_saved_meal_item(&p.to_query()).await)
    }

    #[tool(
        description = "Edit a food item in a saved meal (name or units). Cannot change serving_id. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = true)
    )]
    async fn edit_saved_meal_item(
        &self,
        Parameters(p): Parameters<EditSavedMealItemParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.edit_saved_meal_item(&p.to_query()).await)
    }

    #[tool(
        description = "Remove a food item from a saved meal. Requires profile auth (check_auth_status first).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn delete_saved_meal_item(
        &self,
        Parameters(p): Parameters<DeleteSavedMealItemParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.delete_saved_meal_item(&p.to_query()).await)
    }
}

#[tool_router(router = profile_b_tool_router, vis = "pub")]
impl FatSecretServer {
    #[tool(
        description = "Save a full weekly meal plan to the FatSecret food diary (one create-food-entry call per item). Input: plan JSON (days -> meals -> items with food_id, serving_id, number_of_units), start_date (YYYY-MM-DD of day 1) and overwrite. overwrite=true deletes existing diary entries of the same meal types on those dates first; overwrite=false skips plan meals whose meal type already has entries. Rate limits are retried after 30s. Reports progress as 'A of B meals processed'. Takes minutes for a full week. Requires profile auth (check_auth_status first).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn save_weekly_food_diary(
        &self,
        Parameters(p): Parameters<SaveWeeklyFoodDiaryParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Err(e) = self.client.ensure_profile_auth() {
            return tool_error(e);
        }
        let sink = PeerProgress {
            token: ctx.meta.get_progress_token(),
            peer: ctx.peer,
        };
        match diary_plan::save_weekly_food_diary(&*self.client, &sink, &p, Timing::default()).await
        {
            Ok(report) => json_text(&diary_plan::report_json(&report)),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Record the user's weight for a date. First weigh-in requires goal_weight_kg and current_height_cm. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = true)
    )]
    async fn update_weight(
        &self,
        Parameters(p): Parameters<UpdateWeightParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.update_weight(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Get the user's weight entries for a month. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_weight_month(
        &self,
        Parameters(p): Parameters<MonthDateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.get_weight_month(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Get the full list of supported exercise types and their IDs. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_exercises(
        &self,
        Parameters(_): Parameters<EmptyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_exercises().await)
    }

    #[tool(
        description = "Shift exercise time between activities for a date. Moves minutes from one exercise to another. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = false)
    )]
    async fn edit_exercise_entries(
        &self,
        Parameters(p): Parameters<EditExerciseEntriesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.edit_exercise_entries(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Get daily calories expended from exercise for a month. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_exercise_entries_month(
        &self,
        Parameters(p): Parameters<MonthDateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.get_exercise_entries_month(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Save the current day's exercise entries as a template for specified days of the week. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = true)
    )]
    async fn save_exercise_template(
        &self,
        Parameters(p): Parameters<SaveExerciseTemplateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match p.to_query() {
            Ok(q) => respond(self.client.save_exercise_template(&q).await),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Get profile status information for the authenticated user. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn get_profile(
        &self,
        Parameters(_): Parameters<EmptyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.get_profile().await)
    }

    #[tool(
        description = "Create a custom food with nutrition info. Premier exclusive. Requires profile auth (check_auth_status first).",
        annotations(read_only_hint = false, idempotent_hint = false)
    )]
    async fn create_food(
        &self,
        Parameters(p): Parameters<CreateFoodParams>,
    ) -> Result<CallToolResult, ErrorData> {
        respond(self.client.create_food(&p.to_query()).await)
    }

    #[tool(
        description = "Report which capability tier is available for the current authenticated user.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn check_auth_status(
        &self,
        Parameters(_): Parameters<EmptyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        json_text(&auth_status(
            self.client.has_api_credentials(),
            self.client.has_profile_tokens(),
        ))
    }
}

/// Mirrors JS `check_auth_status` output (no API call is made).
fn auth_status(has_credentials: bool, has_tokens: bool) -> Value {
    if !has_credentials {
        return serde_json::json!({
            "credentials_configured": false,
            "profile_authenticated": false,
            "message": "Server-level FatSecret API credentials are missing. Contact the server operator.",
        });
    }
    serde_json::json!({
        "credentials_configured": true,
        "profile_authenticated": has_tokens,
        "message": if has_tokens {
            "Fully configured. Public and profile tools are available for this user."
        } else {
            "Public tools available, profile tools disabled. Reconnect via Claude.ai to authorize FatSecret."
        },
    })
}

#[tool_handler(
    router = self.tool_router,
    name = "fatsecret-mcp",
    instructions = "FatSecret MCP server. Each user authorizes their own FatSecret account.\n\nPUBLIC API: Food search, recipes, brands, categories (OAuth 2.0, app-level).\nPROFILE API: Food diary, saved meals, favorites, weight, exercises, profile (per-user OAuth 1.0).\n\nIf a tool fails with an auth error, the user should disconnect and reconnect via Claude.ai."
)]
impl rmcp::ServerHandler for FatSecretServer {}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::ServerHandler;

    fn server() -> FatSecretServer {
        FatSecretServer::with_credentials("", "", OAuth1Credentials::default())
    }

    #[test]
    fn info_matches_js() {
        let info = server().get_info();
        assert_eq!(info.server_info.name, "fatsecret-mcp");
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.instructions.as_deref(), Some(SERVER_INSTRUCTIONS));
    }

    #[test]
    fn tools_match_fixture_metadata() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/tools_baseline.json")).unwrap();
        let tools = server().tool_router.list_all();
        assert_eq!(tools.len(), 42);
        let mut got: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        got.sort();
        let mut want: Vec<String> = fixture["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        want.sort();
        assert_eq!(got, want);
        for t in tools {
            let f = fixture["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["name"] == t.name.as_ref())
                .unwrap_or_else(|| panic!("{} not in fixture", t.name));
            assert_eq!(
                f["description"].as_str(),
                t.description.as_deref(),
                "{}",
                t.name
            );
            let a = t.annotations.as_ref().unwrap();
            let fa = &f["annotations"];
            assert_eq!(fa["readOnlyHint"].as_bool(), a.read_only_hint, "{}", t.name);
            assert_eq!(
                fa["idempotentHint"].as_bool(),
                a.idempotent_hint,
                "{}",
                t.name
            );
            assert_eq!(
                fa["openWorldHint"].as_bool(),
                a.open_world_hint,
                "{}",
                t.name
            );
            let req: Vec<String> = f["inputSchema"]["required"]
                .as_array()
                .map(|r| r.iter().map(|s| s.as_str().unwrap().to_string()).collect())
                .unwrap_or_default();
            let mut got: Vec<String> = t
                .input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|r| r.iter().map(|s| s.as_str().unwrap().to_string()).collect())
                .unwrap_or_default();
            got.sort();
            let mut req = req;
            req.sort();
            assert_eq!(req, got, "{}", t.name);
            let keys = |v: Option<&Value>| -> Vec<String> {
                let mut k: Vec<String> = v
                    .and_then(|p| p.as_object())
                    .map(|o| o.keys().cloned().collect())
                    .unwrap_or_default();
                k.sort();
                k
            };
            let props = t.input_schema.get("properties");
            // Superset: Rust-only extra properties (FatSecret v6/v3 params) are allowed.
            let got_keys = keys(props);
            for k in keys(f["inputSchema"].get("properties")) {
                assert!(got_keys.contains(&k), "{} missing property {k}", t.name);
            }
            for (pk, pv) in f["inputSchema"]["properties"]
                .as_object()
                .into_iter()
                .flatten()
            {
                let gp = &props.unwrap()[pk];
                // schemars renders Option<T> as ["T","null"]; compare the non-null type.
                let ty = match &gp["type"] {
                    Value::Array(a) => a.iter().find(|x| *x != "null").cloned().unwrap(),
                    other => other.clone(),
                };
                assert_eq!(pv["type"], ty, "{}.{} type", t.name, pk);
                assert_eq!(pv["enum"], gp["enum"], "{}.{} enum", t.name, pk);
            }
        }
    }

    #[test]
    fn auth_status_matches_js() {
        let v = auth_status(false, true);
        assert_eq!(v["credentials_configured"], false);
        assert_eq!(v["profile_authenticated"], false);
        let v = auth_status(true, false);
        assert_eq!(v["profile_authenticated"], false);
        assert!(v["message"].as_str().unwrap().starts_with("Public tools"));
        let v = auth_status(true, true);
        assert_eq!(v["profile_authenticated"], true);
    }

    #[test]
    fn json_text_is_pretty() {
        let r = json_text(&serde_json::json!({"a": 1})).unwrap();
        let s = serde_json::to_string(&r.content).unwrap();
        assert!(s.contains("{\\n  \\\"a\\\": 1\\n}"));
    }
}
