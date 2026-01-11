use crate::error::stdio::ErrorCode;

pub const DEFAULT_TIMEOUT_SECS: u64 = 300;
pub const MAX_TIMEOUT_SECS: u64 = 60 * 60;

pub fn effective_timeout_secs(timeout: Option<u64>) -> u64 {
    let v = timeout.unwrap_or(DEFAULT_TIMEOUT_SECS);
    v.clamp(1, MAX_TIMEOUT_SECS)
}

pub fn max_attempts(retry: Option<u32>) -> u32 {
    retry.unwrap_or(0).saturating_add(1).max(1)
}

pub fn exit_code_for_timeout() -> i32 {
    ErrorCode::Timeout.as_u16() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_default_and_clamp() {
        assert_eq!(effective_timeout_secs(None), DEFAULT_TIMEOUT_SECS);
        assert_eq!(effective_timeout_secs(Some(0)), 1);
        assert_eq!(
            effective_timeout_secs(Some(MAX_TIMEOUT_SECS + 10)),
            MAX_TIMEOUT_SECS
        );
    }

    #[test]
    fn attempts_default_and_retry() {
        assert_eq!(max_attempts(None), 1);
        assert_eq!(max_attempts(Some(0)), 1);
        assert_eq!(max_attempts(Some(2)), 3);
    }
}
