//! `save_weekly_food_diary`: writes a whole weekly meal plan into the FatSecret food diary.
//!
//! The orchestration is generic over [`DiaryApi`] (the three diary calls it needs) and
//! [`ProgressSink`] (MCP progress reporting) so it can be unit-tested without network or
//! real sleeps. Rate limiting (HTTP 429 or FatSecret error code 12) is handled per request:
//! the same request is retried after [`Timing::rate_limit_wait`] (30 s in production).

use std::future::Future;
use std::time::Duration;

use chrono::{Days, NaiveDate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::args::{CreateFoodEntryParams, Meal, Query, date_to_days};
use crate::fatsecret::client::{FatSecretClient, FatSecretError};

type ApiResult = Result<Value, FatSecretError>;

// ---------------------------------------------------------------------------------------------
// Tool input
// ---------------------------------------------------------------------------------------------

/// Weekly plan in the `weekly_plan_to_fatsecret` format (extra fields are ignored).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct WeeklyPlan {
    /// Short plan identifier, e.g. "week7" (used in the report only)
    pub plan_name: Option<String>,
    /// Days in order, first day = `start_date` (up to 7)
    pub days: Vec<PlanDay>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct PlanDay {
    /// Day name, e.g. "Monday"
    pub name: String,
    pub meals: Vec<PlanMeal>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct PlanMeal {
    /// Meal name, e.g. "Breakfast"
    pub label: String,
    /// FatSecret meal type: breakfast, lunch, dinner or other (snacks). If several are
    /// comma-separated, the first one is used for the diary entries.
    pub fatsecret_meal_type: String,
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct PlanItem {
    /// Food name, used as the diary entry name
    pub name: String,
    /// FatSecret food ID
    pub food_id: i64,
    /// FatSecret serving ID
    pub serving_id: i64,
    /// Number of servings
    pub number_of_units: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveWeeklyFoodDiaryParams {
    /// Full weekly meal plan (every item needs food_id, serving_id, number_of_units)
    pub plan: WeeklyPlan,
    /// Date of the first plan day, YYYY-MM-DD (day N goes to start_date + N-1)
    pub start_date: String,
    /// true: delete existing diary entries of the same meal types on the plan's dates before
    /// writing. false (default): leave those meals untouched and skip the plan meals for them.
    #[serde(default)]
    pub overwrite: bool,
}

// ---------------------------------------------------------------------------------------------
// Seams
// ---------------------------------------------------------------------------------------------

pub trait DiaryApi: Sync {
    fn get_entries(&self, q: &Query) -> impl Future<Output = ApiResult> + Send;
    fn create_entry(&self, q: &Query) -> impl Future<Output = ApiResult> + Send;
    fn delete_entry(&self, q: &Query) -> impl Future<Output = ApiResult> + Send;
}

impl DiaryApi for FatSecretClient {
    fn get_entries(&self, q: &Query) -> impl Future<Output = ApiResult> + Send {
        self.get_food_entries(q)
    }
    fn create_entry(&self, q: &Query) -> impl Future<Output = ApiResult> + Send {
        self.create_food_entry(q)
    }
    fn delete_entry(&self, q: &Query) -> impl Future<Output = ApiResult> + Send {
        self.delete_food_entry(q)
    }
}

pub trait ProgressSink: Sync {
    /// `progress` is strictly increasing across calls; `total` is the number of meals.
    fn report(
        &self,
        progress: f64,
        total: usize,
        message: String,
    ) -> impl Future<Output = ()> + Send;
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Pause before every API request.
    pub request_delay: Duration,
    /// Pause before retrying a rate-limited request.
    pub rate_limit_wait: Duration,
    /// Rate-limit retries per request before giving up on it.
    pub max_rate_limit_retries: u32,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            request_delay: Duration::from_secs(1),
            rate_limit_wait: Duration::from_secs(30),
            max_rate_limit_retries: 5,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Default)]
pub struct MealResult {
    pub date: String,
    pub day: String,
    pub meal: String,
    /// created | partial | skipped_existing | failed
    pub status: &'static str,
    pub entries_created: usize,
}

#[derive(Debug, Serialize)]
pub struct Failure {
    pub date: String,
    pub meal: String,
    pub item: Option<String>,
    pub error: String,
}

#[derive(Debug, Serialize, Default)]
pub struct Report {
    pub plan_name: Option<String>,
    pub overwrite: bool,
    pub summary: String,
    pub meals_total: usize,
    pub meals_created: usize,
    pub meals_partial: usize,
    pub meals_skipped_existing: usize,
    pub meals_failed: usize,
    pub entries_created: usize,
    pub entries_deleted: usize,
    pub rate_limit_waits: u32,
    pub meals: Vec<MealResult>,
    pub failures: Vec<Failure>,
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// FatSecret may answer `200` with `{"error":{"code":..,"message":..}}`; turn that into an Err.
fn normalize(res: ApiResult) -> ApiResult {
    match res {
        Ok(v) if v.get("error").is_some_and(|e| !e.is_null()) => Err(FatSecretError::Api {
            status: 200,
            message: v.to_string(),
        }),
        other => other,
    }
}

fn existing_entries(v: &Value) -> Vec<&Value> {
    match v.get("food_entries").and_then(|e| e.get("food_entry")) {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(o @ Value::Object(_)) => vec![o],
        _ => vec![],
    }
}

fn entry_id(e: &Value) -> Option<String> {
    match e.get("food_entry_id")? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn entry_meal(e: &Value) -> String {
    e.get("meal")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase()
}

fn plan_meal_type(meal: &PlanMeal) -> Option<Meal> {
    Meal::parse(
        meal.fatsecret_meal_type
            .split(',')
            .next()
            .unwrap_or_default(),
    )
}

// ---------------------------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------------------------

struct Runner<'a, A: DiaryApi, S: ProgressSink> {
    api: &'a A,
    sink: &'a S,
    timing: Timing,
    total: usize,
    done: usize,
    last_progress: f64,
    report: Report,
}

enum Op {
    Get,
    Create,
    Delete,
}

impl<A: DiaryApi, S: ProgressSink> Runner<'_, A, S> {
    async fn emit(&mut self, wanted: f64, message: String) {
        // MCP requires progress to increase with every notification.
        let p = if wanted > self.last_progress {
            wanted
        } else {
            self.last_progress + 0.001
        };
        self.last_progress = p;
        self.sink.report(p, self.total, message).await;
    }

    fn counter(&self) -> String {
        format!("{} of {} meals processed", self.done, self.total)
    }

    /// One API request with pacing and rate-limit retries.
    async fn call(&mut self, op: Op, q: &Query) -> ApiResult {
        let mut retries = 0;
        loop {
            tokio::time::sleep(self.timing.request_delay).await;
            let res = normalize(match op {
                Op::Get => self.api.get_entries(q).await,
                Op::Create => self.api.create_entry(q).await,
                Op::Delete => self.api.delete_entry(q).await,
            });
            match res {
                Err(e) if e.is_rate_limited() && retries < self.timing.max_rate_limit_retries => {
                    retries += 1;
                    self.report.rate_limit_waits += 1;
                    let msg = format!(
                        "Rate limited by FatSecret, waiting {}s (retry {retries}/{}). {}",
                        self.timing.rate_limit_wait.as_secs(),
                        self.timing.max_rate_limit_retries,
                        self.counter()
                    );
                    self.emit(self.done as f64, msg).await;
                    tokio::time::sleep(self.timing.rate_limit_wait).await;
                }
                other => return other,
            }
        }
    }

    fn fail(&mut self, date: &str, meal: &str, item: Option<&str>, error: impl ToString) {
        self.report.failures.push(Failure {
            date: date.to_string(),
            meal: meal.to_string(),
            item: item.map(String::from),
            error: error.to_string(),
        });
    }

    async fn finish_meal(
        &mut self,
        day: &PlanDay,
        date: &str,
        meal: &PlanMeal,
        status: &'static str,
        created: usize,
    ) {
        match status {
            "created" => self.report.meals_created += 1,
            "partial" => self.report.meals_partial += 1,
            "skipped_existing" => self.report.meals_skipped_existing += 1,
            _ => self.report.meals_failed += 1,
        }
        self.report.meals.push(MealResult {
            date: date.to_string(),
            day: day.name.clone(),
            meal: meal.label.clone(),
            status,
            entries_created: created,
        });
        self.done += 1;
        let msg = format!(
            "{} ({}, {}: {status})",
            self.counter(),
            day.name,
            meal.label
        );
        self.emit(self.done as f64, msg).await;
    }

    async fn run_day(&mut self, day: &PlanDay, date: NaiveDate, overwrite: bool) {
        let date_str = date.format("%Y-%m-%d").to_string();
        let days = match date_to_days(&date_str) {
            Ok(d) => d.to_string(),
            Err(e) => return self.fail_day(day, &date_str, e).await,
        };

        // Meal types this day's plan touches.
        let mut types: Vec<&'static str> = day
            .meals
            .iter()
            .filter_map(plan_meal_type)
            .map(Meal::as_str)
            .collect();
        types.sort_unstable();
        types.dedup();

        let existing = match self.call(Op::Get, &vec![("date".into(), days)]).await {
            Ok(v) => v,
            Err(e) => {
                return self
                    .fail_day(
                        day,
                        &date_str,
                        format!("could not read existing diary entries: {e}"),
                    )
                    .await;
            }
        };
        let conflicting: Vec<(String, &'static str)> = existing_entries(&existing)
            .into_iter()
            .filter_map(|e| {
                let m = entry_meal(e);
                let t = types.iter().find(|t| **t == m)?;
                Some((entry_id(e)?, *t))
            })
            .collect();

        // Meal types that must not be written (delete failed / not overwriting).
        let mut blocked: Vec<(&'static str, &'static str)> = vec![]; // (type, status)
        if overwrite {
            for (id, t) in conflicting {
                match self
                    .call(Op::Delete, &vec![("food_entry_id".into(), id.clone())])
                    .await
                {
                    Ok(_) => self.report.entries_deleted += 1,
                    Err(e) => {
                        self.fail(
                            &date_str,
                            t,
                            None,
                            format!("could not delete entry {id}: {e}"),
                        );
                        if !blocked.iter().any(|(bt, _)| *bt == t) {
                            blocked.push((t, "failed"));
                        }
                    }
                }
            }
        } else {
            for (_, t) in conflicting {
                if !blocked.iter().any(|(bt, _)| *bt == t) {
                    blocked.push((t, "skipped_existing"));
                }
            }
        }

        for meal in &day.meals {
            let Some(mt) = plan_meal_type(meal) else {
                self.fail(
                    &date_str,
                    &meal.label,
                    None,
                    format!("unsupported meal type {:?}", meal.fatsecret_meal_type),
                );
                self.finish_meal(day, &date_str, meal, "failed", 0).await;
                continue;
            };
            if let Some((_, status)) = blocked.iter().find(|(t, _)| *t == mt.as_str()) {
                self.finish_meal(day, &date_str, meal, status, 0).await;
                continue;
            }
            let mut created = 0;
            for item in &meal.items {
                let q = CreateFoodEntryParams {
                    food_id: item.food_id,
                    food_entry_name: item.name.clone(),
                    serving_id: item.serving_id,
                    number_of_units: item.number_of_units,
                    meal: mt,
                    date: Some(date_str.clone()),
                }
                .to_query();
                let res = match q {
                    Ok(q) => self.call(Op::Create, &q).await,
                    Err(e) => Err(FatSecretError::Api {
                        status: 400,
                        message: e,
                    }),
                };
                match res {
                    Ok(_) => created += 1,
                    Err(e) => self.fail(&date_str, &meal.label, Some(&item.name), e),
                }
            }
            let status = match (created, meal.items.len()) {
                (c, n) if c == n => "created",
                (0, _) => "failed",
                _ => "partial",
            };
            self.report.entries_created += created;
            self.finish_meal(day, &date_str, meal, status, created)
                .await;
        }
    }

    async fn fail_day(&mut self, day: &PlanDay, date: &str, error: impl ToString) {
        let error = error.to_string();
        for meal in &day.meals {
            self.fail(date, &meal.label, None, &error);
            self.finish_meal(day, date, meal, "failed", 0).await;
        }
    }
}

pub async fn save_weekly_food_diary<A: DiaryApi, S: ProgressSink>(
    api: &A,
    sink: &S,
    params: &SaveWeeklyFoodDiaryParams,
    timing: Timing,
) -> Result<Report, String> {
    let plan = &params.plan;
    if plan.days.is_empty() || plan.days.len() > 7 {
        return Err(format!(
            "plan.days must contain 1-7 days, got {}",
            plan.days.len()
        ));
    }
    let start = NaiveDate::parse_from_str(&params.start_date, "%Y-%m-%d").map_err(|_| {
        format!(
            "Invalid start_date: {:?} (expected YYYY-MM-DD)",
            params.start_date
        )
    })?;

    let total = plan.days.iter().map(|d| d.meals.len()).sum();
    let mut r = Runner {
        api,
        sink,
        timing,
        total,
        done: 0,
        last_progress: -1.0,
        report: Report {
            plan_name: plan.plan_name.clone(),
            overwrite: params.overwrite,
            meals_total: total,
            ..Default::default()
        },
    };
    r.emit(0.0, r.counter()).await;
    for (i, day) in plan.days.iter().enumerate() {
        let date = start + Days::new(i as u64);
        r.run_day(day, date, params.overwrite).await;
    }
    let mut report = r.report;
    report.summary = format!(
        "{} of {} meals processed: {} created, {} partial, {} skipped (already in diary), {} failed",
        total,
        total,
        report.meals_created,
        report.meals_partial,
        report.meals_skipped_existing,
        report.meals_failed
    );
    Ok(report)
}

pub fn report_json(report: &Report) -> Value {
    serde_json::to_value(report).unwrap_or_else(|_| json!({ "summary": report.summary }))
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    const FAST: Timing = Timing {
        request_delay: Duration::ZERO,
        rate_limit_wait: Duration::ZERO,
        max_rate_limit_retries: 2,
    };

    #[derive(Default)]
    struct MockApi {
        log: Mutex<Vec<String>>,
        /// Scripted answers for create calls (front first); empty = success.
        create_script: Mutex<VecDeque<ApiResult>>,
        existing: Value,
    }

    impl DiaryApi for MockApi {
        async fn get_entries(&self, q: &Query) -> ApiResult {
            self.log.lock().unwrap().push(format!("get {}", q[0].1));
            Ok(self.existing.clone())
        }
        async fn create_entry(&self, q: &Query) -> ApiResult {
            let get = |k: &str| q.iter().find(|(a, _)| a == k).unwrap().1.clone();
            self.log.lock().unwrap().push(format!(
                "create {} {} {}",
                get("food_entry_name"),
                get("meal"),
                get("date")
            ));
            self.create_script
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(json!({"food_entry_id": "1"})))
        }
        async fn delete_entry(&self, q: &Query) -> ApiResult {
            self.log.lock().unwrap().push(format!("delete {}", q[0].1));
            Ok(json!({"success": {"value": 1}}))
        }
    }

    #[derive(Default)]
    struct MockSink(Mutex<Vec<(f64, usize, String)>>);
    impl ProgressSink for MockSink {
        async fn report(&self, progress: f64, total: usize, message: String) {
            self.0.lock().unwrap().push((progress, total, message));
        }
    }

    fn params(overwrite: bool) -> SaveWeeklyFoodDiaryParams {
        serde_json::from_value(json!({
            "plan": {"plan_name": "w1", "days": [
                {"name": "Mon", "meals": [
                    {"label": "Breakfast", "fatsecret_meal_type": "breakfast", "items": [
                        {"name": "Oats", "food_id": 1, "serving_id": 2, "number_of_units": 60},
                        {"name": "Milk", "food_id": 3, "serving_id": 4, "number_of_units": 1.5}]},
                    {"label": "Snack", "fatsecret_meal_type": "other", "items": [
                        {"name": "Apple", "food_id": 5, "serving_id": 6, "number_of_units": 1}]}]},
                {"name": "Tue", "meals": [
                    {"label": "Dinner", "fatsecret_meal_type": "lunch,dinner", "items": [
                        {"name": "Fish", "food_id": 7, "serving_id": 8, "number_of_units": 100}]}]}
            ]},
            "start_date": "2026-11-16",
            "overwrite": overwrite
        }))
        .unwrap()
    }

    fn rate_limited() -> ApiResult {
        Err(FatSecretError::Api {
            status: 429,
            message: "slow down".into(),
        })
    }

    #[tokio::test]
    async fn creates_everything_and_reports_progress() {
        let api = MockApi::default();
        let sink = MockSink::default();
        let r = save_weekly_food_diary(&api, &sink, &params(false), FAST)
            .await
            .unwrap();
        assert_eq!(
            (r.meals_total, r.meals_created, r.entries_created),
            (3, 3, 4)
        );
        let log = api.log.lock().unwrap();
        assert!(log.contains(&format!(
            "create Oats breakfast {}",
            date_to_days("2026-11-16").unwrap()
        )));
        assert!(log.contains(&format!(
            "create Fish lunch {}",
            date_to_days("2026-11-17").unwrap()
        )));
        let p = sink.0.lock().unwrap();
        let msgs: Vec<&str> = p.iter().map(|(_, _, m)| m.as_str()).collect();
        assert_eq!(msgs[0], "0 of 3 meals processed");
        assert!(msgs[1].starts_with("1 of 3 meals processed"));
        assert!(msgs[3].starts_with("3 of 3 meals processed"));
        assert!(
            p.windows(2).all(|w| w[0].0 < w[1].0),
            "progress must increase"
        );
        assert!(p.iter().all(|(_, t, _)| *t == 3));
    }

    #[tokio::test]
    async fn rate_limit_is_retried_and_reported() {
        let api = MockApi::default();
        api.create_script.lock().unwrap().extend([
            rate_limited(),
            Ok(json!({"error": {"code": 12, "message": "limit"}})),
        ]);
        let sink = MockSink::default();
        let r = save_weekly_food_diary(&api, &sink, &params(false), FAST)
            .await
            .unwrap();
        assert_eq!(r.rate_limit_waits, 2);
        assert_eq!((r.meals_created, r.meals_failed), (3, 0));
        let creates = api
            .log
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with("create Oats"))
            .count();
        assert_eq!(creates, 3);
        let p = sink.0.lock().unwrap();
        assert!(p.iter().any(|(_, _, m)| m.starts_with("Rate limited")));
        assert!(p.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[tokio::test]
    async fn rate_limit_gives_up_after_max_retries() {
        let api = MockApi::default();
        api.create_script
            .lock()
            .unwrap()
            .extend([rate_limited(), rate_limited(), rate_limited()]);
        let r = save_weekly_food_diary(&api, &MockSink::default(), &params(false), FAST)
            .await
            .unwrap();
        assert_eq!(r.meals_partial, 1); // Oats failed, Milk ok
        assert_eq!(r.failures.len(), 1);
        assert_eq!(r.failures[0].item.as_deref(), Some("Oats"));
    }

    fn existing_breakfast() -> Value {
        json!({"food_entries": {"food_entry": {"food_entry_id": "99", "meal": "Breakfast"}}})
    }

    #[tokio::test]
    async fn no_overwrite_skips_existing_meal_type() {
        let api = MockApi {
            existing: existing_breakfast(),
            ..Default::default()
        };
        let r = save_weekly_food_diary(&api, &MockSink::default(), &params(false), FAST)
            .await
            .unwrap();
        // Existing breakfast is reported by the mock for every date: Mon breakfast skipped,
        // Tue has no breakfast in the plan.
        assert_eq!(r.meals_skipped_existing, 1);
        assert_eq!(r.entries_deleted, 0);
        assert!(
            !api.log
                .lock()
                .unwrap()
                .iter()
                .any(|l| l.starts_with("delete") || l.starts_with("create Oats"))
        );
    }

    #[tokio::test]
    async fn overwrite_deletes_existing_then_creates() {
        let api = MockApi {
            existing: existing_breakfast(),
            ..Default::default()
        };
        let r = save_weekly_food_diary(&api, &MockSink::default(), &params(true), FAST)
            .await
            .unwrap();
        assert_eq!((r.meals_created, r.entries_deleted), (3, 1));
        let log = api.log.lock().unwrap();
        let del = log.iter().position(|l| l == "delete 99").unwrap();
        let cre = log
            .iter()
            .position(|l| l.starts_with("create Oats"))
            .unwrap();
        assert!(del < cre);
    }

    #[tokio::test]
    async fn validates_input() {
        let mut p = params(false);
        p.start_date = "nope".into();
        assert!(
            save_weekly_food_diary(&MockApi::default(), &MockSink::default(), &p, FAST)
                .await
                .is_err()
        );
        let mut p = params(false);
        p.plan.days.clear();
        assert!(
            save_weekly_food_diary(&MockApi::default(), &MockSink::default(), &p, FAST)
                .await
                .is_err()
        );
    }
}
