//! Tests for binary content functionality
#![cfg(test)]

use expectorate::assert_contents;
use std::fs;
use tempfile::TempDir;

/// Test that binary content can be used with assert_contents
#[test]
fn binary_content_works() {
    let dir = TempDir::with_prefix("expectorate-").unwrap();
    let path = dir.path().join("binary-file.bin");

    // Create binary content with some non-UTF8 bytes
    let binary_data: &[u8] = &[0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD];

    // Write initial content
    fs::write(&path, binary_data).unwrap();

    // Test that same binary content matches (this should not panic)
    assert_contents(&path, binary_data);

    // Verify file still contains the same data
    let read_back = fs::read(&path).unwrap();
    assert_eq!(read_back, binary_data);
}

/// Test that binary content comparison shows proper diff when different
#[test]
#[should_panic(expected = "assertion failed")]
fn binary_content_diff() {
    let dir = TempDir::with_prefix("expectorate-").unwrap();
    let path = dir.path().join("binary-file.bin");

    let original_data: &[u8] = &[0x00, 0x01, 0x02, 0xFF];
    let different_data: &[u8] = &[0x00, 0x01, 0x03, 0xFF]; // One byte different

    // Write original content
    fs::write(&path, original_data).unwrap();

    // Test that different binary content fails comparison (should panic)
    assert_contents(&path, different_data);
}
