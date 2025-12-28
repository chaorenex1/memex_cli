use std::collections::HashSet;

use crate::gatekeeper::GatekeeperConfig;

pub fn apply_overrides(
    mut cfg: GatekeeperConfig,
    overrides: &[String],
) -> Result<GatekeeperConfig, String> {
    for raw in overrides {
        let mut it = raw.splitn(2, '=');
        let key = it.next().unwrap_or("").trim();
        let val = it.next().unwrap_or("").trim();
        if key.is_empty() || val.is_empty() {
            return Err(format!("invalid override: {}", raw));
        }

        match key {
            "max_inject" => cfg.max_inject = parse_usize(key, val)?,
            "min_level_inject" => cfg.min_level_inject = parse_i32(key, val)?,
            "min_level_fallback" => cfg.min_level_fallback = parse_i32(key, val)?,
            "min_trust_show" => cfg.min_trust_show = parse_f32(key, val)?,
            "block_if_consecutive_fail_ge" => {
                cfg.block_if_consecutive_fail_ge = parse_i32(key, val)?
            }
            "skip_if_top1_score_ge" => cfg.skip_if_top1_score_ge = parse_f32(key, val)?,
            "exclude_stale_by_default" => cfg.exclude_stale_by_default = parse_bool(key, val)?,
            "active_statuses" => cfg.active_statuses = parse_statuses(val),
            _ => return Err(format!("unknown gatekeeper override: {}", key)),
        }
    }

    Ok(cfg)
}

fn parse_usize(key: &str, val: &str) -> Result<usize, String> {
    val.parse::<usize>()
        .map_err(|_| format!("invalid {}: {}", key, val))
}

fn parse_i32(key: &str, val: &str) -> Result<i32, String> {
    val.parse::<i32>()
        .map_err(|_| format!("invalid {}: {}", key, val))
}

fn parse_f32(key: &str, val: &str) -> Result<f32, String> {
    val.parse::<f32>()
        .map_err(|_| format!("invalid {}: {}", key, val))
}

fn parse_bool(key: &str, val: &str) -> Result<bool, String> {
    match val {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(format!("invalid {}: {}", key, val)),
    }
}

fn parse_statuses(val: &str) -> HashSet<String> {
    val.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}
