//! Project links shown on the login screen and About window. Empty = not
//! configured yet: the button is shown disabled and nothing is fetched.
//! Values are driven by `crate::brand` (`brand.local.json`, falling back to
//! upstream Aurora Viewer defaults).

pub use crate::brand::{DISCORD, GITHUB, NEW_ISSUE, NEWS_FEED, PRIVACY_POLICY, RAW_FILES, WEBSITE, WIKI};
