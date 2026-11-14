//! Compares the rmcp-generated tool list against the legacy JS `tools_baseline.json` fixture:
//! tool names, descriptions, annotations, top-level required fields and property names/types.
//!
//! Intentional schema differences from the legacy hand-written (zod) schemas (schemars output):
//! - optional fields are typed nullable (e.g. `["string", "null"]`) or use `anyOf`; only the
//!   non-null type is compared;
//! - extra keywords appear: `format` (e.g. `int64`, `double`), `minimum`, and a
//!   `$schema` of a newer JSON Schema draft instead of draft-07;
//! - nested types may be emitted as `$defs` + `$ref` instead of inlined objects;
//! - `additionalProperties: false` is not emitted;
//! - properties are compared as a superset: Rust-only extras are allowed (FatSecret v6/v3 params
//!   `include_food_classification_data` and `include_nutrient_content_tags`);
//! - only `readOnlyHint`, `idempotentHint` and `openWorldHint` annotations are compared.

use std::collections::BTreeMap;

use fatsecret_mcp_server::server::FatSecretServer;
use serde_json::Value;

fn sorted_strings(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = v
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn sorted_keys(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = v
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    out.sort();
    out
}

#[test]
fn tool_list_matches_baseline() {
    let baseline: Value =
        serde_json::from_str(include_str!("fixtures/tools_baseline.json")).unwrap();
    let expected: BTreeMap<&str, &Value> = baseline["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["name"].as_str().unwrap(), t))
        .collect();
    assert_eq!(expected.len(), 42);

    let router = FatSecretServer::public_tool_router()
        + FatSecretServer::profile_tool_router()
        + FatSecretServer::profile_b_tool_router();
    let tools = router.list_all();
    let actual: BTreeMap<String, _> = tools.iter().map(|t| (t.name.to_string(), t)).collect();

    assert_eq!(
        actual.keys().map(String::as_str).collect::<Vec<_>>(),
        expected.keys().copied().collect::<Vec<_>>(),
        "tool names differ"
    );

    for (name, tool) in &actual {
        let exp = expected[name.as_str()];
        assert_eq!(
            exp["description"].as_str(),
            tool.description.as_deref(),
            "{name}: description"
        );

        let ann = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: missing annotations"));
        assert_eq!(
            exp["annotations"]["readOnlyHint"].as_bool(),
            ann.read_only_hint,
            "{name}: readOnlyHint"
        );
        assert_eq!(
            exp["annotations"]["idempotentHint"].as_bool(),
            ann.idempotent_hint,
            "{name}: idempotentHint"
        );
        assert_eq!(
            exp["annotations"]["openWorldHint"].as_bool(),
            ann.open_world_hint,
            "{name}: openWorldHint"
        );

        let schema = Value::Object((*tool.input_schema).clone());
        assert_eq!(
            sorted_strings(exp["inputSchema"].get("required")),
            sorted_strings(schema.get("required")),
            "{name}: required"
        );
        let got_keys = sorted_keys(schema.get("properties"));
        for k in sorted_keys(exp["inputSchema"].get("properties")) {
            assert!(got_keys.contains(&k), "{name}: missing property {k}");
        }
        for (pk, pv) in exp["inputSchema"]["properties"]
            .as_object()
            .into_iter()
            .flatten()
        {
            let got = &schema["properties"][pk];
            let ty = match &got["type"] {
                Value::Array(a) => a
                    .iter()
                    .find(|x| *x != "null")
                    .cloned()
                    .unwrap_or(Value::Null),
                other => other.clone(),
            };
            assert_eq!(pv["type"], ty, "{name}.{pk}: type");
            assert_eq!(pv["enum"], got["enum"], "{name}.{pk}: enum");
        }
    }
}
