pub mod outline;
pub mod retrieval;

use crate::env::{OUTLINE_API_KEY, OUTLINE_URL};

pub fn is_enabled() -> bool {
    !OUTLINE_URL.is_empty() && !OUTLINE_API_KEY.is_empty()
}
