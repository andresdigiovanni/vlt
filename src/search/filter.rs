use std::cmp::Ordering;

use serde_json::{json, Map, Value};

use crate::{Result, VltError};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Debug)]
enum Cond {
    Contains(Vec<Value>),
    Range(Vec<(Op, Value)>),
}

#[derive(Debug)]
pub struct Filter {
    conds: Vec<(String, Cond)>,
    path_prefix: Option<String>,
}

fn invalid<T>(msg: String) -> Result<T> {
    Err(VltError::InvalidArguments(msg))
}

fn is_scalar(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Number(_) | Value::Bool(_))
}

fn parse_cond(key: &str, v: &Value) -> Result<Cond> {
    match v {
        Value::String(_) | Value::Number(_) | Value::Bool(_) => Ok(Cond::Contains(vec![v.clone()])),
        Value::Array(items) => {
            if items.is_empty() || !items.iter().all(is_scalar) {
                return invalid(format!("where.{key}: an array must be non-empty and hold only strings, numbers or booleans"));
            }
            Ok(Cond::Contains(items.clone()))
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                return invalid(format!("where.{key}: a range object needs at least one of gt, gte, lt, lte"));
            }
            let mut bounds = Vec::new();
            for (k, bound) in obj {
                let op = match k.as_str() {
                    "gt" => Op::Gt,
                    "gte" => Op::Gte,
                    "lt" => Op::Lt,
                    "lte" => Op::Lte,
                    other => return invalid(format!("where.{key}: unknown operator `{other}` (use gt, gte, lt, lte)")),
                };
                if !is_scalar(bound) {
                    return invalid(format!("where.{key}.{k}: the bound must be a string, number or boolean"));
                }
                bounds.push((op, bound.clone()));
            }
            Ok(Cond::Range(bounds))
        }
        Value::Null => invalid(format!("where.{key}: null is not a valid filter value")),
    }
}

fn scalar_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(x), Value::String(y)) => x.to_lowercase() == y.to_lowercase(),
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Bool(x), Value::Bool(y)) => x == y,
        _ => false,
    }
}

/// `prop` is the tag `wanted` or one nested under it (`project` and `project/` match `project/vlt`).
fn tag_eq(prop: &Value, wanted: &Value) -> bool {
    match (prop, wanted) {
        (Value::String(p), Value::String(w)) => {
            let (p, w) = (p.to_lowercase(), w.trim_end_matches('/').to_lowercase());
            p.strip_prefix(&w).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        }
        _ => scalar_eq(prop, wanted),
    }
}

fn scalar_cmp(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::String(x), Value::String(y)) => Some(x.as_str().cmp(y.as_str())),
        (Value::Number(x), Value::Number(y)) => x.as_f64()?.partial_cmp(&y.as_f64()?),
        _ => None,
    }
}

impl Cond {
    fn matches(&self, key: &str, prop: &Value) -> bool {
        match self {
            Cond::Contains(wanted) => {
                let set: &[Value] = match prop {
                    Value::Array(items) => items,
                    single => std::slice::from_ref(single),
                };
                let eq = if key == "tags" { tag_eq } else { scalar_eq };
                wanted.iter().all(|w| set.iter().any(|p| eq(p, w)))
            }
            Cond::Range(bounds) => {
                if prop.is_array() {
                    return false;
                }
                bounds.iter().all(|(op, bound)| match scalar_cmp(prop, bound) {
                    Some(ord) => match op {
                        Op::Gt => ord == Ordering::Greater,
                        Op::Gte => ord != Ordering::Less,
                        Op::Lt => ord == Ordering::Less,
                        Op::Lte => ord != Ordering::Greater,
                    },
                    None => false,
                })
            }
        }
    }
}

impl Filter {
    pub fn new(where_: Option<&Value>, path_prefix: Option<&str>) -> Result<Filter> {
        let mut conds = Vec::new();
        if let Some(w) = where_ {
            let Some(obj) = w.as_object() else {
                return invalid("where must be an object".to_string());
            };
            for (key, v) in obj {
                conds.push((key.clone(), parse_cond(key, v)?));
            }
        }
        Ok(Filter { conds, path_prefix: path_prefix.map(str::to_string) })
    }

    pub fn is_empty(&self) -> bool {
        self.conds.is_empty() && self.path_prefix.is_none()
    }

    pub fn matches(&self, path: &str, properties: Option<&Map<String, Value>>) -> bool {
        if let Some(prefix) = &self.path_prefix {
            if !path.starts_with(prefix.as_str()) {
                return false;
            }
        }
        self.conds.iter().all(|(key, cond)| properties.and_then(|p| p.get(key)).is_some_and(|v| cond.matches(key, v)))
    }
}

fn parse_value(raw: &str) -> Value {
    match raw {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => match raw.parse::<f64>() {
            Ok(n) if n.is_finite() => json!(n),
            _ => Value::String(raw.to_string()),
        },
    }
}

pub fn parse_cli_where(exprs: &[String]) -> Result<Value> {
    let mut out: Map<String, Value> = Map::new();
    for expr in exprs {
        let Some(pos) = expr.find(['<', '>', '=']) else {
            return invalid(format!("invalid --where `{expr}`: expected <key><op><value> with op one of =, >, >=, <, <="));
        };
        let key = &expr[..pos];
        let rest = &expr[pos..];
        let (op, value) = if let Some(v) = rest.strip_prefix(">=") {
            ("gte", v)
        } else if let Some(v) = rest.strip_prefix("<=") {
            ("lte", v)
        } else if let Some(v) = rest.strip_prefix('=') {
            ("=", v)
        } else if let Some(v) = rest.strip_prefix('>') {
            ("gt", v)
        } else {
            ("lt", &rest[1..])
        };
        if key.is_empty() || value.is_empty() {
            return invalid(format!("invalid --where `{expr}`: key and value must not be empty"));
        }
        let value = parse_value(value);
        let conflict = || invalid(format!("invalid --where `{expr}`: `{key}` mixes = with range operators or repeats a range operator"));
        if op == "=" {
            match out.remove(key) {
                None => {
                    out.insert(key.to_string(), value);
                }
                Some(Value::Object(_)) => return conflict(),
                Some(Value::Array(mut items)) => {
                    items.push(value);
                    out.insert(key.to_string(), Value::Array(items));
                }
                Some(prev) => {
                    out.insert(key.to_string(), Value::Array(vec![prev, value]));
                }
            }
        } else {
            match out.get_mut(key) {
                None => {
                    out.insert(key.to_string(), json!({ op: value }));
                }
                Some(Value::Object(o)) if !o.contains_key(op) => {
                    o.insert(op.to_string(), value);
                }
                Some(_) => return conflict(),
            }
        }
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map, Value};

    fn props(v: Value) -> Map<String, Value> { v.as_object().unwrap().clone() }
    fn m(w: Value, p: Value) -> bool { Filter::new(Some(&w), None).unwrap().matches("x.md", Some(&props(p))) }

    #[test]
    fn scalar_equality_is_case_insensitive() {
        assert!(m(json!({"status": "In_Progress"}), json!({"status": "in_progress"})));
        assert!(!m(json!({"status": "done"}), json!({"status": "in_progress"})));
        assert!(!m(json!({"status": "done"}), json!({})));
        assert!(m(json!({"n": 2}), json!({"n": 2.0})));
        assert!(!m(json!({"n": 2}), json!({"n": "2"})));
        assert!(m(json!({"draft": true}), json!({"draft": true})));
    }

    #[test]
    fn list_containment() {
        let p = json!({"tags": ["rust", "MCP", "cli"]});
        assert!(m(json!({"tags": "rust"}), p.clone()));
        assert!(m(json!({"tags": ["rust", "mcp"]}), p.clone()));
        assert!(!m(json!({"tags": ["rust", "go"]}), p.clone()));
        assert!(m(json!({"tags": "rust"}), json!({"tags": "rust"})), "scalar property matches scalar filter");
        assert!(m(json!({"tags": ["rust"]}), json!({"tags": "rust"})));
    }

    #[test]
    fn a_tag_matches_its_nested_tags() {
        let p = json!({"tags": ["Project/vlt/cli", "area"]});
        assert!(m(json!({"tags": "project"}), p.clone()));
        assert!(m(json!({"tags": "project/vlt"}), p.clone()));
        assert!(m(json!({"tags": "project/vlt/cli"}), p.clone()));
        assert!(!m(json!({"tags": "proj"}), p.clone()), "a prefix only counts up to a `/`");
        assert!(!m(json!({"tags": "project/vlt/cli/x"}), p.clone()));
        assert!(!m(json!({"tags": "area/x"}), p));
        assert!(m(json!({"tags": "project"}), json!({"tags": "project/vlt"})), "scalar tags property");
    }

    #[test]
    fn a_trailing_slash_on_a_tag_filter_is_ignored() {
        assert!(m(json!({"tags": "project/"}), json!({"tags": ["project/vlt"]})));
        assert!(m(json!({"tags": "project/"}), json!({"tags": ["project"]})));
        assert!(!m(json!({"tags": "proj/"}), json!({"tags": ["project/vlt"]})));
    }

    #[test]
    fn only_tags_match_nested_values() {
        assert!(!m(json!({"topics": "project"}), json!({"topics": ["project/vlt"]})));
    }

    #[test]
    fn ranges_on_numbers_and_iso_dates() {
        let w = json!({"date": {"gte": "2026-01-01", "lt": "2026-07-01"}});
        assert!(m(w.clone(), json!({"date": "2026-03-04"})));
        assert!(m(w.clone(), json!({"date": "2026-01-01T10:00"})));
        assert!(!m(w.clone(), json!({"date": "2026-07-01"})));
        assert!(!m(w, json!({"date": 5})));
        assert!(m(json!({"n": {"gt": 9}}), json!({"n": 10})));
        assert!(!m(json!({"n": {"gt": 9}}), json!({"n": "10"})));
        assert!(!m(json!({"tags": {"gt": "a"}}), json!({"tags": ["b"]})));
    }

    #[test]
    fn keys_combine_with_and_and_path_prefix_applies() {
        let f = Filter::new(Some(&json!({"a": 1, "b": "x"})), Some("projects/")).unwrap();
        assert!(f.matches("projects/p.md", Some(&props(json!({"a": 1, "b": "X"})))));
        assert!(!f.matches("projects/p.md", Some(&props(json!({"a": 1})))));
        assert!(!f.matches("notes/p.md", Some(&props(json!({"a": 1, "b": "x"})))));
        assert!(!f.matches("projects/p.md", None));
        let only_prefix = Filter::new(None, Some("projects/")).unwrap();
        assert!(only_prefix.matches("projects/q.md", None));
        assert!(Filter::new(None, None).unwrap().is_empty());
        assert!(!only_prefix.is_empty());
    }

    #[test]
    fn invalid_where_shapes() {
        for w in [json!([1]), json!({"a": null}), json!({"a": [[1]]}), json!({"a": {"eq": 1}}), json!({"a": {}}), json!({"a": []}), json!({"a": {"gt": [1]}})] {
            assert!(matches!(Filter::new(Some(&w), None), Err(VltError::InvalidArguments(_))), "{w}");
        }
    }

    #[test]
    fn cli_where_syntax() {
        let e = |v: &[&str]| parse_cli_where(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(e(&["status=in_progress"]).unwrap(), json!({"status": "in_progress"}));
        assert_eq!(e(&["tags=rust", "tags=mcp"]).unwrap(), json!({"tags": ["rust", "mcp"]}));
        assert_eq!(e(&["date>=2026-01-01", "date<2026-07-01"]).unwrap(), json!({"date": {"gte": "2026-01-01", "lt": "2026-07-01"}}));
        assert_eq!(e(&["n>3", "draft=false"]).unwrap(), json!({"n": {"gt": 3.0}, "draft": false}));
        assert_eq!(e(&["t=a=b"]).unwrap(), json!({"t": "a=b"}));
        for bad in [&["noop"][..], &["=x"], &["k="], &["k=1", "k>0"]] {
            assert!(matches!(e(bad), Err(VltError::InvalidArguments(_))), "{bad:?}");
        }
    }

    #[test]
    fn range_bounds_are_exact_at_the_boundary() {
        assert!(!m(json!({"n": {"gt": 5}}), json!({"n": 5})));
        assert!(m(json!({"n": {"gte": 5}}), json!({"n": 5})));
        assert!(!m(json!({"n": {"gte": 5}}), json!({"n": 4})));
        assert!(m(json!({"n": {"lte": 5}}), json!({"n": 5})));
        assert!(!m(json!({"n": {"lte": 5}}), json!({"n": 6})));
        assert!(!m(json!({"n": {"lt": 5}}), json!({"n": 5})));
        assert!(m(json!({"n": {"gt": 5}}), json!({"n": 6})));
        assert!(m(json!({"n": {"lt": 5}}), json!({"n": 4})));
    }

    #[test]
    fn booleans_never_range() {
        assert!(!m(json!({"d": {"gte": true}}), json!({"d": true})));
        assert!(!m(json!({"d": {"lt": true}}), json!({"d": false})));
    }

    #[test]
    fn cli_where_edge_cases() {
        let e = |v: &[&str]| parse_cli_where(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(e(&["t=a", "t=b", "t=c"]).unwrap(), json!({"t": ["a", "b", "c"]}));
        assert_eq!(e(&["n<=2", "n>=1"]).unwrap(), json!({"n": {"lte": 2.0, "gte": 1.0}}));
        assert_eq!(e(&["k=inf", "j=NaN"]).unwrap(), json!({"k": "inf", "j": "NaN"}));
        assert_eq!(e(&["k=>b"]).unwrap(), json!({"k": ">b"}));
        for bad in [&["k>0", "k=1"][..], &["k=1", "k=2", "k>0"], &["k>1", "k>2"], &["k>=1", "k>=2"]] {
            assert!(matches!(e(bad), Err(VltError::InvalidArguments(_))), "{bad:?}");
        }
    }
}
