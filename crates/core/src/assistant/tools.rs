//! Read-only assistant tools over scan data. The assistant cannot delete,
//! move or run anything; it can only read summaries and point at existing
//! recommendations. Paths are shown with the home folder as `~`, private
//! folders appear only as totals, and file contents are never read.

use crate::model::*;
use crate::privacy;
use crate::scanner::DirIndex;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_category_totals",
            description: "Total bytes and recoverable bytes per storage category for the last scan.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "get_top_items",
            description: "Largest files and folders from the last scan.",
            input_schema: json!({"type": "object", "properties": {"limit": {"type": "integer", "default": 10}}}),
        },
        ToolDefinition {
            name: "get_recommendations",
            description: "Cleanup recommendations, optionally filtered by tier (likelySafe, reviewFirst, keep).",
            input_schema: json!({"type": "object", "properties": {"tier": {"type": "string"}}}),
        },
        ToolDefinition {
            name: "get_docker_summary",
            description: "Docker storage summary when a Docker overview has been loaded.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "get_devtools_summary",
            description: "Developer tool storage summary when devtools reports have been loaded.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
    ]
}

pub struct ToolContext<'a> {
    pub summary: Option<&'a ScanSummary>,
    pub index: Option<&'a DirIndex>,
    pub recommendations: &'a [Recommendation],
    pub docker: Option<&'a DockerOverview>,
    pub devtools: &'a [ToolReport],
    pub home: String,
}

pub fn execute_tool(ctx: &ToolContext, name: &str, input: &Value) -> Value {
    match name {
        "get_category_totals" => match ctx.summary {
            Some(s) => json!({
                "totalBytes": s.total_bytes,
                "fileCount": s.file_count,
                "categoryTotals": s.category_totals.iter().map(|(k, v)| (format!("{:?}", k), Value::from(*v))).collect::<serde_json::Map<String, Value>>(),
                "recoverableByCategory": s.recoverable_by_category.iter().map(|(k, v)| (format!("{:?}", k), Value::from(*v))).collect::<serde_json::Map<String, Value>>(),
            }),
            None => json!({"error": "no scan has been run yet"}),
        },
        "get_top_items" => match ctx.summary {
            Some(s) => {
                let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                json!({
                    "topFiles": s.top_files.iter().take(limit).map(|f| json!({
                        "path": privacy::display_path(&f.path, &ctx.home),
                        "bytes": f.bytes,
                        "category": format!("{:?}", f.category),
                    })).collect::<Vec<_>>(),
                    "heavyFolders": s.heavy_folders.iter().take(limit).map(|f| json!({
                        "path": privacy::display_path(&f.path, &ctx.home),
                        "bytes": f.bytes,
                    })).collect::<Vec<_>>(),
                })
            }
            None => json!({"error": "no scan has been run yet"}),
        },
        "get_recommendations" => {
            let tier_filter = input
                .get("tier")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let items: Vec<Value> = ctx
                .recommendations
                .iter()
                .filter(|r| match &tier_filter {
                    Some(t) => format!("{:?}", r.tier).to_lowercase().starts_with(&t.to_lowercase()[..t.len().min(4)]),
                    None => true,
                })
                .take(25)
                .map(|r| json!({
                    "id": r.id,
                    "title": r.title,
                    "bytes": r.bytes,
                    "tier": format!("{:?}", r.tier),
                    "paths": r.paths.iter().map(|p| privacy::display_path(p, &ctx.home)).collect::<Vec<_>>(),
                }))
                .collect();
            json!({"recommendations": items})
        }
        "get_docker_summary" => match ctx.docker {
            Some(d) => json!({
                "installed": d.installed,
                "running": d.running,
                "imagesBytes": d.images_total_bytes,
                "containersBytes": d.containers_total_bytes,
                "volumesBytes": d.volumes_total_bytes,
                "buildCacheBytes": d.build_cache_bytes,
            }),
            None => json!({"error": "docker overview has not been loaded"}),
        },
        "get_devtools_summary" => json!({
            "tools": ctx.devtools.iter().map(|r| json!({
                "tool": r.tool,
                "totalBytes": r.total_bytes(),
                "sections": r.sections.len(),
            })).collect::<Vec<_>>()
        }),
        _ => json!({"error": format!("unknown tool: {}", name)}),
    }
}
