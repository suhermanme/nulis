//! Platform services abstracted away from editor behavior.

use std::path::PathBuf;

pub trait ApplicationDirectories: Send + Sync {
    fn configuration(&self) -> PathBuf;
    fn cache(&self) -> PathBuf;
    fn logs(&self) -> PathBuf;
}
