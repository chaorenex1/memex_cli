//! Memory service CLI commands implementation
use crate::commands::cli::{
    RecordCandidateArgs, RecordHitArgs, RecordSessionArgs, RecordValidationArgs, SearchArgs,
};
use memex_core::api as core_api;
use serde_json::json;

/// Handle search command
pub async fn handle_search(
    args: SearchArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Get project_id
    let project_id = args.project_id.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| crate::utils::project_id::generate_project_id(&p))
            .unwrap_or_else(|_| "default".to_string())
    });

    // Build services
    let services = ctx
        .build_services(cfg)
        .map_err(core_api::CliError::Runner)?;

    // Get memory plugin
    let memory = services
        .memory
        .as_ref()
        .ok_or_else(|| core_api::CliError::Command("Memory service not configured".to_string()))?;

    // Create search payload
    let payload = core_api::QASearchPayload {
        project_id,
        query: args.query.clone(),
        limit: args.limit,
        min_score: args.min_score,
    };

    // Execute search
    let matches = memory
        .search(payload)
        .await
        .map_err(|e| core_api::CliError::Command(format!("Search failed: {}", e)))?;

    // Format output
    match args.format.as_str() {
        "json" => {
            let output = json!({
                "matches": matches,
                "query": args.query,
                "count": matches.len()
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        "markdown" => {
            if matches.is_empty() {
                println!("No matches found for query: {}", args.query);
            } else {
                println!("### 📚 Search Results for: {}\n", args.query);
                for m in matches {
                    println!("**[{}]** Q: {}", m.qa_id, m.question);
                    println!("A: {}", m.answer);
                    println!("_Score: {:.2}_\n---\n", m.score);
                }
            }
        }
        _ => {
            return Err(core_api::CliError::Command(format!(
                "Unknown format: {}",
                args.format
            )));
        }
    }

    Ok(())
}

/// Handle record-candidate command
pub async fn handle_record_candidate(
    args: RecordCandidateArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Get project_id
    let project_id = args.project_id.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| crate::utils::project_id::generate_project_id(&p))
            .unwrap_or_else(|_| "default".to_string())
    });

    // Build services
    let services = ctx
        .build_services(cfg)
        .map_err(core_api::CliError::Runner)?;

    // Get memory plugin
    let memory = services
        .memory
        .as_ref()
        .ok_or_else(|| core_api::CliError::Command("Memory service not configured".to_string()))?;

    // Parse tags
    let tags: Vec<String> = args
        .tags
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    // Parse metadata
    let mut metadata = if let Some(meta_str) = args.metadata {
        serde_json::from_str(&meta_str)
            .map_err(|e| core_api::CliError::Command(format!("Invalid metadata JSON: {}", e)))?
    } else {
        json!({})
    };

    // Add files to metadata if provided
    if let Some(files_str) = args.files {
        let files: Vec<String> = files_str.split(',').map(|s| s.trim().to_string()).collect();
        metadata["files"] = json!(files);
    }

    // Create candidate payload
    let payload = core_api::QACandidatePayload {
        project_id,
        question: args.query.clone(),
        answer: args.answer.clone(),
        tags,
        confidence: 0.8,
        metadata,
        summary: None,
        source: None,
        author: None,
    };

    // Record candidate
    memory
        .record_candidate(payload)
        .await
        .map_err(|e| core_api::CliError::Command(format!("Record candidate failed: {}", e)))?;

    // Output success
    let output = json!({
        "success": true,
        "message": "Candidate recorded successfully"
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    Ok(())
}

/// Handle record-hit command
pub async fn handle_record_hit(
    args: RecordHitArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Get project_id
    let project_id = args.project_id.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| crate::utils::project_id::generate_project_id(&p))
            .unwrap_or_else(|_| "default".to_string())
    });

    // Build services
    let services = ctx
        .build_services(cfg)
        .map_err(core_api::CliError::Runner)?;

    // Get memory plugin
    let memory = services
        .memory
        .as_ref()
        .ok_or_else(|| core_api::CliError::Command("Memory service not configured".to_string()))?;

    // Parse QA IDs
    let used_ids: Vec<String> = args
        .qa_ids
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let shown_ids: Vec<String> = args
        .shown
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(|| used_ids.clone());

    // Build references
    let mut references = Vec::new();
    for id in &shown_ids {
        references.push(core_api::QAReferencePayload {
            qa_id: id.clone(),
            shown: Some(true),
            used: Some(used_ids.contains(id)),
            message_id: None,
            context: None,
        });
    }

    // Create hit payload
    let payload = core_api::QAHitsPayload {
        project_id,
        references,
    };

    // Record hit
    memory
        .record_hit(payload)
        .await
        .map_err(|e| core_api::CliError::Command(format!("Record hit failed: {}", e)))?;

    // Output success
    let output = json!({
        "success": true,
        "recorded_count": used_ids.len()
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    Ok(())
}

/// Handle record-validation command
pub async fn handle_record_validation(
    args: RecordValidationArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Get project_id
    let project_id = args.project_id.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| crate::utils::project_id::generate_project_id(&p))
            .unwrap_or_else(|_| "default".to_string())
    });

    // Build services
    let services = ctx
        .build_services(cfg)
        .map_err(core_api::CliError::Runner)?;

    // Get memory plugin
    let memory = services
        .memory
        .as_ref()
        .ok_or_else(|| core_api::CliError::Command("Memory service not configured".to_string()))?;

    // Create validation payload
    let payload = core_api::QAValidationPayload {
        project_id,
        qa_id: args.qa_id.clone(),
        result: None,
        signal_strength: None,
        success: Some(args.success),
        strong_signal: Some(args.success && args.confidence >= 0.8),
        source: Some("claude-code".to_string()),
        context: Some(format!("confidence:{}", args.confidence)),
        client: None,
        ts: Some(chrono::Local::now().to_rfc3339()),
        payload: None,
    };

    // Record validation
    memory
        .record_validation(payload)
        .await
        .map_err(|e| core_api::CliError::Command(format!("Record validation failed: {}", e)))?;

    // Output success
    let output = json!({
        "success": true,
        "message": "Validation recorded successfully",
        "qa_id": args.qa_id,
        "validation_success": args.success,
        "confidence": args.confidence
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    Ok(())
}

/// Handle record-session command
pub async fn handle_record_session(
    args: RecordSessionArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Get project_id
    let project_id = args.project_id.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| crate::utils::project_id::generate_project_id(&p))
            .unwrap_or_else(|_| "default".to_string())
    });

    // Read transcript file
    let transcript_content = std::fs::read_to_string(&args.transcript)
        .map_err(|e| core_api::CliError::Command(format!("Failed to read transcript: {}", e)))?;

    // Parse JSONL
    let mut tool_events: Vec<core_api::ToolEvent> = Vec::new();
    for line in transcript_content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Try to parse as tool event
        if let Ok(event) = serde_json::from_str::<core_api::ToolEvent>(line) {
            tool_events.push(event);
        }
    }

    // Extract candidates from transcript
    // This is a simplified version - in production, you'd want more sophisticated extraction
    let candidates = extract_candidates_from_transcript(&tool_events, &args.session_id);

    if args.extract_only {
        // Just output extracted candidates
        let candidates_json: Vec<serde_json::Value> =
            candidates.iter().map(|c| c.to_json()).collect();
        let output = json!({
            "success": true,
            "extracted_count": candidates.len(),
            "candidates": candidates_json
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return Ok(());
    }

    // Build services
    let services = ctx
        .build_services(cfg)
        .map_err(core_api::CliError::Runner)?;

    // Get memory plugin
    let memory = services
        .memory
        .as_ref()
        .ok_or_else(|| core_api::CliError::Command("Memory service not configured".to_string()))?;

    // Record each candidate
    let mut recorded_count = 0;
    for candidate in &candidates {
        let payload = core_api::QACandidatePayload {
            project_id: project_id.clone(),
            question: candidate.question.clone(),
            answer: candidate.answer.clone(),
            tags: candidate.tags.clone(),
            confidence: candidate.confidence,
            metadata: json!({
                "session_id": args.session_id
            }),
            summary: None,
            source: None,
            author: None,
        };

        if memory.record_candidate(payload).await.is_ok() {
            recorded_count += 1;
        }
    }

    // Output success
    let candidates_json: Vec<serde_json::Value> = candidates.iter().map(|c| c.to_json()).collect();
    let output = json!({
        "success": true,
        "extracted_count": candidates.len(),
        "recorded_count": recorded_count,
        "candidates": candidates_json
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    Ok(())
}

#[derive(Debug)]
struct CandidateExtract {
    question: String,
    answer: String,
    tags: Vec<String>,
    confidence: f32,
}

impl CandidateExtract {
    fn to_json(&self) -> serde_json::Value {
        json!({
            "question": self.question,
            "answer": self.answer,
            "tags": self.tags,
            "confidence": self.confidence
        })
    }
}

/// Extract candidates from transcript
/// This is a simplified implementation - in production, you'd want more sophisticated logic
fn extract_candidates_from_transcript(
    tool_events: &[core_api::ToolEvent],
    session_id: &str,
) -> Vec<CandidateExtract> {
    let mut candidates = Vec::new();

    // Look for Write/Edit events that create or modify significant files
    for event in tool_events {
        let tool_name = event.tool.as_deref().unwrap_or("");
        if tool_name == "Write" || tool_name == "Edit" {
            if let Some(file_path) = event.args.get("file_path").and_then(|v| v.as_str()) {
                // Skip small or config files
                if file_path.ends_with(".json")
                    || file_path.ends_with(".toml")
                    || file_path.ends_with(".yml")
                    || file_path.ends_with(".yaml")
                {
                    continue;
                }

                let content = event
                    .args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Only record if content is substantial
                if content.len() > 100 {
                    let question = format!("How to implement {}?", file_path);
                    let answer = format!(
                        "Created {} with implementation (session: {})",
                        file_path, session_id
                    );

                    // Extract tags from file extension
                    let ext = std::path::Path::new(file_path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("unknown");

                    candidates.push(CandidateExtract {
                        question,
                        answer,
                        tags: vec![
                            format!("tool:{}", tool_name.to_lowercase()),
                            format!("lang:{}", ext),
                        ],
                        confidence: 0.7,
                    });
                }
            }
        }
    }

    candidates
}
