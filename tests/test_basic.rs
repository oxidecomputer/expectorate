// Copyright 2020 Oxide Computer Company

use expectorate::assert_contents;

#[test]
fn good() {
    let actual = include_str!("data_a.txt");
    assert_contents!("tests/data_a.txt", actual);
}

#[test]
#[should_panic]
fn bad() {
    let actual = include_str!("data_a.txt");
    assert_contents!("tests/data_b.txt", actual);
}
