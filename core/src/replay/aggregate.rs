use super::model::ReplayRun;
use super::parse::parse_events_file;

pub fn replay_events_file(
    path: &str,
    run_id_filter: Option<&str>,
) -> Result<Vec<ReplayRun>, String> {
    parse_events_file(path, run_id_filter)
}

pub fn aggregate_runs(runs: Vec<ReplayRun>) -> Vec<ReplayRun> {
    runs
}
