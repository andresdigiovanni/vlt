use std::sync::Arc;

use rmcp::model::{CallToolResult, JsonObject, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};
use serde_json::{json, Value};

use crate::app::App;
use crate::search::SearchRequest;
use crate::VltError;

const INSTRUCTIONS: &str = "vlt: search the user's markdown knowledge base (hybrid search), read documents, inspect properties and, when the user approves, edit documents and their properties. Read and write the vault only through these tools.";

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchArgs {
    /// Free-text query.
    query: String,
    /// Property filter, e.g. {"tags": "rust", "date": {"gte": "2026-01-01"}}.
    #[serde(default, rename = "where")]
    #[schemars(schema_with = "object_or_string_schema")]
    where_: Option<Value>,
    /// Only documents whose vault-relative path starts with this prefix.
    path_prefix: Option<String>,
    /// Maximum number of results.
    #[schemars(range(min = 1))]
    limit: Option<usize>,
    /// Minimum score in [0, 1]; defaults to the configured value.
    min_score: Option<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetArgs {
    /// Vault-relative path of the document.
    path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct PropertiesArgs {
    /// Only documents whose vault-relative path starts with this prefix.
    path_prefix: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EditArgs {
    /// Vault-relative path of the document.
    path: String,
    /// Exact text to replace; must appear exactly once in the body (copy it from `get`'s `content`).
    old_text: String,
    /// Replacement text.
    new_text: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct UpdatePropertiesArgs {
    /// Vault-relative path of the document.
    path: String,
    /// Properties to add or change: string, number, boolean or list of those.
    #[serde(default)]
    #[schemars(schema_with = "object_or_string_schema")]
    set: Option<Value>,
    /// Property keys to remove.
    unset: Option<Vec<String>>,
}

/// The `set` argument as a map: an object, or a string holding one.
fn set_map(set: Option<Value>) -> crate::Result<Option<serde_json::Map<String, Value>>> {
    match set.map(coerce_stringified_json) {
        None => Ok(None),
        Some(Value::Object(m)) => Ok(Some(m)),
        Some(_) => Err(VltError::InvalidArguments("set must be an object of property values".into())),
    }
}

/// An object, or a string holding one (some clients stringify object arguments).
fn object_or_string_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"anyOf": [{"type": "object", "additionalProperties": true}, {"type": "string"}]})
}

/// A string holding JSON is parsed; anything else is kept as sent.
fn coerce_stringified_json(value: Value) -> Value {
    match value {
        Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
        other => other,
    }
}

/// Deserializes tool arguments here rather than in rmcp's extractor, so a bad argument gets the error envelope.
fn parse_args<T: serde::de::DeserializeOwned>(args: JsonObject) -> crate::Result<T> {
    serde_json::from_value(Value::Object(args)).map_err(|e| VltError::InvalidArguments(format!("invalid arguments: {e}")))
}

fn input_schema<T: schemars::JsonSchema + 'static>() -> Arc<JsonObject> {
    rmcp::handler::server::common::schema_for_input::<T>().unwrap_or_else(|e| panic!("invalid input schema: {e}"))
}

fn search_request(args: SearchArgs) -> SearchRequest {
    SearchRequest {
        query: args.query,
        where_: args.where_.map(coerce_stringified_json),
        path_prefix: args.path_prefix,
        limit: args.limit,
        min_score: args.min_score,
    }
}

pub fn error_envelope(e: &VltError) -> serde_json::Value {
    json!({"error": {"code": e.code(), "message": e.to_string()}})
}

/// The pretty-printed text of the typed payload, in field order (what `--json` prints), and its value.
fn json_payload<T: serde::Serialize>(payload: &T) -> crate::Result<(String, Value)> {
    let internal = |e: serde_json::Error| VltError::Internal(e.to_string());
    Ok((serde_json::to_string_pretty(payload).map_err(internal)?, serde_json::to_value(payload).map_err(internal)?))
}

fn json_ok(text: String, value: Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(value);
    result
}

fn tool_error(e: &VltError) -> CallToolResult {
    let value = error_envelope(e);
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    let mut result = CallToolResult::error(vec![ContentBlock::text(text)]);
    result.structured_content = Some(value);
    result
}

pub struct VltServer {
    app: Arc<App>,
}

impl VltServer {
    pub(super) fn new(app: Arc<App>) -> VltServer {
        VltServer { app }
    }

    /// Runs a blocking `App` call off the async thread and wraps its outcome.
    async fn call<T, F>(&self, f: F) -> CallToolResult
    where
        T: serde::Serialize,
        F: FnOnce(&App) -> crate::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let app = self.app.clone();
        let outcome = tokio::task::spawn_blocking(move || f(&app))
            .await
            .unwrap_or_else(|e| Err(VltError::Internal(format!("tool task failed: {e}"))))
            .and_then(|v| json_payload(&v));
        match outcome {
            Ok((text, value)) => json_ok(text, value),
            Err(e) => tool_error(&e),
        }
    }
}

#[tool_router]
impl VltServer {
    #[tool(
        description = "Hybrid (keyword + semantic) search over the vault with property filters, reranking and a relevance threshold. Returns sections with their `chunk_id`, score, content and document properties. Use `properties` first to learn the real keys and values before filtering. An empty result means nothing relevant.",
        input_schema = input_schema::<SearchArgs>()
    )]
    async fn search(&self, args: JsonObject) -> Result<CallToolResult, ErrorData> {
        let req = match parse_args::<SearchArgs>(args) {
            Ok(a) => search_request(a),
            Err(e) => return Ok(tool_error(&e)),
        };
        Ok(self.call(move |app| app.search(&req)).await)
    }

    #[tool(
        description = "Full document at a vault-relative path: properties, content (the text `edit` matches against), sections, outgoing links with their status (ok, broken, ambiguous) and backlinks.",
        input_schema = input_schema::<GetArgs>()
    )]
    async fn get(&self, args: JsonObject) -> Result<CallToolResult, ErrorData> {
        let path = match parse_args::<GetArgs>(args) {
            Ok(a) => a.path,
            Err(e) => return Ok(tool_error(&e)),
        };
        Ok(self.call(move |app| app.get_document(&path)).await)
    }

    #[tool(
        description = "Inventory of the vault's properties: per key the inferred type, the number of documents using it and its most frequent values. Call it before filtering by, or setting, a property so you reuse the existing keys and values.",
        input_schema = input_schema::<PropertiesArgs>()
    )]
    async fn properties(&self, args: JsonObject) -> Result<CallToolResult, ErrorData> {
        let prefix = match parse_args::<PropertiesArgs>(args) {
            Ok(a) => a.path_prefix,
            Err(e) => return Ok(tool_error(&e)),
        };
        Ok(self.call(move |app| app.list_properties(prefix.as_deref())).await)
    }

    #[tool(description = "Detects changes in the vault (files edited outside vlt) and reindexes them incrementally. Other tools already do this before reading.")]
    async fn reindex(&self) -> Result<CallToolResult, ErrorData> {
        Ok(self.call(|app| app.reindex_incremental()).await)
    }

    #[tool(
        description = "Replaces one exact fragment of a document's body (not its properties). `old_text` must appear exactly once: copy it from `get`'s `content` with enough surrounding text to be unique. Use it to insert links into sentences. Only call it for changes the user approved.",
        input_schema = input_schema::<EditArgs>()
    )]
    async fn edit(&self, args: JsonObject) -> Result<CallToolResult, ErrorData> {
        let a = match parse_args::<EditArgs>(args) {
            Ok(a) => a,
            Err(e) => return Ok(tool_error(&e)),
        };
        Ok(self.call(move |app| app.edit(&a.path, &a.old_text, &a.new_text)).await)
    }

    #[tool(
        description = "Adds, changes or removes properties in a document's frontmatter, keeping the rest of the file as it is. Reuse keys and value formats from `properties`. Only call it for changes the user approved.",
        input_schema = input_schema::<UpdatePropertiesArgs>()
    )]
    async fn update_properties(&self, args: JsonObject) -> Result<CallToolResult, ErrorData> {
        let (a, set) = match parse_args::<UpdatePropertiesArgs>(args).and_then(|mut a| {
            let set = set_map(a.set.take())?;
            Ok((a, set))
        }) {
            Ok(v) => v,
            Err(e) => return Ok(tool_error(&e)),
        };
        Ok(self.call(move |app| app.update_properties(&a.path, set.unwrap_or_default(), a.unset.unwrap_or_default())).await)
    }
}

#[tool_handler]
impl ServerHandler for VltServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("vlt", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn error_envelope_shape() {
        let v = error_envelope(&VltError::DocumentNotFound("a.md".into()));
        assert_eq!(v, serde_json::json!({"error": {"code": "document_not_found", "message": "document not found: a.md"}}));
    }

    fn args(v: Value) -> SearchRequest {
        search_request(serde_json::from_value(v).unwrap())
    }

    #[test]
    fn search_arguments_map_onto_the_request() {
        let req = args(json!({"query": "q", "where": {"tags": "db"}, "path_prefix": "p/", "limit": 3, "min_score": 0.5}));
        assert_eq!(req.query, "q");
        assert_eq!(req.where_, Some(json!({"tags": "db"})));
        assert_eq!(req.path_prefix.as_deref(), Some("p/"));
        assert_eq!(req.limit, Some(3));
        assert_eq!(req.min_score, Some(0.5));
        let bare = args(json!({"query": "q"}));
        assert_eq!((bare.where_, bare.path_prefix, bare.limit, bare.min_score), (None, None, None, None));
    }

    #[test]
    fn stringified_where_is_parsed_and_other_strings_are_kept() {
        assert_eq!(args(json!({"query": "q", "where": "{\"tags\": \"db\"}"})).where_, Some(json!({"tags": "db"})));
        assert_eq!(args(json!({"query": "q", "where": "not json"})).where_, Some(json!("not json")));
    }

    #[test]
    fn where_schema_is_optional_and_accepts_an_object_or_a_json_string() {
        let schema = schemars::schema_for!(SearchArgs);
        assert_eq!(schema.get("required"), Some(&json!(["query"])));
        let w = &schema.get("properties").unwrap()["where"];
        let mut types: Vec<&str> = w["anyOf"].as_array().unwrap().iter().map(|s| s["type"].as_str().unwrap()).collect();
        types.sort();
        assert_eq!(types, vec!["object", "string"], "{w}");
    }

    #[test]
    fn where_description_shows_operators_the_filter_accepts() {
        let schema = schemars::schema_for!(SearchArgs);
        let d = schema.get("properties").unwrap()["where"]["description"].as_str().unwrap();
        assert!(d.contains("gte") && !d.contains(">="), "{d}");
    }

    #[test]
    fn malformed_arguments_are_invalid_arguments() {
        let obj = |v: Value| v.as_object().unwrap().clone();
        for bad in [json!({"query": "a", "limit": -1}), json!({}), json!({"query": "a", "min_score": "x"})] {
            let err = parse_args::<SearchArgs>(obj(bad.clone())).unwrap_err();
            assert_eq!(err.code(), "invalid_arguments", "{bad}");
        }
        let ok = parse_args::<SearchArgs>(obj(json!({"query": "a", "limit": 2}))).unwrap();
        assert_eq!(ok.limit, Some(2));
    }

    #[test]
    fn update_properties_arguments_accept_an_object_or_a_json_string() {
        let a: UpdatePropertiesArgs = serde_json::from_value(json!({"path": "a.md", "set": {"k": 1}, "unset": ["x"]})).unwrap();
        assert_eq!(a.unset, Some(vec!["x".to_string()]));
        assert_eq!(set_map(a.set).unwrap().unwrap()["k"], json!(1));
        assert_eq!(set_map(Some(json!("{\"k\": 2}"))).unwrap().unwrap()["k"], json!(2));
        assert_eq!(set_map(Some(json!("nope"))).unwrap_err().code(), "invalid_arguments");
        assert_eq!(set_map(None).unwrap(), None);
    }
}
