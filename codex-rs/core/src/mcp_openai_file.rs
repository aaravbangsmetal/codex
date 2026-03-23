use serde_json::Map;
use serde_json::Value as JsonValue;

use crate::mcp_connection_manager::ToolInfo;
use crate::openai_files::META_OPENAI_FILE_PARAMS;

pub(crate) fn declared_openai_file_params(meta: Option<&Map<String, JsonValue>>) -> Vec<String> {
    declared_top_level_fields(meta, META_OPENAI_FILE_PARAMS)
}

pub(crate) fn mask_input_schema_for_model(input_schema: &mut JsonValue, file_params: &[String]) {
    let Some(properties) = input_schema
        .as_object_mut()
        .and_then(|schema| schema.get_mut("properties"))
        .and_then(JsonValue::as_object_mut)
    else {
        return;
    };

    for field_name in file_params {
        let Some(property_schema) = properties.get_mut(field_name) else {
            continue;
        };
        mask_input_property_schema(property_schema);
    }
}

pub(crate) fn retain_openai_file_tool_meta(mut tool_info: ToolInfo) -> ToolInfo {
    tool_info.tool.meta =
        filtered_openai_file_tool_meta(tool_info.tool.meta.as_deref()).map(rmcp::model::Meta);
    tool_info
}

pub(crate) fn retain_openai_file_tool_meta_map(
    tools: Option<std::collections::HashMap<String, ToolInfo>>,
) -> Option<std::collections::HashMap<String, ToolInfo>> {
    tools.map(|tools| {
        tools
            .into_iter()
            .map(|(name, tool_info)| (name, retain_openai_file_tool_meta(tool_info)))
            .collect()
    })
}

fn filtered_openai_file_tool_meta(
    meta: Option<&Map<String, JsonValue>>,
) -> Option<Map<String, JsonValue>> {
    let value = meta?.get(META_OPENAI_FILE_PARAMS)?.clone();

    Some(Map::from_iter([(
        META_OPENAI_FILE_PARAMS.to_string(),
        value,
    )]))
}

fn declared_top_level_fields(meta: Option<&Map<String, JsonValue>>, key: &str) -> Vec<String> {
    let Some(meta) = meta else {
        return Vec::new();
    };

    meta.get(key)
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(JsonValue::as_str)
        .filter(|value| is_top_level_field_name(value))
        .map(str::to_string)
        .collect()
}

fn is_top_level_field_name(field_name: &str) -> bool {
    !field_name.is_empty()
        && !field_name.contains('.')
        && !field_name.contains('/')
        && !field_name.contains('[')
        && !field_name.contains(']')
}

fn mask_input_property_schema(schema: &mut JsonValue) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };

    let mut description = object
        .get("description")
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    let guidance = "This parameter expects an absolute local file path. If you want to upload a file, provide the absolute path to that file here.";
    if description.is_empty() {
        description = guidance.to_string();
    } else if !description.contains(guidance) {
        description = format!("{description} {guidance}");
    }

    let is_array = object.get("type").and_then(JsonValue::as_str) == Some("array")
        || object.get("items").is_some();
    object.clear();
    object.insert("description".to_string(), JsonValue::String(description));
    if is_array {
        object.insert("type".to_string(), JsonValue::String("array".to_string()));
        object.insert("items".to_string(), serde_json::json!({ "type": "string" }));
    } else {
        object.insert("type".to_string(), JsonValue::String("string".to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn declared_openai_file_fields_ignore_nested_names() {
        let meta = serde_json::json!({
            "openai/fileParams": ["file", "nested.value", "files[0]", "attachments"],
            "openai/fileOutputs": ["output", "artifacts/0"]
        });
        let meta = meta.as_object().expect("meta object");

        assert_eq!(
            declared_openai_file_params(Some(meta)),
            vec!["file".to_string(), "attachments".to_string()]
        );
    }

    #[test]
    fn mask_input_schema_for_model_rewrites_scalar_and_array_fields() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "object",
                    "description": "Original file payload."
                },
                "files": {
                    "type": "array",
                    "items": {"type": "object"}
                }
            }
        });

        mask_input_schema_for_model(&mut schema, &["file".to_string(), "files".to_string()]);

        assert_eq!(
            schema,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "Original file payload. This parameter expects an absolute local file path. If you want to upload a file, provide the absolute path to that file here."
                    },
                    "files": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "This parameter expects an absolute local file path. If you want to upload a file, provide the absolute path to that file here."
                    }
                }
            })
        );
    }

    #[test]
    fn retain_openai_file_tool_meta_drops_unrelated_meta_entries() {
        let tool_info = ToolInfo {
            server_name: "codex_apps".to_string(),
            tool_name: "tool".to_string(),
            tool_namespace: "ns".to_string(),
            tool: rmcp::model::Tool {
                name: "tool".to_string().into(),
                title: None,
                description: None,
                input_schema: std::sync::Arc::new(rmcp::model::JsonObject::default()),
                output_schema: None,
                annotations: None,
                execution: None,
                icons: None,
                meta: Some(rmcp::model::Meta(
                    serde_json::json!({
                        "openai/fileParams": ["file"],
                        "openai/fileOutputs": ["outputFile"],
                        "_codex_apps": {"connector_id": "calendar"},
                        "other": true
                    })
                    .as_object()
                    .expect("meta object")
                    .clone(),
                )),
            },
            supports_openai_file_bridge_capability: true,
            connector_id: None,
            connector_name: None,
            plugin_display_names: Vec::new(),
            connector_description: None,
        };

        let retained = retain_openai_file_tool_meta(tool_info);

        assert_eq!(
            retained.tool.meta,
            Some(rmcp::model::Meta(
                serde_json::json!({
                    "openai/fileParams": ["file"]
                })
                .as_object()
                .expect("meta object")
                .clone()
            ))
        );
    }
}
