//! Captured upstream responses. They live in the workspace's `tests/fixtures`
//! because the binary's route tests replay the same responses.

/// Reads the fixture at `name`, relative to `tests/fixtures`.
pub(crate) fn read(name: &str) -> Vec<u8> {
    let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(path).expect("fixture should exist")
}
