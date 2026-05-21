/// MCP tool schema generator.
///
/// Converts a `.briefskill` `SkillInterface` into MCP `tools` definitions
/// (one tool per function). The output is a `serde_json::Value` array that
/// can be returned from a `tools/list` response.
///
/// JSON Schema mappings:
/// - `Int`    → `{ "type": "integer" }`
/// - `Float`  → `{ "type": "number" }`
/// - `Bool`   → `{ "type": "boolean" }`
/// - `String` → `{ "type": "string" }`
/// - other    → `{ "type": "string" }` (fallback — documented as opaque)
///
/// Static constraint overlays:
/// - `@range(min, max)` → `"minimum"`, `"maximum"`
/// - `@enum(vals)`      → `"enum"` array
/// - `@matches(regex)`  → `"pattern"`
/// - `@nonEmpty`        → `"minLength": 1`

use serde_json::{json, Value};

use crate::skillgen::{SkillInterface, SkillParam, StaticConstraint};

// ─────────────────────────────────────────────────────────────────────────────

/// A single MCP tool definition.
#[derive(Debug, Clone)]
pub struct McpTool {
    /// Tool name in MCP format: `"SkillName.functionName"`.
    pub name: String,
    /// Human-readable description derived from function name and return type.
    pub description: String,
    /// JSON Schema for the tool's input arguments.
    pub input_schema: Value,
}

/// Convert a `SkillInterface` into a list of MCP tool definitions, one per function.
pub fn interface_to_mcp_tools(skill_name: &str, iface: &SkillInterface) -> Vec<McpTool> {
    iface.funcs.iter().map(|f| {
        let tool_name = format!("{skill_name}.{}", f.name);
        let description = format!("{skill_name}.{} → {}", f.name, f.return_type);
        let input_schema = params_to_json_schema(&f.params);

        McpTool { name: tool_name, description, input_schema }
    }).collect()
}

/// Serialize a list of `McpTool`s into the JSON `tools` array for a `tools/list` response.
pub fn tools_to_json(tools: &[McpTool]) -> Value {
    Value::Array(tools.iter().map(|t| json!({
        "name":        t.name,
        "description": t.description,
        "inputSchema": t.input_schema,
    })).collect())
}

// ─────────────────────────────────────────────────────────────────────────────

fn params_to_json_schema(params: &[SkillParam]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();

    for param in params {
        let schema = param_to_json_schema(param);
        properties.insert(param.name.clone(), schema);
        required.push(Value::String(param.name.clone()));
    }

    json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": Value::Array(required),
        "additionalProperties": false,
    })
}

fn param_to_json_schema(param: &SkillParam) -> Value {
    let mut schema = base_type_schema(&param.base_type);

    // Overlay static constraints.
    for constraint in &param.static_constraints {
        apply_constraint(&mut schema, constraint);
    }

    // Document dynamic attrs as a description annotation if present.
    if !param.dynamic_attrs.is_empty() {
        let attrs = param.dynamic_attrs.join(", ");
        schema["x-brief-annotations"] = Value::String(attrs);
    }

    schema
}

fn base_type_schema(base: &str) -> Value {
    match base {
        "Int" | "Integer"     => json!({ "type": "integer" }),
        "Float" | "Double"    => json!({ "type": "number" }),
        "Bool" | "Boolean"    => json!({ "type": "boolean" }),
        "String" | "Text"     => json!({ "type": "string" }),
        _                     => json!({ "type": "string", "x-brief-type": base }),
    }
}

fn apply_constraint(schema: &mut Value, constraint: &StaticConstraint) {
    match constraint {
        StaticConstraint::Range(min, max) => {
            schema["minimum"] = json!(min);
            schema["maximum"] = json!(max);
        }
        StaticConstraint::Enum(vals) => {
            schema["enum"] = Value::Array(vals.iter().map(|v| Value::String(v.clone())).collect());
        }
        StaticConstraint::Matches(pattern) => {
            schema["pattern"] = Value::String(pattern.clone());
        }
        StaticConstraint::NonEmpty => {
            if schema.get("type").and_then(|t| t.as_str()) == Some("string") {
                schema["minLength"] = json!(1);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skillgen::{SkillFn, SkillInterface, SkillParam, StaticConstraint};

    fn make_interface() -> SkillInterface {
        SkillInterface {
            name: "Mapper".into(),
            effects: vec![],
            funcs: vec![
                SkillFn {
                    name: "mapValue".into(),
                    arg_count: 1,
                    return_type: "Code".into(),
                    params: vec![SkillParam {
                        name: "input".into(),
                        base_type: "Int".into(),
                        static_constraints: vec![StaticConstraint::Range(0, 100)],
                        dynamic_attrs: vec![],
                    }],
                },
                SkillFn {
                    name: "classify".into(),
                    arg_count: 1,
                    return_type: "Status".into(),
                    params: vec![SkillParam {
                        name: "cat".into(),
                        base_type: "String".into(),
                        static_constraints: vec![StaticConstraint::Enum(vec![
                            "ok".into(), "warn".into(), "err".into(),
                        ])],
                        dynamic_attrs: vec![],
                    }],
                },
            ],
        }
    }

    #[test]
    fn generates_one_tool_per_function() {
        let iface = make_interface();
        let tools = interface_to_mcp_tools("Mapper", &iface);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "Mapper.mapValue");
        assert_eq!(tools[1].name, "Mapper.classify");
    }

    #[test]
    fn range_constraint_produces_min_max() {
        let iface = make_interface();
        let tools = interface_to_mcp_tools("Mapper", &iface);
        let schema = &tools[0].input_schema;
        let input_schema = &schema["properties"]["input"];
        assert_eq!(input_schema["type"], "integer");
        assert_eq!(input_schema["minimum"], 0);
        assert_eq!(input_schema["maximum"], 100);
    }

    #[test]
    fn enum_constraint_produces_enum_array() {
        let iface = make_interface();
        let tools = interface_to_mcp_tools("Mapper", &iface);
        let schema = &tools[1].input_schema;
        let cat_schema = &schema["properties"]["cat"];
        assert_eq!(cat_schema["enum"], json!(["ok", "warn", "err"]));
    }

    #[test]
    fn tools_to_json_produces_valid_mcp_structure() {
        let iface = make_interface();
        let tools = interface_to_mcp_tools("Mapper", &iface);
        let json_tools = tools_to_json(&tools);
        let arr = json_tools.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(arr[0]["name"].as_str().is_some());
        assert!(arr[0]["inputSchema"]["type"] == "object");
        assert!(arr[0]["inputSchema"]["properties"].is_object());
    }
}
