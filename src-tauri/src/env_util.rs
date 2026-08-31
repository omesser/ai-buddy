//! Environment variable utilities.

/// Check if an environment variable is on (1), off (0), or unset (off).
pub fn env_flag_is_on(name: &str) -> bool {
    std::env::var(name).ok().as_deref() == Some("1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_variable_is_off() {
        std::env::remove_var("AI_BUDDY_TEST_FLAG_UNSET");
        assert!(!env_flag_is_on("AI_BUDDY_TEST_FLAG_UNSET"));
    }

    #[test]
    fn zero_is_off() {
        std::env::set_var("AI_BUDDY_TEST_FLAG_ZERO", "0");
        assert!(!env_flag_is_on("AI_BUDDY_TEST_FLAG_ZERO"));
        std::env::remove_var("AI_BUDDY_TEST_FLAG_ZERO");
    }

    #[test]
    fn one_is_on() {
        std::env::set_var("AI_BUDDY_TEST_FLAG_ONE", "1");
        assert!(env_flag_is_on("AI_BUDDY_TEST_FLAG_ONE"));
        std::env::remove_var("AI_BUDDY_TEST_FLAG_ONE");
    }
}
