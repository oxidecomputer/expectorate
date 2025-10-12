// Test demonstrating binary content usage with expectorate

use expectorate::assert_contents;
use std::fs;
use tempfile::TempDir;

fn main() {
    // Create a temporary directory for our demo
    let temp_dir = TempDir::new().unwrap();

    let binary_content: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        0x49, 0x48, 0x44, 0x52,
    ];

    let binary_path = temp_dir.path().join("binary_example.bin");

    // First, create the file with our binary content
    fs::write(&binary_path, binary_content).unwrap();
    println!("Created binary file with {} bytes", binary_content.len());

    // Test 1: Successful comparison
    println!("\n=== Test 1: Successful binary comparison ===");
    assert_contents(&binary_path, binary_content);
    println!("✓ Binary content comparison successful!");
    println!("Binary data: {:02x?}", binary_content);

    // Test 2: Failed comparison - show what the diff looks like
    println!("\n=== Test 2: Failed binary comparison (different content) ===");
    let different_content: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        0x49, 0x48, 0x44, 0x53, // Changed last byte: 0x52 → 0x53
        0xFF, // Added extra byte
    ];

    println!("Attempting to compare with different binary content...");
    println!("Original: {:02x?}", binary_content);
    println!("Different: {:02x?}", different_content);
    println!(
        "\nExpected output: binary diff showing size and content differences"
    );

    // This will panic and show the binary diff
    assert_contents(&binary_path, different_content);
}
